//! Drop the Gemini key copy trigger. The script is safe to apply more than once.
//!
//! The version name is the file stem. It must not be `migrations`, because the
//! row-level security plugin already records a migration under that name.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Ok(());
        }
        manager
            .get_connection()
            .execute_unprepared(super::GEMINI_SQL)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager;
        Err(DbErr::Custom(
            "the Gemini key trigger is not reversed automatically".into(),
        ))
    }
}
