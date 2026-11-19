//! Postgres row-level security for this website deployment.
//!
//! Call [`prepare_database`] before [`lariv_core::app::App::load_config`] so the serving
//! pool logs in as `lariv_website_app` and stamps `app.user_id` on every checkout.
//! Migrations use `admin_database_url` and are not subject to the policies.

mod bootstrap;
mod catalog;
mod directory;
mod indian;
mod migrations;

pub use bootstrap::{ensure_login_role, prepare_database};
pub use catalog::{clone_catalogs, collapse_duplicate_catalog};
pub use indian::{ensure_indian_defaults, ensure_indian_defaults_for_all};
pub use migrations::install_sql;

use frunk::{HCons, HNil, hlist::HList};
use lariv_core::app::{App, MountedApp};
use lariv_core::capability::CapStore;
use lariv_core::config::{AppConfig, AppConfigTag, ConfigCap, ConfigTag};
use lariv_core::hooks::{AttachState, RunSeed, RunServeStartup};
use lariv_core::plugin_install::define_plugin_install;
use lariv_core::traits::{
    add::{AddCapability, CapTagAbsent},
    get::{GetByCapTag, GetByTag},
};
use lariv_plugin_users::config::{UsersConfig, UsersConfigTag};

/// Capability tag for the row-level security plugin.
pub struct RlsTag;

/// Marker state. The admin pool lives in a process-local cell, not on the request.
#[derive(Clone, Copy, Default)]
pub struct RlsState;

lariv_core::define_passthrough_cap!(RlsStateCap, RlsTag, RlsState);

define_plugin_install! {
    plugin: RlsTag;
    steps: [
        migrations(migrations::Hook),
        state(StateHook),
        seeds(SeedsHook),
        // Last install, so this runs after the other plugins' startup hooks.
        serve_startup(ServeStartupHook),
    ]
}

/// Remember the admin connection and register the users directory.
#[derive(Clone, Copy, Default)]
pub struct StateHook;

impl<L, CfgIdx, Configs, AppCfgIdx, UsersCfgIdx, TagProof>
    AttachState<L, (CfgIdx, Configs, AppCfgIdx, UsersCfgIdx, TagProof)> for StateHook
where
    L: GetByCapTag<ConfigTag, CfgIdx, Value = ConfigCap<HNil, Configs>>,
    Configs: GetByTag<AppConfigTag, AppCfgIdx, Value = AppConfig>,
    Configs: GetByTag<UsersConfigTag, UsersCfgIdx, Value = UsersConfig>,
    L: HList + CapTagAbsent<RlsTag, TagProof>,
{
    type Output = HCons<RlsStateCap, L>;

    fn attach_state(app: App<L>) -> App<Self::Output> {
        let configs = &app.get_capability::<ConfigTag, CfgIdx>().items;
        let app_cfg = <Configs as GetByTag<AppConfigTag, AppCfgIdx>>::get_by_tag(configs);
        let users_cfg = <Configs as GetByTag<UsersConfigTag, UsersCfgIdx>>::get_by_tag(configs);
        if let Some(admin_url) = app_cfg
            .admin_database_url
            .as_deref()
            .filter(|url| !url.is_empty())
        {
            directory::register(admin_url.to_owned(), users_cfg.admin_email.clone());
        } else {
            tracing::warn!("admin_database_url is unset; row-level security directory is inactive");
        }
        app.add_capability(CapStore::with_items(RlsState))
    }
}

/// Give the seeded admin ownership of rows that nobody owns yet.
#[derive(Clone, Copy, Default)]
pub struct SeedsHook;

#[async_trait::async_trait]
impl<M> RunSeed<M, ()> for SeedsHook
where
    M: Sync,
{
    async fn run_seed(_app: &MountedApp<M>) -> anyhow::Result<()> {
        directory::backfill_admin().await
    }
}

/// Drop the startup actor once every earlier serve-startup hook has finished.
#[derive(Clone, Copy, Default)]
pub struct ServeStartupHook;

#[async_trait::async_trait]
impl<M> RunServeStartup<M, ()> for ServeStartupHook
where
    M: Sync,
{
    async fn run_serve_startup(_app: &MountedApp<M>) -> anyhow::Result<()> {
        bootstrap::clear_startup_user();
        Ok(())
    }
}

/// Register the checkout hook. [`prepare_database`] does this before the pool opens.
pub fn install_session_hook() {
    bootstrap::install_session_hook();
}
