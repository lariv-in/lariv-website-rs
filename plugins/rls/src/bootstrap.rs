//! Create the non-owner login role and install the session-user checkout hook.

use std::sync::Once;
use std::sync::atomic::{AtomicI64, Ordering};

use futures_util::FutureExt;
use lariv_core::db::{self, db_actor};
use lariv_plugin_users::role_authorization::current_auth;
use sea_orm::sqlx;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use url::Url;

pub const APP_ROLE: &str = "lariv_website_app";

static HOOK: Once = Once::new();

/// Admin id used for pool checkouts while serve startup is still running.
///
/// mdBook creates its directories before any request exists. The insert policy
/// requires `app.user_id` and `app.company_id`. Cleared once startup hooks have
/// finished, so an anonymous request does not keep those ids.
static STARTUP_USER: AtomicI64 = AtomicI64::new(0);
static STARTUP_COMPANY: AtomicI64 = AtomicI64::new(0);

pub fn set_startup_user(id: i64) {
    STARTUP_USER.store(id, Ordering::Release);
}

pub fn set_startup_company(id: i64) {
    STARTUP_COMPANY.store(id, Ordering::Release);
}

pub fn clear_startup_user() {
    STARTUP_USER.store(0, Ordering::Release);
    STARTUP_COMPANY.store(0, Ordering::Release);
}

fn startup_user() -> Option<i64> {
    let id = STARTUP_USER.load(Ordering::Acquire);
    if id == 0 { None } else { Some(id) }
}

fn startup_company() -> Option<i64> {
    let id = STARTUP_COMPANY.load(Ordering::Acquire);
    if id == 0 { None } else { Some(id) }
}

/// Stamp `app.user_id` on every pooled Postgres connection.
///
/// `SET LOCAL` is a no-op outside a transaction, and most handlers run autocommit
/// queries. A session setting overwritten on the next checkout covers both.
pub fn install_session_hook() {
    HOOK.call_once(|| {
        db::set_db_actor_probe(|| current_auth().map(|auth| auth.user.id));
        db::set_postgres_before_acquire(Some(session_hook));
    });
}

/// Create `lariv_website_app` from the admin URL before the serving pool connects.
pub async fn prepare_database(config_path: &str) -> anyhow::Result<()> {
    install_session_hook();
    let raw = std::fs::read_to_string(config_path)?;
    let root: toml::Value = toml::from_str(&raw)?;
    let mut admin_url = root
        .get("admin_database_url")
        .and_then(toml::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut app_url = root
        .get("database_url")
        .and_then(toml::Value::as_str)
        .unwrap_or("")
        .to_owned();
    if let Ok(url) = std::env::var("ADMIN_DATABASE_URL") {
        if !url.is_empty() {
            admin_url = url;
        }
    }
    if let Ok(url) = std::env::var("DATABASE_URL") {
        if !url.is_empty() {
            app_url = url;
        }
    }
    if admin_url.is_empty() {
        anyhow::bail!("admin_database_url is required for row-level security");
    }
    if app_url.is_empty() {
        anyhow::bail!("database_url is required for row-level security");
    }
    ensure_login_role(&admin_url, &app_url).await
}

pub async fn ensure_login_role(admin_url: &str, app_url: &str) -> anyhow::Result<()> {
    let app = Url::parse(app_url)?;
    let username = percent_decode(app.username());
    if username != APP_ROLE {
        anyhow::bail!("database_url user must be {APP_ROLE}, got {username}");
    }
    let password = percent_decode(app.password().unwrap_or(""));
    let database = app
        .path()
        .trim_start_matches('/')
        .split('?')
        .next()
        .unwrap_or("")
        .to_owned();
    if database.is_empty() {
        anyhow::bail!("database_url is missing a database name");
    }
    let db = app_url_parts_ok(&username)?;
    let _ = db;
    let admin = sea_orm::Database::connect(admin_url).await?;
    if password.contains("$lariv_rls_role$") {
        anyhow::bail!("app role password cannot contain the setup delimiter");
    }
    let create_role = format!(
        "DO $lariv_rls_role$ BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = {role}) THEN
                CREATE ROLE {role_ident} LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD {password};
            ELSE
                ALTER ROLE {role_ident} WITH LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD {password};
            END IF;
        END $lariv_rls_role$;",
        role = quote_literal(APP_ROLE),
        role_ident = quote_ident(APP_ROLE),
        password = quote_literal(&password),
    );
    admin
        .execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            create_role,
        ))
        .await?;
    let role = quote_ident(APP_ROLE);
    let database = quote_ident(&database);
    admin
        .execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            format!("GRANT CONNECT ON DATABASE {database} TO {role}"),
        ))
        .await?;
    admin
        .execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            format!("GRANT USAGE ON SCHEMA public TO {role}"),
        ))
        .await?;
    Ok(())
}

fn app_url_parts_ok(username: &str) -> anyhow::Result<()> {
    if !username
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        anyhow::bail!("refusing to create a role named {username}");
    }
    Ok(())
}

fn session_hook<'c>(
    conn: &'c mut sqlx::postgres::PgConnection,
    _meta: sqlx::pool::PoolConnectionMetadata,
) -> futures_util::future::BoxFuture<'c, Result<bool, sqlx::Error>> {
    async move {
        let actor = db_actor()
            .or_else(|| current_auth().map(|auth| auth.user.id))
            .or_else(startup_user);
        let user_value = actor.map(|id| id.to_string()).unwrap_or_default();
        sqlx::query("SELECT set_config('app.user_id', $1, false)")
            .bind(user_value)
            .execute(&mut *conn)
            .await?;
        // Prefer an in-task hint (background jobs), then serve-startup, then SQL
        // resolution from memberships / prefs now that app.user_id is set.
        let company_value = if let Some(id) =
            website_plugin_company_context::company_hint().or_else(startup_company)
        {
            id.to_string()
        } else {
            let row: Option<(Option<i64>,)> =
                sea_orm::sqlx::query_as("SELECT rls_resolve_active_company()")
                    .fetch_optional(&mut *conn)
                    .await?;
            row.and_then(|(id,)| id)
                .map(|id| id.to_string())
                .unwrap_or_default()
        };
        sqlx::query("SELECT set_config('app.company_id', $1, false)")
            .bind(company_value)
            .execute(&mut *conn)
            .await?;
        // Admin DB role always bypasses. Seed / serve-startup also bypass while the
        // startup actor is set so migration-like DML is not blocked by policies.
        let role: (String,) = sea_orm::sqlx::query_as("SELECT current_user")
            .fetch_one(&mut *conn)
            .await?;
        let bypass = role.0 != APP_ROLE || startup_user().is_some();
        sqlx::query("SELECT set_config('app.rls_bypass', $1, false)")
            .bind(if bypass { "1" } else { "" })
            .execute(&mut *conn)
            .await?;
        Ok(true)
    }
    .boxed()
}

fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
