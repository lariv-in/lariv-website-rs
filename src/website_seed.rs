//! Idempotent seed for the Lariv marketing site, static media, and Custom theme.
//!
//! Registered as a [`lariv_rs::hooks::RunSeed`] hook so it runs only for `seed`, not `serve`.

use chrono::Utc;
use lariv_rs::app::MountedApp;
use lariv_rs::hooks::RunSeed;
use lariv_rs::plugin_install::define_plugin_install;
use lariv_rs::plugins::filesystem::node::{self, NodeFile};
use lariv_rs::plugins::filesystem::storage::DynFilestore;
use lariv_rs::plugins::website::{
    WebsiteTag,
    builder_assets::public_asset_url,
    entities::{
        WebsitePreferences,
        db_route::{self, Column as DbRouteColumn, Entity as DbRouteEntity},
        route_reference::{self, Column as RouteRefColumn, Entity as RouteRefEntity},
    },
    preferences::{self, CUSTOM_THEME_ID},
    render,
    state::WebsiteState,
};
use lariv_rs::traits::get::GetByTag;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tokio::io::AsyncReadExt;

/// Hook identity for the deployment-local website seed (distinct from [`WebsiteTag`] state).
pub struct LarivWebsiteSeedTag;

define_plugin_install! {
    plugin: LarivWebsiteSeedTag;
    /// Queue homepage/media seed for the `seed` CLI command.
    steps: [seeds(SeedsHook)]
}

/// Runs [`ensure_site`] when seed hooks execute.
#[derive(Clone, Copy, Default)]
pub struct SeedsHook;

#[async_trait::async_trait]
impl<M, WebsiteIdx> RunSeed<M, WebsiteIdx> for SeedsHook
where
    M: GetByTag<WebsiteTag, WebsiteIdx, Value = WebsiteState> + Sync,
{
    async fn run_seed(app: &MountedApp<M>) -> anyhow::Result<()> {
        tracing::info!("lariv website: seeding pages, theme, and media");
        ensure_site(app.get_capability_output::<WebsiteTag, WebsiteIdx>()).await?;
        tracing::info!("lariv website: seed complete");
        Ok(())
    }
}

const THEME_CSS: &[u8] = include_bytes!("../assets/theme/lariv.css");
const THEME_JS: &[u8] = include_bytes!("../assets/theme/lariv.js");
const HEADER_HTML: &str = include_str!("../assets/header.html");
const FOOTER_HTML: &str = include_str!("../assets/footer.html");
const THEME_CSS_NAME: &str = "lariv.css";
const THEME_JS_NAME: &str = "lariv.js";
const THEME: &str = CUSTOM_THEME_ID;

struct StaticAsset {
    name: &'static str,
    bytes: &'static [u8],
}

const STATIC_ASSETS: &[StaticAsset] = &[
    StaticAsset {
        name: "logo.svg",
        bytes: include_bytes!("../assets/static/logo.svg"),
    },
    StaticAsset {
        name: "main-artwork.svg",
        bytes: include_bytes!("../assets/static/main-artwork.svg"),
    },
    StaticAsset {
        name: "benchmark_metrics.json",
        bytes: include_bytes!("../assets/static/benchmark_metrics.json"),
    },
];

struct PageSeed {
    name: &'static str,
    html: &'static str,
    path: &'static str,
    dir: &'static [&'static str],
}

const HOMEPAGE_HTML: &str = include_str!("../assets/pages/index.html");
const WEBSITE_DIR: &[&str] = &["website"];
const PAGES_DIR: &[&str] = &["website", "pages"];

const PAGES: &[PageSeed] = &[
    PageSeed {
        name: "index.html",
        html: HOMEPAGE_HTML,
        path: "/",
        dir: PAGES_DIR,
    },
    PageSeed {
        name: "landing.html",
        html: HOMEPAGE_HTML,
        path: "/",
        dir: WEBSITE_DIR,
    },
    PageSeed {
        name: "pricing.html",
        html: include_str!("../assets/pages/pricing.html"),
        path: "/pricing",
        dir: PAGES_DIR,
    },
    PageSeed {
        name: "blogs.html",
        html: include_str!("../assets/pages/blogs.html"),
        path: "/blogs",
        dir: PAGES_DIR,
    },
    PageSeed {
        name: "blogs_slug.html",
        html: include_str!("../assets/pages/blogs_slug.html"),
        path: "/blogs/*",
        dir: PAGES_DIR,
    },
    PageSeed {
        name: "privacy-policy.html",
        html: include_str!("../assets/pages/privacy-policy.html"),
        path: "/privacy-policy",
        dir: PAGES_DIR,
    },
];

/// Go-site filenames at `website/*.html`. Seeded last so existing `/` routes
/// that still point at `landing.html` pick up Minijinja markup in place.
const GO_COMPAT_PAGES: &[PageSeed] = &[
    PageSeed {
        name: "pricing.html",
        html: include_str!("../assets/pages/pricing.html"),
        path: "/pricing",
        dir: WEBSITE_DIR,
    },
    PageSeed {
        name: "blogs.html",
        html: include_str!("../assets/pages/blogs.html"),
        path: "/blogs",
        dir: WEBSITE_DIR,
    },
    PageSeed {
        name: "blogs_slug.html",
        html: include_str!("../assets/pages/blogs_slug.html"),
        path: "/blogs/*",
        dir: WEBSITE_DIR,
    },
    PageSeed {
        name: "privacy-policy.html",
        html: include_str!("../assets/pages/privacy-policy.html"),
        path: "/privacy-policy",
        dir: WEBSITE_DIR,
    },
];

pub async fn ensure_site(state: &WebsiteState) -> anyhow::Result<()> {
    ensure_site_state(&state.db, state.store.as_ref()).await
}

async fn ensure_site_state(db: &DatabaseConnection, store: &DynFilestore) -> anyhow::Result<()> {
    ensure_custom_theme(db, store).await?;
    let media_urls = ensure_static_assets(db, store).await?;
    let (header, header_rewritten) =
        ensure_named_html(db, store, &["website".into()], "header.html", HEADER_HTML, &media_urls)
            .await?;
    let (footer, footer_rewritten) =
        ensure_named_html(db, store, &["website".into()], "footer.html", FOOTER_HTML, &media_urls)
            .await?;
    let refs_changed = header_rewritten || footer_rewritten;

    for page in PAGES.iter().chain(GO_COMPAT_PAGES.iter()) {
        let dir: Vec<String> = page.dir.iter().map(|s| (*s).to_string()).collect();
        let (vnode, page_rewritten) = ensure_named_html(
            db,
            store,
            &dir,
            page.name,
            page.html,
            &media_urls,
        )
        .await?;
        let route_id =
            ensure_db_route(db, page.path, vnode.id, THEME, page_rewritten || refs_changed).await?;
        ensure_route_refs(db, route_id, &[header.id, footer.id]).await?;
        tracing::info!(
            path = page.path,
            page_id = vnode.id,
            route_id,
            name = page.name,
            "lariv website: page route ready"
        );
    }
    Ok(())
}

async fn ensure_named_html(
    db: &DatabaseConnection,
    store: &DynFilestore,
    segments: &[String],
    name: &str,
    html: &str,
    media_urls: &[(String, String)],
) -> anyhow::Result<(lariv_rs::plugins::filesystem::entities::VNode, bool)> {
    let rewritten = html_with_media_urls(html, media_urls);
    let parent_id = node::ensure_directory_path(db, store, None, segments)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let parent = match parent_id {
        Some(id) => match node::get_by_id(db, id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, name, "get node by id for html parent");
                None
            }
        },
        None => None,
    };
    ensure_file_vnode(db, store, parent_id, parent.as_ref(), name, rewritten.as_bytes()).await
}

/// Seeds theme CSS/JS under `website/themes/` and points Custom theme preferences at them.
async fn ensure_custom_theme(db: &DatabaseConnection, store: &DynFilestore) -> anyhow::Result<()> {
    let segments = ["website".into(), "themes".into()];
    let parent_id = node::ensure_directory_path(db, store, None, &segments)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let parent = match parent_id {
        Some(id) => match node::get_by_id(db, id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "get node by id for website themes parent");
                None
            }
        },
        None => None,
    };

    let css = ensure_file_vnode(
        db,
        store,
        parent_id,
        parent.as_ref(),
        THEME_CSS_NAME,
        THEME_CSS,
    )
    .await?
    .0;
    let js = ensure_file_vnode(
        db,
        store,
        parent_id,
        parent.as_ref(),
        THEME_JS_NAME,
        THEME_JS,
    )
    .await?
    .0;

    preferences::save_preferences(
        db,
        WebsitePreferences {
            id: 1,
            created_at: None,
            updated_at: None,
            custom_theme_css_vnode_id: Some(css.id),
            custom_theme_js_vnode_id: Some(js.id),
        },
    )
    .await?;

    tracing::info!(
        css_vnode_id = css.id,
        js_vnode_id = js.id,
        "lariv website: custom theme preferences ready"
    );
    Ok(())
}

fn html_with_media_urls(html: &str, urls: &[(String, String)]) -> String {
    let mut out = html.to_string();
    for (name, url) in urls {
        out = out.replace(&format!("/static/{name}"), url);
    }
    out
}

/// Seeds blobs + `/static/{name}` aliases. Returns `(filename, /media/{id}/)` pairs
/// so pages can use the website plugin's public asset route instead of the
/// catch-all (which production proxies often intercept for `/static/`).
async fn ensure_static_assets(
    db: &DatabaseConnection,
    store: &DynFilestore,
) -> anyhow::Result<Vec<(String, String)>> {
    let segments = ["website".into(), "static".into()];
    let parent_id = node::ensure_directory_path(db, store, None, &segments)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let parent = match parent_id {
        Some(id) => match node::get_by_id(db, id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "get node by id for website static parent");
                None
            }
        },
        None => None,
    };

    let mut urls = Vec::with_capacity(STATIC_ASSETS.len());
    for asset in STATIC_ASSETS {
        let vnode = ensure_file_vnode(
            db,
            store,
            parent_id,
            parent.as_ref(),
            asset.name,
            asset.bytes,
        )
        .await?
        .0;
        let media_url = public_asset_url(vnode.id);
        tracing::info!(
            name = asset.name,
            vnode_id = vnode.id,
            media_url = %media_url,
            bytes = asset.bytes.len(),
            "lariv website: static asset ready"
        );
        ensure_db_route(db, &format!("/static/{}", asset.name), vnode.id, "", false).await?;
        urls.push((asset.name.to_string(), media_url));
    }
    Ok(urls)
}

async fn ensure_file_vnode(
    db: &DatabaseConnection,
    store: &DynFilestore,
    parent_id: Option<i64>,
    parent: Option<&lariv_rs::plugins::filesystem::entities::VNode>,
    name: &str,
    bytes: &[u8],
) -> anyhow::Result<(lariv_rs::plugins::filesystem::entities::VNode, bool)> {
    if let Some(existing) = node::find_child(db, parent_id, name, false).await? {
        if vnode_bytes_match(store, &existing, bytes).await? {
            return Ok((existing, false));
        }
        tracing::warn!(
            name,
            vnode_id = existing.id,
            stored_path = existing.file_path.as_deref().unwrap_or(""),
            "lariv website: rewriting vnode blob"
        );
        let updated = render::replace_vnode_content(db, store, existing, bytes)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok((updated, true));
    }

    tracing::info!(name, "lariv website: creating vnode");
    let created = node::create(
        db,
        store,
        name.into(),
        false,
        Some(NodeFile::Bytes {
            filename: name.into(),
            data: bytes.to_vec(),
        }),
        parent,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((created, true))
}

async fn vnode_bytes_match(
    store: &DynFilestore,
    existing: &lariv_rs::plugins::filesystem::entities::VNode,
    bytes: &[u8],
) -> anyhow::Result<bool> {
    let path = existing.file_path.as_deref().unwrap_or("");
    let mut download = match store.open(path, &existing.name).await {
        Ok(d) => d,
        Err(e) if e.is_missing() => {
            tracing::warn!(
                name = %existing.name,
                vnode_id = existing.id,
                stored_path = path,
                "lariv website: blob missing from store"
            );
            return Ok(false);
        }
        Err(e) => return Err(anyhow::anyhow!("{e}")),
    };
    let mut current = Vec::new();
    download.reader.read_to_end(&mut current).await?;
    Ok(current == bytes)
}

async fn ensure_db_route(
    db: &DatabaseConnection,
    path: &str,
    page_id: i64,
    theme: &str,
    reset_grapes_project: bool,
) -> anyhow::Result<i64> {
    if let Some(existing) = DbRouteEntity::find()
        .filter(DbRouteColumn::Path.eq(path))
        .one(db)
        .await?
    {
        let id = existing.id;
        let mut am: db_route::ActiveModel = existing.into();
        am.page_id = Set(page_id);
        am.is_active = Set(true);
        am.theme = Set(theme.into());
        if reset_grapes_project {
            am.grapes_project = Set(None);
        }
        am.updated_at = Set(Some(Utc::now()));
        am.update(db).await?;
        tracing::info!(path, page_id, "lariv website: updated db route");
        return Ok(id);
    }

    let now = Utc::now();
    let inserted = db_route::ActiveModel {
        id: Default::default(),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        path: Set(path.into()),
        page_id: Set(page_id),
        is_active: Set(true),
        theme: Set(theme.into()),
        grapes_project: Set(None),
    }
    .insert(db)
    .await?;
    tracing::info!(path, page_id, "lariv website: created db route");
    Ok(inserted.id)
}

async fn ensure_route_refs(
    db: &DatabaseConnection,
    route_id: i64,
    vnode_ids: &[i64],
) -> anyhow::Result<()> {
    RouteRefEntity::delete_many()
        .filter(RouteRefColumn::DbRouteId.eq(route_id))
        .exec(db)
        .await?;
    for &vid in vnode_ids {
        route_reference::ActiveModel {
            db_route_id: Set(route_id),
            v_node_id: Set(vid),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}
