//! Re-apply the policy script so databases that already ran the first migration
//! pick up later policy changes. The script is safe to apply more than once.

use sea_orm_migration::prelude::*;

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
            .execute_unprepared(super::INSTALL_SQL)
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
