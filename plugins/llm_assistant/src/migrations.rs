//! Drop the Gemini key copy trigger after row-level security has created owner tables.

use sea_orm_migration::prelude::*;

use super::AssistantDeployTag;

mod m20261117_000001_gemini_key;

pub const GEMINI_SQL: &str = include_str!("gemini.sql");

#[derive(Clone, Copy, Default)]
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261117_000001_gemini_key::Migration)]
    }
}

/// Drop the copy trigger after every migration pass.
///
/// The preferences table and the owners table come from other plugins. A one-shot
/// migration would miss them when this plugin's migration has already been recorded.
pub fn after_migrate(
    db: &sea_orm::DatabaseConnection,
) -> futures_util::future::BoxFuture<'_, Result<(), DbErr>> {
    Box::pin(async move {
        if db.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Ok(());
        }
        db.execute_unprepared(GEMINI_SQL).await?;
        Ok(())
    })
}

lariv_core::define_register_migrations! {
    plugin: AssistantDeployTag;
    migrator: Migrator;
    after: after_migrate;
}
