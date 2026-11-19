//! Deployment wiring for the LLM assistant and company app access.
//!
//! The framework plugin owns chat, tools, and routes. This plugin owns the pieces
//! that depend on this website's row-level security: loading preferences as the
//! admin role, and opening Accounting and the linked apps (including the
//! assistant) to company roles. Site-wide API key fallbacks live in the global
//! preferences app.

mod access;
mod migrations;
mod preferences;
mod sidebar;

/// Drop the old Gemini key copy trigger after row-level security is installed.
pub use migrations::GEMINI_SQL;

use frunk::{HCons, HNil, hlist::HList};
use lariv_core::app::App;
use lariv_core::capability::CapStore;
use lariv_core::config::{AppConfig, AppConfigTag, ConfigCap, ConfigTag};
use lariv_core::hooks::AttachState;
use lariv_core::plugin_install::define_plugin_install;
use lariv_core::traits::{
    add::{AddCapability, CapTagAbsent},
    get::{GetByCapTag, GetByTag},
};

/// Capability tag for this deployment's assistant wiring.
pub struct AssistantDeployTag;

/// Marker state. The admin pool lives in a process-local cell, not on the request.
#[derive(Clone, Copy, Default)]
pub struct AssistantDeployState;

lariv_core::define_passthrough_cap!(
    AssistantDeployStateCap,
    AssistantDeployTag,
    AssistantDeployState
);

define_plugin_install! {
    plugin: AssistantDeployTag;
    steps: [
        migrations(migrations::Hook),
        apps(access::AppsHook),
        cap_hook(lariv_plugin_users::role_authorization::RoleAuthorizationTag, lariv_plugin_users::role_authorization::RoleAuthorizationCap, access::RoleHook),
        cap_hook(lariv_plugin_finance_accounts::accounting_sidebar::AccountingSidebarTag, lariv_plugin_finance_accounts::accounting_sidebar::AccountingSidebarCap, sidebar::Hook),
        state(StateHook),
    ]
}

/// Remember the admin connection and register the preferences loader.
#[derive(Clone, Copy, Default)]
pub struct StateHook;

impl<L, CfgIdx, Configs, AppCfgIdx, TagProof> AttachState<L, (CfgIdx, Configs, AppCfgIdx, TagProof)>
    for StateHook
where
    L: GetByCapTag<ConfigTag, CfgIdx, Value = ConfigCap<HNil, Configs>>,
    Configs: GetByTag<AppConfigTag, AppCfgIdx, Value = AppConfig>,
    L: HList + CapTagAbsent<AssistantDeployTag, TagProof>,
{
    type Output = HCons<AssistantDeployStateCap, L>;

    fn attach_state(app: App<L>) -> App<Self::Output> {
        let configs = &app.get_capability::<ConfigTag, CfgIdx>().items;
        let app_cfg = <Configs as GetByTag<AppConfigTag, AppCfgIdx>>::get_by_tag(configs);
        if let Some(admin_url) = app_cfg
            .admin_database_url
            .as_deref()
            .filter(|url| !url.is_empty())
        {
            preferences::register(admin_url.to_owned());
        } else {
            tracing::warn!(
                "admin_database_url is unset; assistant background preferences are inactive"
            );
        }
        app.add_capability(CapStore::with_items(AssistantDeployState))
    }
}
