//! `GET /dashboard` — apps launchpad with new-tab tiles.

use lariv_core::apps::AppsCapability;
use lariv_core::components::{SharedChromeFolder, SlotCtx};
use lariv_core::http::Cap;
use lariv_core::web::{Htmx, html_built_page_or_app_layout};
use lariv_plugin_users::middleware::RequireAuth;

use crate::templates::LaunchpadPage;

/// Apps launchpad (requires auth). Tiles open in a new browser tab.
pub async fn apps(
    Cap(catalog): Cap<AppsCapability>,
    Cap(chrome): Cap<SharedChromeFolder>,
    RequireAuth(ctx): RequireAuth,
    htmx: Htmx,
) -> maud::Markup {
    let apps = catalog.visible_apps(&ctx.role);
    let avatar = ctx
        .user
        .name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".into());
    let slot_ctx = SlotCtx::from_auth(&ctx);
    let page = LaunchpadPage {
        name: ctx.user.name.clone(),
        role: ctx.role.clone(),
        avatar,
        apps,
    };
    html_built_page_or_app_layout(&page, &htmx, &chrome, &slot_ctx)
}
