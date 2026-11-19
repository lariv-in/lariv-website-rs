#![recursion_limit = "4096"]

use lariv_rs::app::App;
use lariv_rs::plugins::{
    bank_accounts, blog, companies, contacts, crm, customer, dashboard, filesystem,
    filesystem_sync_git, finance_accounts, finance_creditnotes, finance_customers,
    finance_gst_export, finance_indian, finance_invoices, finance_invoices_log, finance_payments,
    finance_products, finance_purchases, finance_taxes, import_export, llm_assistant, logging,
    mdbook, otp, signup, users, website,
};
use tracing_subscriber::EnvFilter;
use website_plugin_accounting_sidebar as accounting_sidebar;
use website_plugin_company_context as company_context;
use website_plugin_dashboard_launch as dashboard_launch;
use website_plugin_global_preferences as global_preferences;
use website_plugin_llm_assistant as assistant;
use website_plugin_rls as rls;

#[lariv_rs::main(
    stack_size = 128 * 1024 * 1024,
    flavor = "multi_thread",
    thread_name = "lariv-website-server"
)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("info".parse().expect("directive")),
        )
        .init();

    let app = App::new_web_app();
    let app = users::install(app);
    // After OTP. OTP also owns `/users/login`, and the last install wins.
    // Signup's page keeps "Forgot password?" and adds the sign-up link.
    let app = otp::install(app);
    let app = signup::install(app);
    let app = filesystem::install(app);
    // Before companies, contacts, customer, and finance plugins that hook the assistant.
    let app = llm_assistant::install(app);
    let app = finance_accounts::install(app);
    // Before invoices and payments, which hook the customer journal.
    let app = finance_customers::install(app);
    let app = companies::install(app);
    // After companies so switch routes can reuse CompaniesView. Before assistant,
    // which patches that allowlist for company members.
    let app = company_context::install(app);
    let app = bank_accounts::install(app);
    let app = contacts::install(app);
    let app = customer::install(app);
    let app = crm::install(app);
    let app = finance_creditnotes::install(app);
    let app = finance_taxes::install(app);
    let app = finance_products::install(app);
    let app = finance_invoices::install(app);
    // After invoices, which own the detail menu this log hooks.
    let app = logging::install(app);
    let app = finance_invoices_log::install(app);
    let app = finance_payments::install(app);
    let app = finance_purchases::install(app);
    let app = finance_indian::install(app);
    // Before GST export, which hooks the import/export download page.
    let app = import_export::install(app);
    let app = finance_gst_export::install(app);
    let app = blog::install(app);
    let app = dashboard::install(app);
    // After dashboard: hide apps-grid icon and open tiles in a new tab.
    let app = dashboard_launch::install(app);
    // After dashboard so website can own `/` (CMS home) over the auth redirect.
    let app = website::install(app);
    let app = mdbook::install(app);
    let app = filesystem_sync_git::install(app);
    // External links from Accounting to the other apps this deployment ships.
    let app = accounting_sidebar::install(app);
    // Before row-level security, so the singleton exists when policies are applied.
    let app = global_preferences::install(app);
    // After the other plugins' tables exist, so policies cover them.
    // The assistant plugin follows: its Gemini key script needs the owner tables.
    let app = rls::install(app);
    let app = assistant::install(app);

    rls::prepare_database("config.toml").await?;
    let app = app.load_config("config.toml").await?;
    let app = app.mount();
    app.run_migrations().await?;
    app.run_seeds().await?;
    app.run().await?;
    Ok(())
}
