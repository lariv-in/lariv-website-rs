//! Load assistant preferences through the admin connection.
//!
//! The serving role cannot see another company's preferences row. Background workers
//! (IMAP, cron) need every configured row, so they read through the owner role and
//! then act as that company's owner.

use std::sync::Arc;

use lariv_plugin_llm_assistant::entities::llm_assistant_preferences::Entity as AssistantPrefs;
use lariv_plugin_llm_assistant::preferences::BackgroundPreferences;
use sea_orm::{
    ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, EntityTrait, Statement,
};
use tokio::sync::OnceCell;

pub struct AdminPreferences {
    admin_url: String,
    admin: OnceCell<DatabaseConnection>,
}

pub fn register(admin_url: String) {
    let source = Arc::new(AdminPreferences {
        admin_url,
        admin: OnceCell::new(),
    });
    lariv_plugin_llm_assistant::preferences::set_background_preferences(source);
}

impl AdminPreferences {
    async fn admin(&self) -> Result<&DatabaseConnection, sea_orm::DbErr> {
        self.admin
            .get_or_try_init(|| Database::connect(self.admin_url.as_str()))
            .await
    }
}

#[async_trait::async_trait]
impl BackgroundPreferences for AdminPreferences {
    async fn owned_preferences(
        &self,
        _db: &DatabaseConnection,
    ) -> Result<
        Vec<(
            i64,
            lariv_plugin_llm_assistant::entities::LlmAssistantPreferences,
        )>,
        sea_orm::DbErr,
    > {
        let admin = self.admin().await?;
        let owners = admin
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT rls_owners_name('llm_assistant_preferences') AS name".to_owned(),
            ))
            .await?
            .ok_or_else(|| {
                sea_orm::DbErr::Custom("assistant preferences have no owners table".into())
            })?;
        let owners_name: String = owners.try_get("", "name")?;
        if !owners_name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(sea_orm::DbErr::Custom(
                "refusing unsafe owners table name".into(),
            ));
        }
        let links = admin
            .query_all_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                format!(
                    "SELECT record_id, company_id FROM {owners_name} ORDER BY company_id, record_id"
                ),
            ))
            .await?;
        // company_id is nullable: a superuser with no active company owns the row.
        let mut company_of = std::collections::HashMap::<i64, Option<i64>>::new();
        for link in links {
            let record_id: i64 = link.try_get("", "record_id")?;
            let company_id: Option<i64> = link.try_get("", "company_id")?;
            company_of.insert(record_id, company_id);
        }
        let prefs = AssistantPrefs::find().all(admin).await?;
        let mut owned = Vec::new();
        for pref in prefs {
            let Some(company_id) = company_of.get(&pref.id).copied() else {
                continue;
            };
            let Some(user_id) = actor_for_company(admin, company_id).await? else {
                continue;
            };
            owned.push((user_id, pref));
        }
        Ok(owned)
    }
}

/// User the listener should act as, with `company_user_prefs` pointed at `company_id`.
///
/// `None` is an unassigned row. The actor is a superuser with no active company, so
/// checkout stamps an empty `app.company_id` and later inserts stay unassigned.
async fn actor_for_company(
    admin: &DatabaseConnection,
    company_id: Option<i64>,
) -> Result<Option<i64>, sea_orm::DbErr> {
    let Some(company_id) = company_id else {
        let Some(actor) = admin
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT id FROM users WHERE role = $1 ORDER BY id LIMIT 1",
                [lariv_plugin_users::roles::Superuser::NAME.into()],
            ))
            .await?
        else {
            return Ok(None);
        };
        let user_id: i64 = actor.try_get("", "id")?;
        admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "INSERT INTO company_user_prefs (user_id, active_company_id, updated_at)
                 VALUES ($1, NULL, now())
                 ON CONFLICT (user_id) DO UPDATE
                 SET active_company_id = NULL,
                     updated_at = EXCLUDED.updated_at",
                [user_id.into()],
            ))
            .await?;
        return Ok(Some(user_id));
    };

    let Some(actor) = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT user_id FROM company_memberships
             WHERE company_id = $1
             ORDER BY owned_by DESC, id
             LIMIT 1",
            [company_id.into()],
        ))
        .await?
    else {
        return Ok(None);
    };
    let user_id: i64 = actor.try_get("", "user_id")?;
    // Point this actor's active company at the prefs row's company so the
    // checkout hook's SQL resolve stamps the right app.company_id.
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO company_user_prefs (user_id, active_company_id, updated_at)
             VALUES ($1, $2, now())
             ON CONFLICT (user_id) DO UPDATE
             SET active_company_id = EXCLUDED.active_company_id,
                 updated_at = EXCLUDED.updated_at",
            [user_id.into(), company_id.into()],
        ))
        .await?;
    Ok(Some(user_id))
}
