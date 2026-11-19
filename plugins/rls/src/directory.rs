//! Login and signup against tables the app role cannot read freely.

use std::sync::{Arc, Mutex, OnceLock};

use lariv_plugin_users::auth::{self, AuthDirectory, CreateUser};
use lariv_plugin_users::entities::user::{Entity as UserEntity, User};
use lariv_plugin_users::error::UsersError;
use lariv_plugin_users::password;
use lariv_plugin_users::role_authorization::current_auth;
use lariv_plugin_users::roles::{Superuser, Unassigned};
use sea_orm::{
    ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, EntityTrait, Statement,
};
use tokio::sync::OnceCell;

use crate::catalog;

struct Directory {
    admin_url: String,
    admin_email: String,
    admin: OnceCell<DatabaseConnection>,
}

static DIRECTORY: OnceLock<Arc<Directory>> = OnceLock::new();
static REGISTERED: Mutex<bool> = Mutex::new(false);

pub fn register(admin_url: String, admin_email: String) {
    let directory = Arc::new(Directory {
        admin_url,
        admin_email,
        admin: OnceCell::new(),
    });
    let _ = DIRECTORY.set(directory.clone());
    let mut registered = REGISTERED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !*registered {
        auth::set_auth_directory(directory.clone());
        register_company_catalog_hook();
        *registered = true;
    }
}

fn register_company_catalog_hook() {
    use futures_util::FutureExt;
    lariv_plugin_companies::hooks::set_after_create_company(Some(
        |db, company_id, _owner_user_id| {
            async move {
                let Some(directory) = DIRECTORY.get() else {
                    return Ok(());
                };
                let admin = directory.admin().await.map_err(|err| err.to_string())?;
                let template = template_company_id(admin, &directory.admin_email)
                    .await
                    .map_err(|err| err.to_string())?;
                if template == company_id {
                    return Ok(());
                }
                // Clone uses the admin pool; the request pool may be the new company.
                let _ = db;
                catalog::collapse_duplicate_catalog(admin, template)
                    .await
                    .map_err(|err| err.to_string())?;
                catalog::clone_catalogs(admin, template, company_id)
                    .await
                    .map_err(|err| err.to_string())?;
                // A company that already owns some catalog rows still needs the
                // Indian GST accounts, taxes, and preferences the full clone skips.
                crate::indian::ensure_indian_defaults(admin, template, company_id)
                    .await
                    .map_err(|err| err.to_string())
            }
            .boxed()
        },
    ));
}

async fn template_company_id(
    admin: &DatabaseConnection,
    admin_email: &str,
) -> Result<i64, UsersError> {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT m.company_id AS id
             FROM users u
             JOIN company_memberships m ON m.user_id = u.id AND m.owned_by
             WHERE u.email = $1
             ORDER BY m.id
             LIMIT 1",
            [admin_email.into()],
        ))
        .await?
        .ok_or_else(|| UsersError::validation("admin has no company to clone catalogs from"))?;
    row.try_get("", "id").map_err(UsersError::from)
}

pub async fn backfill_admin() -> anyhow::Result<()> {
    let Some(directory) = DIRECTORY.get() else {
        tracing::warn!("row-level security directory is not registered; skip ownership backfill");
        return Ok(());
    };
    let admin = directory.admin().await?;
    let email = directory.admin_email.clone();
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT id FROM users WHERE email = $1",
            [email.into()],
        ))
        .await?;
    let Some(row) = row else {
        tracing::warn!("no admin user to backfill; ownership stays empty until one exists");
        return Ok(());
    };
    let admin_id: i64 = row.try_get("", "id")?;
    // Serve startup (mdBook directories) runs after this seed and has no session user.
    crate::bootstrap::set_startup_user(admin_id);
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT rls_backfill($1)",
            [admin_id.into()],
        ))
        .await?;
    if let Some(company_row) = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT rls_admin_company_id($1) AS id",
            [admin_id.into()],
        ))
        .await?
    {
        if let Ok(company_id) = company_row.try_get::<i64>("", "id") {
            crate::bootstrap::set_startup_company(company_id);
            catalog::collapse_duplicate_catalog(admin, company_id).await?;
            crate::indian::ensure_indian_defaults_for_all(admin, company_id).await?;
        }
    }
    Ok(())
}

impl Directory {
    async fn admin(&self) -> Result<&DatabaseConnection, UsersError> {
        self.admin
            .get_or_try_init(|| Database::connect(self.admin_url.as_str()))
            .await
            .map_err(|err| UsersError::Other(err.into()))
    }
}

fn db_actor() -> Option<i64> {
    lariv_core::db::db_actor()
}

#[async_trait::async_trait]
impl AuthDirectory for Directory {
    async fn find_by_email(
        &self,
        db: &DatabaseConnection,
        email: &str,
    ) -> Result<Option<User>, UsersError> {
        UserEntity::find()
            .from_raw_sql(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT * FROM rls_find_user_by_email($1)",
                [email.into()],
            ))
            .one(db)
            .await
            .map_err(UsersError::from)
    }

    async fn create_user(
        &self,
        db: &DatabaseConnection,
        input: CreateUser,
    ) -> Result<User, UsersError> {
        let signup =
            current_auth().is_none() && db_actor().is_none() && input.role == Unassigned::NAME;
        let user = if signup {
            signup_user(db, &input).await?
        } else if input.role == Superuser::NAME || current_auth().is_some() {
            auth::insert_user_row(self.admin().await?, input.clone()).await?
        } else {
            return Err(UsersError::validation(
                "that role cannot be created from this session",
            ));
        };
        // Catalogs clone when a company is created, not when a user signs up.
        Ok(user)
    }
}

async fn signup_user(db: &DatabaseConnection, input: &CreateUser) -> Result<User, UsersError> {
    let salt = password::generate_salt();
    let hash = password::hash_password(input.plain_password.as_bytes(), &salt)?;
    let timezone = input
        .timezone
        .clone()
        .unwrap_or_else(|| "Asia/Kolkata".into());
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT rls_signup_user($1, $2, $3, $4, $5, $6) AS id",
            [
                input.name.clone().into(),
                input.email.clone().into(),
                input.phone.clone().into(),
                hash.into(),
                salt.into(),
                timezone.into(),
            ],
        ))
        .await?
        .ok_or_else(|| UsersError::validation("signup did not return a user"))?;
    let id: i64 = row.try_get("", "id")?;
    UserEntity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT * FROM rls_find_user_by_email($1)",
            [input.email.clone().into()],
        ))
        .one(db)
        .await?
        .ok_or_else(|| UsersError::validation(format!("signed up user {id} is not visible")))
}
