//! One site-wide row (`id = 1`) of fallback API credentials.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(GlobalPreferences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(GlobalPreferences::Id)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(GlobalPreferences::CreatedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(GlobalPreferences::UpdatedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(GlobalPreferences::GeminiApiKey)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(GlobalPreferences::GoogleSearchApiKey)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(GlobalPreferences::GoogleSearchCx)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await?;

        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE global_preferences
                     ADD CONSTRAINT global_preferences_singleton CHECK (id = 1)",
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GlobalPreferences::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum GlobalPreferences {
    Table,
    Id,
    CreatedAt,
    UpdatedAt,
    GeminiApiKey,
    GoogleSearchApiKey,
    GoogleSearchCx,
}
