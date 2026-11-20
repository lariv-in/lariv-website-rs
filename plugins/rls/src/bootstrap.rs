//! Stamp the request user on every pooled connection.
//!
//! The process uses one database login: whoever `database_url` names. Boot sets
//! `app.rls_bypass` until serve startup finishes. Request checkouts do not.
//! The installing login owns the tables. Request checkouts `SET ROLE lariv_rls`
//! so that login stays exempt (Cloud SQL is not a superuser; FORCE RLS would
//! otherwise apply to every request) while handler queries stay under the policies.

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
        // Stay the login role while stamping GUCs and resolving the company.
        // load_config opens the pool before migrations install the RLS functions,
        // and lariv_rls only exists after install.sql. Request traffic switches
        // to lariv_rls after the stamps so policies apply to handler queries.
        let bypass = BOOT_PRIVILEGED.load(Ordering::Acquire) || startup_user().is_some();
        sqlx::query("RESET ROLE").execute(&mut *conn).await?;
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
            resolve_active_company(&mut *conn).await?
        };
        sqlx::query("SELECT set_config('app.company_id', $1, false)")
            .bind(company_value)
            .execute(&mut *conn)
            .await?;
        assume_subject_role(&mut *conn, bypass).await?;
        Ok(true)
    }
    .boxed()
}

/// Resolve `app.company_id` from prefs / memberships.
///
/// Missing function (pool opened before `install.sql`) leaves the company unset
/// instead of failing every checkout.
async fn resolve_active_company(
    conn: &mut sqlx::postgres::PgConnection,
) -> Result<String, sqlx::Error> {
    match sea_orm::sqlx::query_as::<_, (Option<i64>,)>("SELECT public.rls_resolve_active_company()")
        .fetch_optional(&mut *conn)
        .await
    {
        Ok(row) => Ok(row
            .and_then(|(id,)| id)
            .map(|id| id.to_string())
            .unwrap_or_default()),
        Err(err) if missing_rls_function(&err) => Ok(String::new()),
        Err(err) => Err(err),
    }
}

fn missing_rls_function(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            db.code().is_some_and(|code| code == "42883")
                || db.message().contains("rls_resolve_active_company")
                    && db.message().contains("does not exist")
        }
        _ => false,
    }
}

/// Request sessions run as `lariv_rls`, which does not own the tables.
///
/// `RESET ROLE` on boot and on the privileged pool keeps migrations on the
/// installing login. That login is exempt: a superuser skips policies, and
/// Cloud SQL's non-superuser owner is treated as bypass by `rls_is_bypass`.
/// Leaving a request on that login would exempt it too.
async fn assume_subject_role(
    conn: &mut sqlx::postgres::PgConnection,
    bypass: bool,
) -> Result<(), sqlx::Error> {
    if bypass {
        sqlx::query("RESET ROLE").execute(&mut *conn).await?;
        return Ok(());
    }
    sqlx::query("SET ROLE lariv_rls").execute(&mut *conn).await?;
    Ok(())
}
