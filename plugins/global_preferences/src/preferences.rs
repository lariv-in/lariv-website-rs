//! Load the singleton row, and supply it when a user has no key of their own.
//!
//! The assistant asks the shared source before the user's row. This source
//! returns the user's key when it is set, and the global key otherwise, so the
//! global values are the fallback.

use std::sync::Arc;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DatabaseConnection, DbErr, EntityTrait, QuerySelect,
};
use tokio::sync::OnceCell;

use lariv_plugin_llm_assistant::entities::llm_assistant_preferences::Entity as UserPrefs;
use lariv_plugin_llm_assistant::preferences::{SharedLlmCredentials, set_shared_llm_credentials};

use super::entities::{
    GlobalPreferences,
    global_preferences::{self, Entity as GlobalPrefs},
};

const SINGLETON_ID: i64 = 1;

pub struct GlobalCredentials {
    database_url: String,
    admin: OnceCell<DatabaseConnection>,
}

pub fn register(database_url: String) {
    let source = Arc::new(GlobalCredentials {
        database_url,
        admin: OnceCell::new(),
    });
    set_shared_llm_credentials(source);
}

/// User key when it is non-empty, otherwise the site-wide key.
pub fn prefer_user_key(user_key: &str, global_key: &str) -> String {
    let user_key = user_key.trim();
    if !user_key.is_empty() {
        return user_key.to_owned();
    }
    global_key.trim().to_owned()
}

/// Load the singleton, creating it when it is missing.
pub async fn load_preferences(db: &DatabaseConnection) -> Result<GlobalPreferences, DbErr> {
    if let Some(prefs) = GlobalPrefs::find_by_id(SINGLETON_ID).one(db).await? {
        return Ok(prefs);
    }
    let now = Utc::now();
    let model = global_preferences::ActiveModel {
        id: Set(SINGLETON_ID),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        gemini_api_key: Set(String::new()),
        google_search_api_key: Set(String::new()),
        google_search_cx: Set(String::new()),
    };
    model.insert(db).await
}

/// Persist the three fallback credentials onto the singleton row.
pub async fn save_preferences(
    db: &DatabaseConnection,
    prefs: GlobalPreferences,
) -> Result<GlobalPreferences, DbErr> {
    let mut am: global_preferences::ActiveModel = load_preferences(db).await?.into();
    am.gemini_api_key = Set(prefs.gemini_api_key.trim().to_owned());
    am.google_search_api_key = Set(prefs.google_search_api_key.trim().to_owned());
    am.google_search_cx = Set(prefs.google_search_cx.trim().to_owned());
    am.updated_at = Set(Some(Utc::now()));
    am.update(db).await
}

impl GlobalCredentials {
    async fn admin(&self) -> Result<Option<&DatabaseConnection>, DbErr> {
        if self.database_url.is_empty() {
            return Ok(None);
        }
        let conn = self
            .admin
            .get_or_try_init(|| website_plugin_rls::connect_privileged(self.database_url.as_str()))
            .await?;
        Ok(Some(conn))
    }

    async fn global_row(&self) -> Result<Option<GlobalPreferences>, DbErr> {
        let Some(admin) = self.admin().await? else {
            return Ok(None);
        };
        GlobalPrefs::find_by_id(SINGLETON_ID).one(admin).await
    }
}

async fn user_field(
    db: &DatabaseConnection,
    pick: impl Fn(&lariv_plugin_llm_assistant::entities::LlmAssistantPreferences) -> &str,
) -> Result<String, DbErr> {
    let Some(prefs) = UserPrefs::find().limit(1).one(db).await? else {
        return Ok(String::new());
    };
    Ok(pick(&prefs).to_owned())
}

#[async_trait::async_trait]
impl SharedLlmCredentials for GlobalCredentials {
    async fn api_key(&self, db: &DatabaseConnection) -> Result<String, DbErr> {
        self.resolved(
            db,
            |prefs| prefs.api_key.as_str(),
            |row| row.gemini_api_key.clone(),
        )
        .await
    }

    async fn cse_api_key(&self, db: &DatabaseConnection) -> Result<String, DbErr> {
        self.resolved(
            db,
            |prefs| prefs.cse_api_key.as_str(),
            |row| row.google_search_api_key.clone(),
        )
        .await
    }

    async fn cse_cx(&self, db: &DatabaseConnection) -> Result<String, DbErr> {
        self.resolved(
            db,
            |prefs| prefs.cse_cx.as_str(),
            |row| row.google_search_cx.clone(),
        )
        .await
    }
}

impl GlobalCredentials {
    async fn resolved(
        &self,
        db: &DatabaseConnection,
        user_pick: impl Fn(&lariv_plugin_llm_assistant::entities::LlmAssistantPreferences) -> &str,
        global_pick: impl Fn(&GlobalPreferences) -> String,
    ) -> Result<String, DbErr> {
        let user = user_field(db, user_pick).await?;
        if !user.trim().is_empty() {
            return Ok(prefer_user_key(&user, ""));
        }
        let global = self
            .global_row()
            .await?
            .map(|row| global_pick(&row))
            .unwrap_or_default();
        Ok(prefer_user_key(&user, &global))
    }
}

#[cfg(test)]
mod tests {
    use super::prefer_user_key;

    #[test]
    fn user_key_wins() {
        assert_eq!(prefer_user_key("user-key", "global-key"), "user-key");
    }

    #[test]
    fn blank_user_key_uses_global() {
        assert_eq!(prefer_user_key("  ", "global-key"), "global-key");
        assert_eq!(prefer_user_key("", "  "), "");
    }
}
