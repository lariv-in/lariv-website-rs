//! Site-wide Gemini and Google Search credentials.
//!
//! Superusers edit one row at `/dashboard/global-preferences`. Chat and search
//! use a user's own assistant preferences when those fields are set, and these
//! values when they are not.

pub mod apps;
pub mod entities;
pub mod forms;
pub mod handlers;
pub mod migrations;
pub mod preferences;
pub mod routes;
pub mod templates;

use frunk::{HCons, HNil, hlist::HList};
use sea_orm::DatabaseConnection;

use lariv_core::app::App;
use lariv_core::capability::CapStore;
use lariv_core::config::{AppConfig, AppConfigTag, ConfigCap, ConfigTag};
use lariv_core::db::{DbCap, DbTag};
use lariv_core::hooks::AttachState;
use lariv_core::plugin_install::define_plugin_install;
use lariv_core::traits::{
    add::{AddCapability, CapTagAbsent},
    get::{GetByCapTag, GetByTag},
};

/// Capability tag for the global preferences plugin.
pub struct GlobalPreferencesTag;

/// Serving pool for the preferences page. Fallbacks read through the admin pool.
#[derive(Clone)]
pub struct GlobalPreferencesState {
    pub db: DatabaseConnection,
}

lariv_core::define_passthrough_cap!(
    GlobalPreferencesStateCap,
    GlobalPreferencesTag,
    GlobalPreferencesState
);

define_plugin_install! {
    plugin: GlobalPreferencesTag;
    steps: [
        apps(apps::Hook),
        migrations(migrations::Hook),
        templates(templates::Hook),
        cap_hook(lariv_plugin_users::role_authorization::RoleAuthorizationTag, lariv_plugin_users::role_authorization::RoleAuthorizationCap, routes::RoleHook),
        http(routes::Hook),
        state(StateHook),
    ]
}

/// Remember the serving pool and register the API-key fallback.
#[derive(Clone, Copy, Default)]
pub struct StateHook;

impl<L, DbIdx, CfgIdx, Configs, AppCfgIdx, TagProof>
    AttachState<L, (DbIdx, CfgIdx, Configs, AppCfgIdx, TagProof)> for StateHook
where
    L: GetByCapTag<DbTag, DbIdx, Value = DbCap>,
    L: GetByCapTag<ConfigTag, CfgIdx, Value = ConfigCap<HNil, Configs>>,
    Configs: GetByTag<AppConfigTag, AppCfgIdx, Value = AppConfig>,
    L: HList + CapTagAbsent<GlobalPreferencesTag, TagProof>,
{
    type Output = HCons<GlobalPreferencesStateCap, L>;

    fn attach_state(app: App<L>) -> App<Self::Output> {
        let configs = &app.get_capability::<ConfigTag, CfgIdx>().items;
        let app_cfg = <Configs as GetByTag<AppConfigTag, AppCfgIdx>>::get_by_tag(configs);
        preferences::register(app_cfg.database_url.clone());
        let db = app.get_capability::<DbTag, DbIdx>().items.conn.clone();
        app.add_capability(CapStore::with_items(GlobalPreferencesState { db }))
    }
}
