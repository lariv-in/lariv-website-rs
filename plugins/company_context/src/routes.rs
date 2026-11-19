use lariv_plugin_companies::routes::CompaniesView;
use lariv_plugin_users::roles::{Admin, Unassigned};

use super::handlers;

lariv_core::define_plugin_routes! {
    plugin: CompanyContextTag;
    prefix: "/dashboard";
    routes: [
        post CompanySwitchPostRouteTag, "/companies/switch/{id}", bare handlers::switch_post, redirect, authorize(CompaniesView, [Unassigned, Admin]);
        post CompanyClearPostRouteTag, "/companies/switch/clear", bare handlers::clear_company_post, redirect, authorize(CompaniesView, [Unassigned, Admin]);
    ]
}
