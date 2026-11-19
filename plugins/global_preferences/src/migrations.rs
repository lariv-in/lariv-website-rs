//! Create the global preferences singleton.

use sea_orm_migration::prelude::*;

use super::GlobalPreferencesTag;

mod m20261119_000001_create_global_preferences;

#[derive(Clone, Copy, Default)]
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(
            m20261119_000001_create_global_preferences::Migration,
        )]
    }
}

lariv_core::define_register_migrations! {
    plugin: GlobalPreferencesTag;
    migrator: Migrator;
}
