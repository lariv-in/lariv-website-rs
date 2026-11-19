//! Companies, bank accounts, contacts, import/export, and filesystem on the accounting sidebar.
//!
//! Each of these leaves the Accounting app, so the link is marked external.
//! Finance plugins already register their own in-app links.

use lariv_core::plugin_install::define_plugin_install;
use lariv_plugin_bank_accounts::routes::BankAccountDefaultRouteTag;
use lariv_plugin_companies::routes::CompanyDefaultRouteTag;
use lariv_plugin_contacts::routes::ContactDefaultRouteTag;
use lariv_plugin_filesystem::routes::VNodeListRouteTag;
use lariv_plugin_finance_accounts::accounting_sidebar::{self, AccountingSidebarRegistrar};
use lariv_plugin_import_export::routes::ImportPageRouteTag;

/// Capability tag for the deployment's accounting sidebar links.
pub struct AccountingSidebarLinksTag;

define_plugin_install! {
    plugin: AccountingSidebarLinksTag;
    /// Register sidebar links to the other apps this deployment ships.
    steps: [
        cap_hook(lariv_plugin_finance_accounts::accounting_sidebar::AccountingSidebarTag, lariv_plugin_finance_accounts::accounting_sidebar::AccountingSidebarCap, Hook),
    ]
}

#[derive(Clone, Copy, Default)]
pub struct Hook;

impl AccountingSidebarRegistrar for Hook {
    fn register_accounting_sidebar(
        self,
        cap: accounting_sidebar::AccountingSidebarRegistry,
    ) -> accounting_sidebar::AccountingSidebarRegistry {
        cap.push(
            accounting_sidebar::link::<CompanyDefaultRouteTag>(
                "companies",
                "Companies",
                900,
                Some("building-office-2"),
            )
            .external(),
        )
        .push(
            accounting_sidebar::link::<BankAccountDefaultRouteTag>(
                "bank-accounts",
                "Bank Accounts",
                905,
                Some("building-library"),
            )
            .external(),
        )
        .push(
            accounting_sidebar::link::<ContactDefaultRouteTag>(
                "contacts",
                "Contacts",
                910,
                Some("users"),
            )
            .external(),
        )
        .push(
            accounting_sidebar::link::<ImportPageRouteTag>(
                "import-export",
                "Import / Export",
                930,
                Some("arrows-right-left"),
            )
            .external(),
        )
        .push(
            accounting_sidebar::link::<VNodeListRouteTag>(
                "filesystem",
                "Filesystem",
                940,
                Some("folder"),
            )
            .external(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_links_the_other_apps() {
        let cap =
            Hook.register_accounting_sidebar(accounting_sidebar::AccountingSidebarRegistry::new());
        let links: Vec<_> = cap
            .links()
            .iter()
            .map(|link| (link.label, link.url.clone(), link.external))
            .collect();
        assert_eq!(
            links,
            vec![
                ("Companies", CompanyDefaultRouteTag.url(), true),
                ("Bank Accounts", BankAccountDefaultRouteTag.url(), true),
                ("Contacts", ContactDefaultRouteTag.url(), true),
                ("Import / Export", ImportPageRouteTag.url(), true),
                ("Filesystem", VNodeListRouteTag.url(), true),
            ]
        );
    }
}
