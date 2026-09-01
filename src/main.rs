#![recursion_limit = "512"]

use lariv_rs::app::App;
use lariv_rs::plugins::{blog, dashboard, filesystem, users, website};
use tracing_subscriber::EnvFilter;

mod website_seed;

#[lariv_rs::main(
    stack_size = 64 * 1024 * 1024,
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
    let app = filesystem::install(app);
    let app = blog::install(app);
    let app = dashboard::install(app);
    // After dashboard so website can own `/` (CMS home) over the auth redirect.
    let app = website::install(app);
    let app = website_seed::install(app);

    let app = app.load_config("config.toml").await?;
    let app = app.mount();
    app.run_migrations().await?;
    app.run().await?;
    Ok(())
}
