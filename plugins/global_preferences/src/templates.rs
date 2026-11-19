//! Global preferences page.

use frunk::Generic;
use maud::{Markup, html};

use lariv_core::capability::define_register_items;
use lariv_core::components::{
    ButtonSubmit, Crumb, FormOpts, LayoutMain, LayoutSidebar, ShellChrome, ShellScaffold,
    SidebarMenu, SidebarNavLink, breadcrumbs, button_submit, form, form_hx_post_main, layout_main,
    layout_sidebar, shell_scaffold, sidebar_menu, sidebar_nav_items_pane,
};
use lariv_core::html_form::{CsrfToken, FormCtx, HtmlForm};
use lariv_core::http::ProvideRequestCaps;
use lariv_core::template::{RenderTemplate, TemplateCapability, TemplateOf, TemplateRegistrar};

use super::forms::{PreferencesForm, PreferencesFormField};
use super::routes::{GlobalPrefsGetRouteTag, GlobalPrefsPostRouteTag};

#[derive(Generic)]
pub struct GlobalPreferencesPage {
    pub gemini_api_key: String,
    pub google_search_api_key: String,
    pub google_search_cx: String,
    pub error: String,
}

impl GlobalPreferencesPage {
    fn body(&self) -> Markup {
        form(
            &CsrfToken::current(),
            FormOpts {
                attrs: form_hx_post_main(GlobalPrefsPostRouteTag),
                title: "Global Preferences",
                subtitle: "Default Gemini and Google Search credentials. A user's own assistant preferences are used when set.",
                form_error: Some(self.error.as_str()).filter(|e| !e.is_empty()),
                inputs: PreferencesForm::render_inputs(
                    &FormCtx::form::<PreferencesForm>(CsrfToken::current())
                        .value(
                            PreferencesFormField::GeminiApiKey,
                            self.gemini_api_key.as_str(),
                        )
                        .value(
                            PreferencesFormField::GoogleSearchApiKey,
                            self.google_search_api_key.as_str(),
                        )
                        .value(
                            PreferencesFormField::GoogleSearchCx,
                            self.google_search_cx.as_str(),
                        ),
                ),
                actions: html! {
                    (button_submit(ButtonSubmit {
                        label: "Save preferences",
                        ..Default::default()
                    }))
                },
                ..Default::default()
            },
        )
    }
}

fn crumbs() -> Markup {
    breadcrumbs(&[Crumb {
        label: "Global Preferences",
        href: None,
    }])
}

fn menu(current_path: &str) -> Markup {
    let prefs_url = GlobalPrefsGetRouteTag.url();
    let links = [SidebarNavLink {
        key: "preferences",
        title: "Preferences",
        url: &prefs_url,
        icon_name: None,
        match_prefixes: &[],
    }];
    sidebar_menu(SidebarMenu {
        title: "Global Preferences",
        children: sidebar_nav_items_pane(&links, current_path),
    })
}

impl lariv_core::template::RenderAppPane for GlobalPreferencesPage {
    fn render_pane(&self) -> lariv_core::components::AppLayoutHtml {
        layout_sidebar(LayoutSidebar {
            sidebar: menu(&GlobalPrefsGetRouteTag.url()),
            breadcrumbs: crumbs(),
            content: self.body(),
        })
    }

    fn render_main(&self) -> lariv_core::components::MainContentHtml {
        layout_main(LayoutMain {
            breadcrumbs: crumbs(),
            content: self.body(),
        })
    }
}

impl RenderTemplate for GlobalPreferencesPage {
    fn render(&self, chrome: &ShellChrome) -> Markup {
        shell_scaffold(ShellScaffold {
            title: "Lariv",
            registry_head: chrome.head.clone(),
            topbar_items: chrome.topbar_items.clone(),
            right_sidebar: chrome.right_sidebar.clone(),
            sidebar: menu(&GlobalPrefsGetRouteTag.url()),
            breadcrumbs: crumbs(),
            body: self.body(),
            ..Default::default()
        })
    }
}

define_register_items! {
    plugin: GlobalPreferencesTag;
    capability: TemplateCapability;
    trait: TemplateRegistrar;
    method: register_templates;
    wrapper: TemplateOf;
    bounds: [Clone, ProvideRequestCaps, Send, Sync];
    hook: Hook;
    items: [
        GlobalPreferencesIdx: GlobalPreferencesPageTag => GlobalPreferencesPage,
    ]
}
