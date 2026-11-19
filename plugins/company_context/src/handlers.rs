//! Switch and clear the active company for the current user.

use axum::{
    extract::Path,
    response::{IntoResponse, Redirect, Response},
};
use lariv_core::http::Cap;
use lariv_core::web::Htmx;
use lariv_plugin_companies::logic;
use lariv_plugin_companies::routes::{CompanyDefaultRouteTag, CompanyDetailRouteTag};
use lariv_plugin_companies::state::CompaniesState;
use lariv_plugin_users::middleware::RequireAuth;
use lariv_plugin_users::roles::Superuser;

pub async fn switch_post(
    Cap(state): Cap<CompaniesState>,
    RequireAuth(ctx): RequireAuth,
    htmx: Htmx,
    Path(id): Path<i64>,
) -> Response {
    if let Err(err) = logic::set_active_company(&state.db, ctx.user.id, Some(id)).await {
        tracing::error!(error = %err, company_id = id, "switch company");
        return Redirect::to(&CompanyDefaultRouteTag.url()).into_response();
    }
    htmx.redirect(&CompanyDetailRouteTag::new(id).url())
}

pub async fn clear_company_post(
    Cap(state): Cap<CompaniesState>,
    RequireAuth(ctx): RequireAuth,
    htmx: Htmx,
) -> Response {
    if !Superuser::matches(&ctx.role) {
        return Redirect::to(&CompanyDefaultRouteTag.url()).into_response();
    }
    if let Err(err) = logic::set_active_company(&state.db, ctx.user.id, None).await {
        tracing::error!(error = %err, "clear active company");
        return Redirect::to(&CompanyDefaultRouteTag.url()).into_response();
    }
    htmx.redirect(&CompanyDefaultRouteTag.url())
}
