//! Global Preferences dashboard tile. Empty role list: superuser only.

use lariv_core::apps::define_register_apps;

define_register_apps! {
    plugin: GlobalPreferencesTag;
    key: "p_global_preferences";
    name: "Global Preferences";
    href: crate::routes::GlobalPrefsGetRouteTag.url();
    icon: "key";
    roles: [];
}
