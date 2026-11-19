//! Assistant preferences on the accounting sidebar.
//!
//! The page lives in the assistant app, so the link is marked external.

use lariv_plugin_finance_accounts::accounting_sidebar::{self, AccountingSidebarRegistrar};
use lariv_plugin_llm_assistant::routes::PrefsGetRouteTag;

#[derive(Clone, Copy, Default)]
pub struct Hook;

impl AccountingSidebarRegistrar for Hook {
    fn register_accounting_sidebar(
        self,
        cap: accounting_sidebar::AccountingSidebarRegistry,
    ) -> accounting_sidebar::AccountingSidebarRegistry {
        cap.push(
            accounting_sidebar::link::<PrefsGetRouteTag>(
                "llm-preferences",
                "Assistant preferences",
                950,
                Some("adjustments-horizontal"),
            )
            .external(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_links_assistant_preferences() {
        let cap =
            Hook.register_accounting_sidebar(accounting_sidebar::AccountingSidebarRegistry::new());
        let links: Vec<_> = cap
            .links()
            .iter()
            .map(|link| (link.label, link.url.clone(), link.external))
            .collect();
        assert_eq!(
            links,
            vec![("Assistant preferences", PrefsGetRouteTag.url(), true)]
        );
    }
}
