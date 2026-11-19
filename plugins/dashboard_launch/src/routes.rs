//! Override `GET /dashboard` with the new-tab launchpad.

use lariv_core::define_plugin_routes;

use super::handlers;

define_plugin_routes! {
    plugin: DashboardLaunchTag;
    routes: [
        get WebsiteDashboardAppsRouteTag, "/dashboard", handlers::apps;
    ]
}
