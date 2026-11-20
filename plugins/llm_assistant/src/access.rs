//! Company members can open Accounting and the linked apps without an install step.
//!
//! Framework tiles and routes start as superuser-only (`roles: []` / empty allowlists).
//! This hook adds `unassigned` and `admin` to the dashboard tiles and route allowlists
//! that the former App Manager catalog unlocked for every company.

use lariv_core::apps::{AppsCapability, AppsRegistrar};
use lariv_plugin_bank_accounts::routes::{BankAccountsMutate, BankAccountsView};
use lariv_plugin_companies::routes::{CompaniesMembersMutate, CompaniesMutate, CompaniesView};
use lariv_plugin_contacts::routes::{ContactsMutate, ContactsView};
use lariv_plugin_crm::routes::CustomerSelect;
use lariv_plugin_customer::routes::CustomerMutate;
use lariv_plugin_filesystem::apps::FILESYSTEM_APP_KEY;
use lariv_plugin_filesystem::routes::FilesystemPermissions;
use lariv_plugin_finance_accounts::{
    ACCOUNTING_APP_KEY,
    routes::{FinanceAccountsMutate, FinanceAccountsPreferencesMutate, FinanceAccountsView},
};
use lariv_plugin_finance_creditnotes::routes::FinanceCreditNotesView;
use lariv_plugin_finance_customers::routes::FinanceCustomersView;
use lariv_plugin_finance_gst_export::routes::{GstExcelView, GstExportMutate, GstExportView};
use lariv_plugin_finance_invoices::routes::{FinanceInvoicesMutate, FinanceInvoicesView};
use lariv_plugin_finance_payments::routes::{FinancePaymentsMutate, FinancePaymentsView};
use lariv_plugin_finance_products::routes::{FinanceProductsMutate, FinanceProductsView};
use lariv_plugin_finance_purchases::routes::{FinancePurchasesMutate, FinancePurchasesView};
use lariv_plugin_finance_taxes::routes::{FinanceTaxesMutate, FinanceTaxesView};
use lariv_plugin_import_export::routes::{ExportAdmin, ImportAdmin};
use lariv_plugin_llm_assistant::{
    apps::{LLM_ASSISTANT_APP_KEY, allow_sidebar_role},
    routes::LlmPrefsAdmin,
};
use lariv_plugin_users::role_authorization::{
    RoleAuthorizationRegistrar, RoleAuthorizationRegistry,
};
use lariv_plugin_users::roles::{Admin, Unassigned};

/// Dashboard tiles company members may open (former App Manager Accounting bundle).
const COMPANY_APPS: &[&str] = &[
    ACCOUNTING_APP_KEY,
    "p_finance_creditnotes",
    "p_finance_taxes",
    "p_finance_products",
    "p_finance_invoices",
    "p_finance_payments",
    "p_finance_purchases",
    "p_finance_gst_export",
    "p_companies",
    "p_bank_accounts",
    "p_contacts",
    "p_import_export",
    FILESYSTEM_APP_KEY,
    LLM_ASSISTANT_APP_KEY,
];

#[derive(Clone, Copy, Default)]
pub struct AppsHook;

impl AppsRegistrar for AppsHook {
    fn register_apps(self, apps: AppsCapability) -> AppsCapability {
        allow_sidebar_role(Unassigned::NAME);
        allow_sidebar_role(Admin::NAME);
        COMPANY_APPS
            .iter()
            .fold(apps, |apps, key| allow_app(apps, key))
    }
}

fn allow_app(apps: AppsCapability, key: &str) -> AppsCapability {
    let Some(mut tile) = apps.apps().iter().find(|tile| tile.key == key).cloned() else {
        return apps;
    };
    for name in [Unassigned::NAME, Admin::NAME] {
        if !tile.roles.iter().any(|role| role == name) {
            tile.roles.push(name.into());
        }
    }
    apps.register(tile)
}

#[derive(Clone, Copy, Default)]
pub struct RoleHook;

impl RoleAuthorizationRegistrar for RoleHook {
    fn register_roles(self, registry: RoleAuthorizationRegistry) -> RoleAuthorizationRegistry {
        registry
            .patch::<FinanceAccountsView>(allow_company)
            .patch::<FinanceAccountsMutate>(allow_company)
            .patch::<FinanceAccountsPreferencesMutate>(allow_company)
            .patch::<FinanceCreditNotesView>(allow_company)
            .patch::<FinanceCustomersView>(allow_company)
            .patch::<FinanceTaxesView>(allow_company)
            .patch::<FinanceTaxesMutate>(allow_company)
            .patch::<FinanceProductsView>(allow_company)
            .patch::<FinanceProductsMutate>(allow_company)
            .patch::<FinanceInvoicesView>(allow_company)
            .patch::<FinanceInvoicesMutate>(allow_company)
            .patch::<FinancePaymentsView>(allow_company)
            .patch::<FinancePaymentsMutate>(allow_company)
            .patch::<FinancePurchasesView>(allow_company)
            .patch::<FinancePurchasesMutate>(allow_company)
            .patch::<GstExportView>(allow_company)
            .patch::<GstExportMutate>(allow_company)
            .patch::<GstExcelView>(allow_company)
            .patch::<BankAccountsView>(allow_company)
            .patch::<BankAccountsMutate>(allow_company)
            .patch::<CompaniesView>(allow_company)
            .patch::<CompaniesMutate>(allow_company)
            .patch::<CompaniesMembersMutate>(allow_company)
            .patch::<ContactsView>(allow_company)
            .patch::<ContactsMutate>(allow_company)
            .patch::<CustomerSelect>(allow_company)
            .patch::<CustomerMutate>(allow_company)
            .patch::<ImportAdmin>(allow_company)
            .patch::<ExportAdmin>(allow_company)
            .patch::<FilesystemPermissions>(allow_company)
            .patch::<LlmPrefsAdmin>(allow_company)
    }
}

fn allow_company(roles: &mut Vec<String>) {
    for name in [Unassigned::NAME, Admin::NAME] {
        if !roles.iter().any(|role| role == name) {
            roles.push(name.into());
        }
    }
}
