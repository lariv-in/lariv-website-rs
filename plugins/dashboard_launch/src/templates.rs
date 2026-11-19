//! New-tab apps grid and empty replacement for the topbar apps-grid button.

use frunk::hlist::HList;
use lariv_core::components::{
    RenderSlot, ShellChrome, ShellTopbar, SlotCapability, SlotCtx, SlotOf, SlotRegistrar,
    TopbarItemsSlotTag, dashboard_app_href, icon, shell_topbar,
};
use lariv_core::template::{RenderAppPane, RenderTemplate};
use lariv_core::traits::replace::MapByTag;
use lariv_plugin_dashboard::AppTile;
use lariv_plugin_dashboard::templates::DashboardAppsPageButtonTag;
use maud::{Markup, PreEscaped, html};

/// Empty topbar contributor — hides the shared dashboard apps-grid button.
#[derive(Default)]
pub struct HiddenAppsButton;

impl RenderSlot for HiddenAppsButton {
    fn render_slot(&self, _ctx: &SlotCtx) -> Markup {
        Markup::default()
    }
}

type HiddenAppsSlot = SlotOf<TopbarItemsSlotTag, HiddenAppsButton>;

/// Replace [`DashboardAppsPageButtonTag`] with [`HiddenAppsButton`].
#[derive(Copy, Clone)]
pub struct SlotsHook<AppsBtnIdx>(std::marker::PhantomData<AppsBtnIdx>);

impl<AppsBtnIdx> Default for SlotsHook<AppsBtnIdx> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T, AppsBtnIdx> SlotRegistrar<T> for SlotsHook<AppsBtnIdx>
where
    T: HList,
    T: MapByTag<DashboardAppsPageButtonTag, HiddenAppsSlot, AppsBtnIdx>,
{
    type Output = <T as MapByTag<DashboardAppsPageButtonTag, HiddenAppsSlot, AppsBtnIdx>>::Output;

    fn register_slots(self, cap: SlotCapability<T>) -> SlotCapability<Self::Output> {
        cap.replace_slot::<DashboardAppsPageButtonTag, AppsBtnIdx, HiddenAppsSlot>(|_| {
            SlotOf::new()
        })
    }
}

fn apps_grid(apps: &[AppTile]) -> Markup {
    html! {
        (PreEscaped(
            r##"<div class="container max-w-5xl mx-auto mt-4 @container" x-data="{ search: ''}">"##,
        ))
        div class="mb-4" {
            (PreEscaped(
                r##"<input type="text" x-model="search" placeholder="Search apps..." class="input input-bordered w-full">"##,
            ))
        }
        (PreEscaped(r##"<div class="grid grid-cols-2 @md:grid-cols-4 @2xl:grid-cols-6 gap-2">"##))
        @for app in apps {
            (PreEscaped({
                let href = dashboard_app_href(&app.href);
                format!(
                    r##"<a href="{href}" target="_blank" rel="noopener" class="btn btn-md h-auto flex-col space-y-1 py-4" x-show="'{name}'.toLowerCase().includes(search.toLowerCase())" x-cloak>"##,
                    href = html_escape_attr(&href),
                    name = html_escape_js_string(&app.verbose_name),
                )
            }))
            (icon(&app.icon, "w-8 h-8"))
            div class="text-sm truncate min-w-0 w-full" { (app.verbose_name) }
            (PreEscaped("</a>"))
        }
        (PreEscaped("</div></div>"))
    }
}

fn html_escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn html_escape_js_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out
}

/// Deployment launchpad page (new-tab tiles).
pub struct LaunchpadPage {
    pub name: String,
    pub role: String,
    pub avatar: String,
    pub apps: Vec<AppTile>,
}

impl LaunchpadPage {
    fn pane_body(&self) -> Markup {
        apps_grid(&self.apps)
    }
}

impl RenderAppPane for LaunchpadPage {
    fn render_pane(&self) -> lariv_core::components::AppLayoutHtml {
        use lariv_core::components::app_layout_pane;
        app_layout_pane(self.pane_body())
    }

    fn render_main(&self) -> lariv_core::components::MainContentHtml {
        use lariv_core::components::{LayoutMain, layout_main};
        layout_main(LayoutMain {
            breadcrumbs: Markup::default(),
            content: self.pane_body(),
        })
    }
}

impl RenderTemplate for LaunchpadPage {
    fn render(&self, chrome: &ShellChrome) -> Markup {
        shell_topbar(ShellTopbar {
            title: "Lariv",
            registry_head: chrome.head.clone(),
            topbar_items: chrome.topbar_items.clone(),
            right_sidebar: chrome.right_sidebar.clone(),
            body: self.pane_body(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lariv_plugin_dashboard::PluginType;

    #[test]
    fn launchpad_tiles_open_in_new_tab_without_htmx() {
        lariv_plugin_dashboard::nav_origin::register();
        let page = LaunchpadPage {
            name: "Ada".into(),
            role: "Admin".into(),
            avatar: "A".into(),
            apps: vec![AppTile {
                key: "p_users".into(),
                verbose_name: "Users".into(),
                href: "/users".into(),
                icon: "users".into(),
                plugin_type: PluginType::App,
                roles: vec![],
            }],
        };
        let html = page.render(&ShellChrome::default()).into_string();
        assert!(html.contains(r#"target="_blank""#));
        assert!(html.contains(r#"rel="noopener""#));
        assert!(html.contains("/users/?from=dashboard"));
        assert!(!html.contains("hx-get="));
        assert!(!html.contains("hx-target="));
        assert!(!html.contains("squares-2x2"));
    }

    #[test]
    fn hidden_apps_button_renders_nothing() {
        let html = HiddenAppsButton.render_slot(&SlotCtx::default()).into_string();
        assert!(html.is_empty());
    }
}
