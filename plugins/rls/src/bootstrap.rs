//! Stamp the request user on every pooled connection.
//!
//! The process uses one database login: whoever `database_url` names. Boot sets
//! `app.rls_bypass` until serve startup finishes. Request checkouts do not.
//! A superuser login skips policies, so those request checkouts `SET ROLE lariv_rls`.

use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use futures_util::FutureExt;
use lariv_core::db::{self, db_actor};
use lariv_plugin_users::role_authorization::current_auth;
use sea_orm::sqlx;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

static HOOK: Once = Once::new();

/// Migrations, seeds, and serve-startup run before any request. Cleared by the
/// row-level security serve-startup hook, which is installed last.
static BOOT_PRIVILEGED: AtomicBool = AtomicBool::new(true);

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

/// Request traffic is subject to the policies. Call after serve startup.
pub fn end_boot_privilege() {
    BOOT_PRIVILEGED.store(false, Ordering::Release);
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

/// Install the checkout hook before [`lariv_core::app::App::load_config`] opens the pool.
pub async fn prepare_database() -> anyhow::Result<()> {
    install_session_hook();
    Ok(())
}

/// Pool that may read every company. Login, catalog clone, and background jobs
/// use it. The serving pool does not.
pub async fn connect_privileged(url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(url.to_owned());
    opt.max_connections(10)
        .min_connections(1)
        .max_lifetime(Duration::from_secs(3600))
        .idle_timeout(Duration::from_secs(900))
        .sqlx_logging(false);
    opt.map_sqlx_postgres_before_acquire(|conn, _meta| {
        async move {
            // Stay the login role. Request pools switch away from a superuser.
            sqlx::query("RESET ROLE").execute(&mut *conn).await?;
            sqlx::query("SELECT set_config('app.rls_bypass', '1', false)")
                .execute(&mut *conn)
                .await?;
            Ok(true)
        }
        .boxed()
    });
    Database::connect(opt).await
}

fn session_hook<'c>(
    conn: &'c mut sqlx::postgres::PgConnection,
    _meta: sqlx::pool::PoolConnectionMetadata,
) -> futures_util::future::BoxFuture<'c, Result<bool, sqlx::Error>> {
    async move {
        // Set bypass before the company lookup. That query reads memberships, and
        // boot still has to see every row. A request leaves the setting empty.
        let bypass = BOOT_PRIVILEGED.load(Ordering::Acquire) || startup_user().is_some();
        // Superusers never consult policies. Request work has to run as lariv_rls,
        // which install.sql creates without SUPERUSER or BYPASSRLS.
        assume_subject_role(&mut *conn, bypass).await?;
        sqlx::query("SELECT set_config('app.rls_bypass', $1, false)")
            .bind(if bypass { "1" } else { "" })
            .execute(&mut *conn)
            .await?;
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
        Ok(true)
    }
    .boxed()
}

/// Request sessions must not be a role that skips row-level security.
///
/// `RESET ROLE` on boot and on the privileged pool keeps migrations on the
/// login role. A login that already has neither superuser nor `BYPASSRLS` is
/// left as itself.
async fn assume_subject_role(
    conn: &mut sqlx::postgres::PgConnection,
    bypass: bool,
) -> Result<(), sqlx::Error> {
    if bypass {
        sqlx::query("RESET ROLE").execute(&mut *conn).await?;
        return Ok(());
    }
    let (skips_rls,): (bool,) = sqlx::query_as(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = session_user",
    )
    .fetch_one(&mut *conn)
    .await?;
    if skips_rls {
        sqlx::query("SET ROLE lariv_rls")
            .execute(&mut *conn)
            .await?;
    } else {
        sqlx::query("RESET ROLE").execute(&mut *conn).await?;
    }
    Ok(())
}
