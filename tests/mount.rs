//! Compile smoke test for the Lariv website plugin stack.

#![recursion_limit = "4096"]

use std::path::PathBuf;

use lariv_rs::app::App;
use lariv_rs::plugins::{
    bank_accounts, blog, companies, contacts, crm, customer, dashboard, filesystem,
    finance_accounts, finance_creditnotes, finance_customers, finance_gst_export, finance_indian,
    finance_invoices, finance_invoices_log, finance_payments, finance_products, finance_purchases,
    finance_taxes, import_export, llm_assistant, logging, otp, signup, users, website,
};
use website_plugin_accounting_sidebar as accounting_sidebar;
use website_plugin_company_context as company_context;
use website_plugin_dashboard_launch as dashboard_launch;
use website_plugin_global_preferences as global_preferences;
use website_plugin_llm_assistant as assistant;
use website_plugin_rls as rls;

const STACK_SIZE: usize = 64 * 1024 * 1024;

const MINIMAL_DB_TOML: &str = r#"database_url = "sqlite::memory:"
[users]
adminEmail = "admin@test.local"
adminPassword = "adminadmin"
"#;

fn temp_config(body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "lariv-website-mount-{}-{}.toml",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&path, body).expect("write temp config");
    path
}

#[test]
fn lariv_website_stack_mounts() {
    std::thread::Builder::new()
        .name("lariv-website-mount".into())
        .stack_size(STACK_SIZE)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async {
                let app = App::new_web_app();
                let app = users::install(app);
                let app = otp::install(app);
                let app = signup::install(app);
                let app = filesystem::install(app);
                let app = llm_assistant::install(app);
                let app = finance_accounts::install(app);
                let app = finance_customers::install(app);
                let app = companies::install(app);
                let app = company_context::install(app);
                let app = bank_accounts::install(app);
                let app = contacts::install(app);
                let app = customer::install(app);
                let app = crm::install(app);
                let app = finance_creditnotes::install(app);
                let app = finance_taxes::install(app);
                let app = finance_products::install(app);
                let app = finance_invoices::install(app);
                let app = logging::install(app);
                let app = finance_invoices_log::install(app);
                let app = finance_payments::install(app);
                let app = finance_purchases::install(app);
                let app = finance_indian::install(app);
                let app = import_export::install(app);
                let app = finance_gst_export::install(app);
                let app = blog::install(app);
                let app = dashboard::install(app);
                let app = dashboard_launch::install(app);
                let app = website::install(app);
                let app = accounting_sidebar::install(app);
                let app = global_preferences::install(app);
                let app = rls::install(app);
                let app = assistant::install(app);

                let path = temp_config(MINIMAL_DB_TOML);
                let app = app.load_config(&path).await.expect("load_config");
                std::fs::remove_file(&path).ok();
                let _mounted = app.mount();
            });
        })
        .expect("spawn lariv-website-mount thread")
        .join()
        .expect("lariv-website-mount thread");
}
