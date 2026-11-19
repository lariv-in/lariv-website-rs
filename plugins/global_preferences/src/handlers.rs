//! Load and save the global preferences row.

use axum::response::{IntoResponse, Response};

use lariv_core::components::{SharedChromeFolder, SlotCtx};
use lariv_core::html_form::HtmlFormBody;
use lariv_core::http::Cap;
use lariv_core::web::{Htmx, html_built_page_or_app_layout};
use lariv_plugin_users::middleware::RequireAuth;

use crate::GlobalPreferencesState;
use crate::entities::GlobalPreferences;
use crate::forms::PreferencesForm;
use crate::preferences::{load_preferences, save_preferences};
use crate::routes::GlobalPrefsGetRouteTag;
use crate::templates::GlobalPreferencesPage;

fn page(prefs: &GlobalPreferences, error: String) -> GlobalPreferencesPage {
    GlobalPreferencesPage {
        gemini_api_key: prefs.gemini_api_key.clone(),
        google_search_api_key: prefs.google_search_api_key.clone(),
        google_search_cx: prefs.google_search_cx.clone(),
        error,
    }
}

fn empty_prefs() -> GlobalPreferences {
    GlobalPreferences {
        id: 1,
        created_at: None,
        updated_at: None,
        gemini_api_key: String::new(),
        google_search_api_key: String::new(),
        google_search_cx: String::new(),
    }
}

/// HTTP handler: `get`.
pub async fn get(
    Cap(state): Cap<GlobalPreferencesState>,
    Cap(chrome): Cap<SharedChromeFolder>,
    RequireAuth(ctx): RequireAuth,
    htmx: Htmx,
) -> Response {
    let slot_ctx = SlotCtx::from_auth(&ctx);
    let prefs = match load_preferences(&state.db).await {
        Ok(prefs) => prefs,
        Err(err) => {
            let page = page(&empty_prefs(), err.to_string());
            return html_built_page_or_app_layout(&page, &htmx, &chrome, &slot_ctx).into_response();
        }
    };
    html_built_page_or_app_layout(&page(&prefs, String::new()), &htmx, &chrome, &slot_ctx)
        .into_response()
}

/// HTTP handler: `post`.
pub async fn post(
    Cap(state): Cap<GlobalPreferencesState>,
    Cap(chrome): Cap<SharedChromeFolder>,
    RequireAuth(ctx): RequireAuth,
    htmx: Htmx,
    HtmlFormBody(form): HtmlFormBody<PreferencesForm>,
) -> Response {
    let slot_ctx = SlotCtx::from_auth(&ctx);
    let prefs = GlobalPreferences {
        id: 1,
        created_at: None,
        updated_at: None,
        gemini_api_key: form.gemini_api_key.trim().to_owned(),
        google_search_api_key: form.google_search_api_key.trim().to_owned(),
        google_search_cx: form.google_search_cx.trim().to_owned(),
    };
    match save_preferences(&state.db, prefs.clone()).await {
        Ok(_) => htmx.redirect(&GlobalPrefsGetRouteTag.url()),
        Err(err) => {
            let page = page(&prefs, err.to_string());
            html_built_page_or_app_layout(&page, &htmx, &chrome, &slot_ctx).into_response()
        }
    }
}
