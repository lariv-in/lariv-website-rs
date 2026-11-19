//! Active-company hint for the current task, plus DB access for the switcher.
//!
//! The RLS checkout hook stamps `app.company_id` from [`company_hint`] when set,
//! otherwise from `rls_resolve_active_company()` (prefs / memberships).

use std::future::Future;
use std::sync::OnceLock;

use sea_orm::DatabaseConnection;
use tokio::sync::OnceCell;

tokio::task_local! {
    static COMPANY_HINT: i64;
}

static DB: OnceLock<OnceCell<DatabaseConnection>> = OnceLock::new();

/// Remember the serving pool so the company switcher can list memberships.
pub fn remember_db(conn: DatabaseConnection) {
    let cell = DB.get_or_init(OnceCell::new);
    let _ = cell.set(conn);
}

fn db() -> Option<&'static DatabaseConnection> {
    DB.get()?.get()
}

/// Company the current task wants stamped onto Postgres checkouts.
pub fn company_hint() -> Option<i64> {
    COMPANY_HINT.try_with(|id| *id).ok()
}

/// Run `fut` so pool checkouts prefer this company id.
pub async fn with_company_hint<T>(company_id: i64, fut: impl Future<Output = T>) -> T {
    COMPANY_HINT.scope(company_id, fut).await
}

pub async fn with_company_hint_opt<T>(company_id: Option<i64>, fut: impl Future<Output = T>) -> T {
    match company_id {
        Some(id) => with_company_hint(id, fut).await,
        None => fut.await,
    }
}

pub(crate) async fn memberships_for_switcher(user_id: i64) -> Vec<(i64, String, bool)> {
    let Some(db) = db() else {
        return Vec::new();
    };
    let Ok(memberships) =
        lariv_plugin_companies::logic::list_memberships_for_user(db, user_id).await
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for membership in memberships {
        let name = lariv_plugin_companies::logic::get_company(db, membership.company_id)
            .await
            .map(|c| c.name)
            .unwrap_or_else(|_| format!("Company #{}", membership.company_id));
        rows.push((membership.company_id, name, membership.owned_by));
    }
    rows
}

pub(crate) async fn active_company_label(user_id: i64) -> Option<(i64, String)> {
    let db = db()?;
    let company_id = resolved_company_id(db, user_id).await?;
    let name = lariv_plugin_companies::logic::get_company(db, company_id)
        .await
        .map(|c| c.name)
        .unwrap_or_else(|_| format!("Company #{company_id}"));
    Some((company_id, name))
}

/// Company stamped for this user.
///
/// A saved company wins. A NULL preference falls through to an owned membership,
/// then any membership, matching `rls_resolve_active_company()`.
async fn resolved_company_id(db: &sea_orm::DatabaseConnection, user_id: i64) -> Option<i64> {
    if let Some(id) = lariv_plugin_companies::logic::active_company_for_user(db, user_id)
        .await
        .ok()
        .flatten()
    {
        return Some(id);
    }
    let memberships = lariv_plugin_companies::logic::list_memberships_for_user(db, user_id)
        .await
        .ok()?;
    memberships
        .iter()
        .find(|membership| membership.owned_by)
        .or_else(|| memberships.first())
        .map(|membership| membership.company_id)
}
