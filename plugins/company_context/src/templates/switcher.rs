//! Topbar company switcher.

use maud::{Markup, PreEscaped, html};

use lariv_core::components::{RenderSlot, SlotCtx, form_hx_boost_post_main};
use lariv_plugin_companies::routes::CompanyDefaultRouteTag;

use crate::routes::CompanySwitchPostRouteTag;
use crate::session;

#[derive(Default)]
pub struct CompaniesSwitcher;

impl RenderSlot for CompaniesSwitcher {
    fn render_slot(&self, _ctx: &SlotCtx) -> Markup {
        let Some(auth) = lariv_plugin_users::role_authorization::current_auth() else {
            return Markup::default();
        };
        let user_id = auth.user.id;
        let is_superuser = lariv_plugin_users::roles::Superuser::matches(&auth.role);
        let labels = lariv_core::rune_env::block_on_async(async move {
            session::memberships_for_switcher(user_id).await
        });
        if labels.is_empty() && !is_superuser {
            return html! {
                a class="btn btn-ghost btn-sm" href=(CompanyDefaultRouteTag.url()) {
                    "Companies"
                }
            };
        }
        let active = lariv_core::rune_env::block_on_async(async move {
            session::active_company_label(user_id)
                .await
                .map(|(id, _)| id)
        });
        let active_label = if active.is_none() && is_superuser {
            "System"
        } else {
            labels
                .iter()
                .find(|(id, _, _)| Some(*id) == active)
                .map(|(_, name, _)| name.as_str())
                .unwrap_or("Company")
        };
        html! {
            (PreEscaped(
                r##"<details class="dropdown dropdown-end" @click.outside="if(!$event.target.closest('.fk-picker-panel, .fk-modal-container')) $el.removeAttribute('open')" @click="if($event.target.closest('button[type=submit], a')) $el.removeAttribute('open')">"##,
            ))
            summary class="btn btn-ghost btn-sm" {
                (active_label)
            }
            ul class="menu dropdown-content bg-base-100 rounded-box z-50 w-64 p-2 shadow border border-base-300 fixed right-4" {
                @if is_superuser && labels.is_empty() {
                    li {
                        span class="font-semibold" { "System · no company" }
                    }
                }
                @for (id, name, owned_by) in &labels {
                    @if Some(*id) == active {
                        li {
                            span class="font-semibold" {
                                (name)
                                @if *owned_by { " · owner" }
                            }
                        }
                    } @else {
                        (menu_post(
                            &format!("company-switch-{id}"),
                            &switch_label(name, *owned_by),
                            &CompanySwitchPostRouteTag::new(*id).url(),
                        ))
                    }
                }
                // Empty item: daisyUI draws `.menu li:empty` as a rule.
                li {}
                li {
                    a href=(CompanyDefaultRouteTag.url()) { "Manage companies" }
                }
            }
            (PreEscaped("</details>"))
        }
    }
}

/// POST row that daisyUI styles as a menu item.
///
/// `button_post` wraps a `.btn` inside a form, and `.menu` skips `.btn`, so the
/// control renders as a button sitting inside the row. The submit control has
/// to be the `<li>`'s direct child, with the form kept out of the menu layout.
fn menu_post(form_id: &str, label: &str, action: &str) -> Markup {
    let attrs = form_hx_boost_post_main(action);
    html! {
        li {
            (PreEscaped(format!(
                r#"<form id="{form_id}" hidden style="display:none"{attrs}></form>"#,
                attrs = attrs.as_string(),
            )))
            button type="submit" form=(form_id) {
                (label)
            }
        }
    }
}

fn switch_label(name: &str, owned_by: bool) -> String {
    if owned_by {
        format!("{name} · owner")
    } else {
        name.to_owned()
    }
}
