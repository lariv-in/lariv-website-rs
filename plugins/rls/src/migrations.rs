//! Install row-level security after every other plugin has created its tables.

use sea_orm_migration::prelude::*;

use super::RlsTag;

mod m20261115_000001_sync_gemini_key;

pub const INSTALL_SQL: &str = include_str!("install.sql");

#[derive(Clone, Copy, Default)]
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(Migration),
            Box::new(m20261115_000001_sync_gemini_key::Migration),
        ]
    }
}

/// Re-apply policies after every migration pass, including one with nothing pending.
///
/// The script is safe to apply more than once. A table created by a later migration
/// would otherwise stay uncovered, because each SeaORM migration runs only once.
pub fn after_migrate(
    db: &sea_orm::DatabaseConnection,
) -> futures_util::future::BoxFuture<'_, Result<(), DbErr>> {
    Box::pin(async move {
        if db.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Ok(());
        }
        db.execute_unprepared(INSTALL_SQL).await?;
        Ok(())
    })
}

lariv_core::define_register_migrations! {
    plugin: RlsTag;
    migrator: Migrator;
    after: after_migrate;
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Err(DbErr::Custom("row level security requires postgres".into()));
        }
        manager
            .get_connection()
            .execute_unprepared(INSTALL_SQL)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager;
        Err(DbErr::Custom(
            "row level security policies are not reversed automatically".into(),
        ))
    }
}

/// The SQL applied by this migration. Tests call it directly.
pub fn install_sql() -> &'static str {
    INSTALL_SQL
}
