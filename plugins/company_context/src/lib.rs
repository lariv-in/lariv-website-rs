//! Topbar company switcher and task-local active-company context.
//!
//! Company records and memberships stay in `lariv-plugin-companies`. This plugin
//! owns the UI and the hint the RLS checkout hook stamps onto `app.company_id`.

pub mod handlers;
pub mod routes;
pub mod session;
pub mod templates;

pub use session::{company_hint, with_company_hint, with_company_hint_opt};

use frunk::{HCons, hlist::HList};

use lariv_core::app::App;
use lariv_core::capability::CapStore;
use lariv_core::db::{DbCap, DbTag};
use lariv_core::hooks::AttachState;
use lariv_core::plugin_install::define_plugin_install;
use lariv_core::traits::{
    add::{AddCapability, CapTagAbsent},
    get::GetByCapTag,
};

/// Capability tag for the website company-context plugin.
pub struct CompanyContextTag;

/// Marker state. The serving pool lives in a process-local cell for the switcher.
#[derive(Clone, Copy, Default)]
pub struct CompanyContextState;

lariv_core::define_passthrough_cap!(
    CompanyContextStateCap,
    CompanyContextTag,
    CompanyContextState
);

define_plugin_install! {
    plugin: CompanyContextTag;
    steps: [
        slots(templates::SlotsHook),
        cap_hook(lariv_plugin_users::role_authorization::RoleAuthorizationTag, lariv_plugin_users::role_authorization::RoleAuthorizationCap, routes::RoleHook),
        http(routes::Hook),
        state(StateHook),
    ]
}

/// Remember the serving pool so the topbar switcher can list memberships.
#[derive(Clone, Copy, Default)]
pub struct StateHook;

impl<L, DbIdx, TagProof> AttachState<L, (DbIdx, TagProof)> for StateHook
where
    L: GetByCapTag<DbTag, DbIdx, Value = DbCap>,
    L: HList + CapTagAbsent<CompanyContextTag, TagProof>,
{
    type Output = HCons<CompanyContextStateCap, L>;

    fn attach_state(app: App<L>) -> App<Self::Output> {
        let conn = app.get_capability::<DbTag, DbIdx>().items.conn.clone();
        session::remember_db(conn);
        app.add_capability(CapStore::with_items(CompanyContextState))
    }
}
