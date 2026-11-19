//! Global preferences page. Empty allowlist: superuser only.

use lariv_core::define_plugin_routes;

use super::handlers;

/// Who may edit site-wide API keys. Empty extra roles: superuser only.
pub struct GlobalPrefsAdmin;

define_plugin_routes! {
    plugin: GlobalPreferencesTag;
    prefix: "/dashboard";
    routes: [
        get GlobalPrefsGetRouteTag, "/global-preferences", handlers::get, authorize(GlobalPrefsAdmin, []);
        post GlobalPrefsPostRouteTag, "/global-preferences", handlers::post, authorize(GlobalPrefsAdmin, []);
    ]
}
