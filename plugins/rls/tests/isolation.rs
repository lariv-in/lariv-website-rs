//! Company membership isolates rows once policies are installed.
//!
//! Skips unless `DATABASE_URL` or `ADMIN_DATABASE_URL` is set. The test applies the
//! policy script to that database, so point it at a disposable copy of the website schema.

use sea_orm::sqlx::{Executor, Row};
use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

#[tokio::test]
async fn two_companies_cannot_see_each_others_currencies() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    let ready: bool = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT to_regclass('public.currencies') IS NOT NULL
                    AND to_regclass('public.company_memberships') IS NOT NULL AS ready"
                .to_owned(),
        ))
        .await
        .expect("schema probe")
        .expect("schema probe row")
        .try_get("", "ready")
        .expect("ready flag");
    if !ready {
        eprintln!("skipping rls isolation test: currencies/memberships missing");
        return;
    }

    admin
        .execute_unprepared(website_plugin_rls::install_sql())
        .await
        .expect("install policies");

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let owner_a = insert_user(&admin, &format!("rls-test-a-{stamp}@example.com")).await;
    let owner_b = insert_user(&admin, &format!("rls-test-b-{stamp}@example.com")).await;
    let member = insert_user(&admin, &format!("rls-test-m-{stamp}@example.com")).await;
    let company_a = insert_company(&admin, owner_a, &format!("Co A {stamp}")).await;
    let company_b = insert_company(&admin, owner_b, &format!("Co B {stamp}")).await;
    add_member(&admin, company_a, member, false).await;

    let app = connect_subject(&admin, &admin_url).await;
    let code = (stamp % 1_000_000) as i32;
    let id_a = insert_currency(&app, owner_a, company_a, code).await;
    let id_b = insert_currency(&app, owner_b, company_b, code).await;
    assert_ne!(id_a, id_b);

    assert_eq!(
        count_currencies(&app, owner_a, company_a, code).await,
        1,
        "owner A should see company A currency"
    );
    assert_eq!(
        count_currencies(&app, member, company_a, code).await,
        1,
        "member should see company A currency"
    );
    assert_eq!(
        count_currencies(&app, owner_b, company_b, code).await,
        1,
        "owner B should see company B currency"
    );
    assert_eq!(
        count_currencies(&app, owner_a, company_b, code).await,
        0,
        "owner A must not see company B rows even if they force that company GUC"
    );
    assert_eq!(
        count_currencies(&app, member, company_b, code).await,
        0,
        "member of A must not see company B"
    );
    assert_eq!(
        count_currencies(&app, 0, 0, code).await,
        0,
        "empty session should see nothing"
    );

    assert!(
        !can_update_company(&app, member, company_a).await,
        "member must not update the company"
    );
    assert!(
        can_update_company(&app, owner_a, company_a).await,
        "owner must update the company"
    );
    assert!(
        !can_add_member(&app, member, company_a, owner_b).await,
        "member must not add members"
    );
    assert!(
        can_add_member(&app, owner_a, company_a, owner_b).await,
        "owner must add members"
    );

    let other_user: i64 = visible_user(&app, owner_a, owner_b).await;
    assert_eq!(other_user, 0, "user A should not see user B");

    cleanup_companies(&admin, &[company_a, company_b], &[id_a, id_b], code).await;
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM users WHERE id IN ($1, $2, $3)",
            [owner_a.into(), owner_b.into(), member.into()],
        ))
        .await
        .expect("delete users");
}

#[tokio::test]
async fn switching_active_company_hides_other_company_rows() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    let ready: bool = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT to_regclass('public.company_memberships') IS NOT NULL AS ready".to_owned(),
        ))
        .await
        .expect("schema probe")
        .expect("row")
        .try_get("", "ready")
        .expect("ready");
    if !ready {
        return;
    }
    admin
        .execute_unprepared(website_plugin_rls::install_sql())
        .await
        .expect("install policies");

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let user = insert_user(&admin, &format!("rls-switch-{stamp}@example.com")).await;
    let company_a = insert_company(&admin, user, &format!("Switch A {stamp}")).await;
    let company_b = insert_company(&admin, user, &format!("Switch B {stamp}")).await;
    let app = connect_subject(&admin, &admin_url).await;
    let code = (stamp % 1_000_000) as i32;
    let id_a = insert_currency(&app, user, company_a, code).await;
    let id_b = insert_currency(&app, user, company_b, code + 1).await;

    assert_eq!(count_currencies(&app, user, company_a, code).await, 1);
    assert_eq!(count_currencies(&app, user, company_a, code + 1).await, 0);
    assert_eq!(count_currencies(&app, user, company_b, code + 1).await, 1);
    assert_eq!(count_currencies(&app, user, company_b, code).await, 0);

    cleanup_companies(&admin, &[company_a, company_b], &[id_a, id_b], code).await;
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM currencies WHERE id = $1",
            [id_b.into()],
        ))
        .await
        .ok();
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM users WHERE id = $1",
            [user.into()],
        ))
        .await
        .expect("delete user");
}

#[tokio::test]
async fn cloned_catalog_rows_stay_private_to_company() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    admin
        .execute_unprepared(website_plugin_rls::install_sql())
        .await
        .expect("install policies");
    let app = connect_subject(&admin, &admin_url).await;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let template_user = insert_user(&admin, &format!("rls-clone-src-{stamp}@example.com")).await;
    let newbie = insert_user(&admin, &format!("rls-clone-dst-{stamp}@example.com")).await;
    let template_company =
        insert_company(&admin, template_user, &format!("Template {stamp}")).await;
    let new_company = insert_company(&admin, newbie, &format!("Newbie {stamp}")).await;

    let preference = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "INSERT INTO payment_preferences (created_at, updated_at)
             VALUES (now(), now()) RETURNING id"
                .to_owned(),
        ))
        .await
        .expect("insert preference")
        .expect("preference id");
    let preference_id: i64 = preference.try_get("", "id").expect("preference id");
    let owners: String = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT rls_owners_name('payment_preferences') AS name".to_owned(),
        ))
        .await
        .expect("owners name")
        .expect("owners row")
        .try_get("", "name")
        .expect("name");
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!("INSERT INTO {owners} (record_id, company_id) VALUES ($1, $2)"),
            [preference_id.into(), template_company.into()],
        ))
        .await
        .expect("own preference");

    website_plugin_rls::clone_catalogs(&admin, template_company, new_company)
        .await
        .expect("clone catalog");

    let copied: i64 = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "SELECT o.record_id AS id FROM {owners} o
                 WHERE o.company_id = $1"
            ),
            [new_company.into()],
        ))
        .await
        .expect("copied preference")
        .expect("copied row")
        .try_get("", "id")
        .expect("copied id");
    assert_ne!(
        copied, preference_id,
        "clone reused the template preference id"
    );

    assert_eq!(
        count_owned(&app, template_user, template_company, copied).await,
        0,
        "template company saw the copy"
    );
    assert_eq!(
        count_owned(&app, newbie, new_company, preference_id).await,
        0,
        "new company saw the template row"
    );
    assert_eq!(
        count_owned(&app, newbie, new_company, copied).await,
        1,
        "new company should see their copy"
    );

    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM payment_preferences WHERE id IN ($1, $2)",
            [preference_id.into(), copied.into()],
        ))
        .await
        .expect("delete preferences");
    cleanup_companies(&admin, &[template_company, new_company], &[], 0).await;
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM users WHERE id IN ($1, $2)",
            [template_user.into(), newbie.into()],
        ))
        .await
        .expect("delete users");
}

#[tokio::test]
async fn indian_defaults_are_copied_onto_a_new_company() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    let template: Option<i64> = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT o.company_id AS id
             FROM accounts a
             JOIN rlsown_7a90e38a211ece1c346928e7d1f3e968 o ON o.record_id = a.id
             WHERE a.code = 20501
             ORDER BY o.company_id
             LIMIT 1"
                .to_owned(),
        ))
        .await
        .expect("template lookup")
        .and_then(|row| row.try_get("", "id").ok());
    let Some(template) = template else {
        return;
    };

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let user = insert_user(&admin, &format!("rls-indian-{stamp}@example.com")).await;
    let company = insert_company(&admin, user, &format!("Indian {stamp}")).await;

    website_plugin_rls::collapse_duplicate_catalog(&admin, template)
        .await
        .expect("collapse template duplicates");
    let template_currencies = owned_currency_count(&admin, template).await;
    let template_assets = owned_account_code(&admin, template, 10000).await;
    website_plugin_rls::clone_catalogs(&admin, template, company)
        .await
        .expect("clone catalogs");
    website_plugin_rls::ensure_indian_defaults_for_all(&admin, template)
        .await
        .expect("copy indian defaults");
    assert_eq!(owned_account_code(&admin, company, 20501).await, 1);
    assert_eq!(
        owned_account_code(&admin, company, 10000).await,
        template_assets,
        "new company should receive the template asset account"
    );
    assert_eq!(owned_tax_name(&admin, company, "CGST 9%").await, 1);
    assert_eq!(owned_journal_name(&admin, company, "General").await, 1);
    assert_eq!(
        owned_currency_code(&admin, company, 356).await,
        1,
        "new company should own exactly one INR"
    );
    assert_eq!(
        owned_currency_count(&admin, company).await,
        template_currencies,
        "new company should receive every template currency"
    );
    assert!(
        owned_journal_count(&admin, company).await < 20,
        "new company should not inherit dozens of duplicate journals"
    );

    website_plugin_rls::ensure_indian_defaults_for_all(&admin, template)
        .await
        .expect("copy indian defaults again");
    assert_eq!(
        owned_account_code(&admin, company, 20501).await,
        1,
        "second copy duplicated the GST account"
    );
    assert_eq!(
        owned_account_code(&admin, company, 10000).await,
        template_assets,
        "second copy duplicated the asset account"
    );
    assert_eq!(
        owned_currency_code(&admin, company, 356).await,
        1,
        "second copy duplicated INR"
    );
    assert_eq!(
        owned_currency_count(&admin, company).await,
        template_currencies,
        "second copy duplicated a currency"
    );

    delete_owned_indian_rows(&admin, company).await;
    admin
        .execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "ALTER TABLE company_memberships DISABLE TRIGGER rls_membership_keep_owner".to_owned(),
        ))
        .await
        .expect("disable owner trigger");
    cleanup_companies(&admin, &[company], &[], 0).await;
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM users WHERE id = $1",
            [user.into()],
        ))
        .await
        .expect("delete user");
    admin
        .execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "ALTER TABLE company_memberships ENABLE TRIGGER rls_membership_keep_owner".to_owned(),
        ))
        .await
        .expect("enable owner trigger");
}

async fn owned_currency_code(admin: &DatabaseConnection, company_id: i64, code: i32) -> i64 {
    let owners = currency_owners_table(admin).await;
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "SELECT count(*)::bigint AS n
                 FROM currencies c
                 JOIN {owners} o ON o.record_id = c.id
                 WHERE o.company_id = $1 AND c.code = $2"
            ),
            [company_id.into(), code.into()],
        ))
        .await
        .expect("count currencies")
        .expect("count row");
    row.try_get("", "n").expect("n")
}

async fn owned_currency_count(admin: &DatabaseConnection, company_id: i64) -> i64 {
    let owners = currency_owners_table(admin).await;
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "SELECT count(*)::bigint AS n
                 FROM currencies c
                 JOIN {owners} o ON o.record_id = c.id
                 WHERE o.company_id = $1"
            ),
            [company_id.into()],
        ))
        .await
        .expect("count currencies")
        .expect("count row");
    row.try_get("", "n").expect("n")
}

async fn owned_journal_count(admin: &DatabaseConnection, company_id: i64) -> i64 {
    let owners = journal_owners_table(admin).await;
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "SELECT count(*)::bigint AS n
                 FROM journals j
                 JOIN {owners} o ON o.record_id = j.id
                 WHERE o.company_id = $1"
            ),
            [company_id.into()],
        ))
        .await
        .expect("count journals")
        .expect("count row");
    row.try_get("", "n").expect("n")
}

async fn currency_owners_table(admin: &DatabaseConnection) -> String {
    let row = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT rls_owners_name('currencies') AS name".to_owned(),
        ))
        .await
        .expect("owners name")
        .expect("owners row");
    row.try_get("", "name").expect("name")
}

async fn journal_owners_table(admin: &DatabaseConnection) -> String {
    let row = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT rls_owners_name('journals') AS name".to_owned(),
        ))
        .await
        .expect("owners name")
        .expect("owners row");
    row.try_get("", "name").expect("name")
}

async fn owned_account_code(admin: &DatabaseConnection, company_id: i64, code: i32) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT count(*)::bigint AS n
             FROM accounts a
             JOIN rlsown_7a90e38a211ece1c346928e7d1f3e968 o ON o.record_id = a.id
             WHERE o.company_id = $1 AND a.code = $2"
                .to_owned(),
            [company_id.into(), code.into()],
        ))
        .await
        .expect("count accounts")
        .expect("count row");
    row.try_get("", "n").expect("n")
}

async fn owned_tax_name(admin: &DatabaseConnection, company_id: i64, name: &str) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT count(*)::bigint AS n
             FROM taxes t
             JOIN rlsown_a9bb50547acd911a486278929413c569 o ON o.record_id = t.id
             WHERE o.company_id = $1 AND t.name = $2"
                .to_owned(),
            [company_id.into(), name.into()],
        ))
        .await
        .expect("count taxes")
        .expect("count row");
    row.try_get("", "n").expect("n")
}

async fn owned_journal_name(admin: &DatabaseConnection, company_id: i64, name: &str) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT count(*)::bigint AS n
             FROM journals j
             JOIN rlsown_4ca1ac76c8091a68621ecbcff41f0f85 o ON o.record_id = j.id
             WHERE o.company_id = $1 AND j.name = $2"
                .to_owned(),
            [company_id.into(), name.into()],
        ))
        .await
        .expect("count journals")
        .expect("count row");
    row.try_get("", "n").expect("n")
}

async fn delete_owned_indian_rows(admin: &DatabaseConnection, company_id: i64) {
    let statements = [
        "DELETE FROM product_preferences_taxes l
         USING rlsown_50c11d88fbd5759008ebbd96853b1088 o
         WHERE o.product_preferences_id = l.product_preferences_id
           AND o.tax_id = l.tax_id
           AND o.company_id = $1",
        "DELETE FROM journals j
         USING rlsown_4ca1ac76c8091a68621ecbcff41f0f85 o
         WHERE o.record_id = j.id AND o.company_id = $1",
        "DELETE FROM accounting_preferences a
         USING rlsown_961e5c5cf8407efc7c1f1d5fd7ca974d o
         WHERE o.record_id = a.id AND o.company_id = $1",
        "DELETE FROM invoice_preferences a
         USING rlsown_57a7c950d384bf2491bca79341292b2a o
         WHERE o.record_id = a.id AND o.company_id = $1",
        "DELETE FROM payment_preferences a
         USING rlsown_0a05e29bf654e84a007c69e4f1ed6249 o
         WHERE o.record_id = a.id AND o.company_id = $1",
        "DELETE FROM product_preferences a
         USING rlsown_5b2ff78bff6c93f204178f58426ebe6b o
         WHERE o.record_id = a.id AND o.company_id = $1",
        "DELETE FROM taxes t
         USING rlsown_a9bb50547acd911a486278929413c569 o
         WHERE o.record_id = t.id AND o.company_id = $1",
        "DELETE FROM accounts a
         USING rlsown_7a90e38a211ece1c346928e7d1f3e968 o
         WHERE o.record_id = a.id AND o.company_id = $1",
        "DELETE FROM currencies c
         USING rlsown_7f24d9fc4140045241b3f97bcf36bf03 o
         WHERE o.record_id = c.id AND o.company_id = $1",
    ];
    for sql in statements {
        admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql.to_owned(),
                [company_id.into()],
            ))
            .await
            .expect("delete copied rows");
    }
}

#[tokio::test]
async fn gemini_key_stays_on_its_row() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    let ready: bool = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT to_regclass('public.llm_assistant_preferences') IS NOT NULL AS ready"
                .to_owned(),
        ))
        .await
        .expect("schema probe")
        .expect("schema probe row")
        .try_get("", "ready")
        .expect("ready flag");
    if !ready {
        eprintln!("skipping gemini key trigger test: llm_assistant_preferences does not exist");
        return;
    }

    admin
        .execute_unprepared(website_plugin_rls::install_sql())
        .await
        .expect("install policies");
    admin
        .execute_unprepared(include_str!("../../llm_assistant/src/gemini.sql"))
        .await
        .expect("install gemini key trigger");

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let existing = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT record_id, api_key FROM rls_canonical_gemini_pref()".to_owned(),
        ))
        .await
        .expect("canonical preference");
    let (canonical_id, _canonical_key, created) = match existing {
        Some(row) => {
            let id: Option<i64> = row.try_get("", "record_id").expect("record id");
            let key: Option<String> = row.try_get("", "api_key").expect("api key");
            match (id, key) {
                (Some(id), Some(key)) => (id, key, None),
                _ => {
                    let (id, key, superuser) = create_canonical_preference(&admin, stamp).await;
                    (id, key.clone(), Some((id, key, superuser)))
                }
            }
        }
        None => {
            let (id, key, superuser) = create_canonical_preference(&admin, stamp).await;
            (id, key.clone(), Some((id, key, superuser)))
        }
    };

    let member = insert_user(&admin, &format!("rls-gemini-member-{stamp}@example.com")).await;
    let company = insert_company(&admin, member, &format!("Gemini Co {stamp}")).await;
    let app = connect_subject(&admin, &admin_url).await;
    let member_pref = insert_preference_as(&app, member, company, "member-typed-key").await;
    let stored = preference_key(&admin, member_pref).await;
    assert_eq!(stored, "member-typed-key", "insert keeps the member key");

    let rewritten =
        update_preference_key_as(&app, member, company, member_pref, "member-replaced-key").await;
    assert_eq!(
        rewritten, "member-replaced-key",
        "a member write keeps that member key"
    );

    let rotated = format!("gemini-rotated-{stamp}");
    let mut conn = admin
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("UPDATE llm_assistant_preferences SET api_key = $1 WHERE id = $2")
        .bind(&rotated)
        .bind(canonical_id)
        .execute(&mut *conn)
        .await
        .expect("rotate superuser key");
    let followed: String =
        sea_orm::sqlx::query_scalar("SELECT api_key FROM llm_assistant_preferences WHERE id = $1")
            .bind(member_pref)
            .fetch_one(&mut *conn)
            .await
            .expect("read followed key");
    conn.execute("ROLLBACK").await.expect("rollback");
    assert_eq!(
        followed, "member-replaced-key",
        "rotating the superuser key leaves the member key alone"
    );

    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM llm_assistant_preferences WHERE id = $1",
            [member_pref.into()],
        ))
        .await
        .expect("delete member preference");
    if let Some((preference, _, _)) = created {
        admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM llm_assistant_preferences WHERE id = $1",
                [preference.into()],
            ))
            .await
            .expect("delete canonical preference");
    }
    let mut users = vec![member];
    if let Some((_, _, superuser)) = created {
        users.push(superuser);
    }
    // Drop the keep-owner guard so the last owner membership can be removed.
    admin
        .execute_unprepared(
            "ALTER TABLE company_memberships DISABLE TRIGGER rls_membership_keep_owner",
        )
        .await
        .expect("disable keep-owner");
    cleanup_companies(&admin, &[company], &[], 0).await;
    for user_id in users {
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM company_user_prefs WHERE user_id = $1",
                [user_id.into()],
            ))
            .await;
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM company_memberships WHERE user_id = $1",
                [user_id.into()],
            ))
            .await;
        admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM users WHERE id = $1",
                [user_id.into()],
            ))
            .await
            .expect("delete user");
    }
    admin
        .execute_unprepared(
            "ALTER TABLE company_memberships ENABLE TRIGGER rls_membership_keep_owner",
        )
        .await
        .expect("enable keep-owner");
}

async fn create_canonical_preference(
    admin: &DatabaseConnection,
    stamp: u128,
) -> (i64, String, i64) {
    let superuser = insert_user_role(
        admin,
        &format!("rls-gemini-su-{stamp}@example.com"),
        "superuser",
    )
    .await;
    let canonical_key = format!("gemini-canonical-{stamp}");
    let owners: String = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT rls_owners_name('llm_assistant_preferences') AS name".to_owned(),
        ))
        .await
        .expect("owners name")
        .expect("owners row")
        .try_get("", "name")
        .expect("name");
    let preference = insert_preference(admin, &canonical_key).await;
    own_unassigned_preference(admin, &owners, preference).await;
    (preference, canonical_key, superuser)
}

async fn insert_user_role(admin: &DatabaseConnection, email: &str, role: &str) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO users (created_at, updated_at, name, email, phone, role, timezone)
             VALUES (now(), now(), 'RLS test', $1, $2, $3, 'Asia/Kolkata')
             RETURNING id",
            [email.into(), email.into(), role.into()],
        ))
        .await
        .expect("insert user")
        .expect("user id");
    row.try_get("", "id").expect("id")
}

async fn insert_preference(admin: &DatabaseConnection, api_key: &str) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO llm_assistant_preferences (created_at, updated_at, api_key)
             VALUES (now(), now(), $1)
             RETURNING id",
            [api_key.into()],
        ))
        .await
        .expect("insert preference")
        .expect("preference id");
    row.try_get("", "id").expect("id")
}

async fn own_unassigned_preference(admin: &DatabaseConnection, owners: &str, record_id: i64) {
    // Admin inserts already fire the optional-company own trigger with a NULL
    // company. Keep this for databases that leave the row unowned.
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "INSERT INTO {owners} (record_id, company_id) VALUES ($1, NULL)
                 ON CONFLICT DO NOTHING"
            ),
            [record_id.into()],
        ))
        .await
        .expect("own unassigned preference");
}

async fn preference_key(admin: &DatabaseConnection, id: i64) -> String {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT api_key FROM llm_assistant_preferences WHERE id = $1",
            [id.into()],
        ))
        .await
        .expect("read key")
        .expect("key row");
    row.try_get("", "api_key").expect("api_key")
}

async fn insert_preference_as(
    app: &DatabaseConnection,
    user_id: i64,
    company_id: i64,
    api_key: &str,
) -> i64 {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set company");
    let row = sea_orm::sqlx::query(
        "INSERT INTO llm_assistant_preferences (created_at, updated_at, api_key)
         VALUES (now(), now(), $1)
         RETURNING id",
    )
    .bind(api_key)
    .fetch_one(&mut *conn)
    .await
    .expect("insert preference as user");
    let id: i64 = row.try_get("id").expect("id");
    conn.execute("COMMIT").await.expect("commit");
    id
}

async fn update_preference_key_as(
    app: &DatabaseConnection,
    user_id: i64,
    company_id: i64,
    id: i64,
    api_key: &str,
) -> String {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set company");
    let row = sea_orm::sqlx::query(
        "UPDATE llm_assistant_preferences SET api_key = $1 WHERE id = $2 RETURNING api_key",
    )
    .bind(api_key)
    .bind(id)
    .fetch_one(&mut *conn)
    .await
    .expect("update preference as user");
    let stored: String = row.try_get("api_key").expect("api_key");
    conn.execute("COMMIT").await.expect("commit");
    stored
}

#[tokio::test]
async fn clone_catalogs_fills_existing_companies() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    let ready: bool = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT to_regclass('public.accounts') IS NOT NULL
                    AND to_regclass('public.companies') IS NOT NULL AS ready"
                .to_owned(),
        ))
        .await
        .expect("schema probe")
        .expect("schema probe row")
        .try_get("", "ready")
        .expect("ready flag");
    if !ready {
        eprintln!("skipping catalog clone repair: accounts missing");
        return;
    }

    admin
        .execute_unprepared(website_plugin_rls::install_sql())
        .await
        .expect("install policies");

    let template = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT m.company_id AS id
             FROM users u
             JOIN company_memberships m ON m.user_id = u.id AND m.owned_by
             WHERE u.role = 'superuser'
             ORDER BY m.id
             LIMIT 1"
                .to_owned(),
        ))
        .await
        .expect("template lookup")
        .expect("template company");
    let template_id: i64 = template.try_get("", "id").expect("id");
    website_plugin_rls::collapse_duplicate_catalog(&admin, template_id)
        .await
        .expect("collapse template");
    let others = admin
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT id FROM companies WHERE id <> $1 ORDER BY id",
            [template_id.into()],
        ))
        .await
        .expect("list companies");
    for row in others {
        let company_id: i64 = row.try_get("", "id").expect("id");
        website_plugin_rls::clone_catalogs(&admin, template_id, company_id)
            .await
            .expect("clone catalogs");
        website_plugin_rls::ensure_indian_defaults(&admin, template_id, company_id)
            .await
            .expect("indian defaults");
        let accounts = owned_account_count(&admin, company_id).await;
        assert!(
            accounts > 5,
            "company {company_id} should own a full chart after clone, got {accounts}"
        );
    }
}

async fn owned_account_count(admin: &DatabaseConnection, company_id: i64) -> i64 {
    let owners = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT rls_owners_name('accounts') AS name".to_owned(),
        ))
        .await
        .expect("owners name")
        .expect("owners row");
    let owners: String = owners.try_get("", "name").expect("name");
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!("SELECT count(*)::bigint AS n FROM {owners} WHERE company_id = $1"),
            [company_id.into()],
        ))
        .await
        .expect("count")
        .expect("count row");
    row.try_get("", "n").expect("n")
}

#[tokio::test]
async fn member_cannot_delete_other_company_journal() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    let ready: bool = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT to_regclass('public.journals') IS NOT NULL
                    AND to_regclass('public.company_memberships') IS NOT NULL AS ready"
                .to_owned(),
        ))
        .await
        .expect("schema probe")
        .expect("schema probe row")
        .try_get("", "ready")
        .expect("ready flag");
    if !ready {
        eprintln!("skipping rls isolation test: journals missing");
        return;
    }

    admin
        .execute_unprepared(website_plugin_rls::install_sql())
        .await
        .expect("install policies");

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let owner_a = insert_user(&admin, &format!("rls-jdel-a-{stamp}@example.com")).await;
    let owner_b = insert_user(&admin, &format!("rls-jdel-b-{stamp}@example.com")).await;
    let company_a = insert_company(&admin, owner_a, &format!("JDel A {stamp}")).await;
    let company_b = insert_company(&admin, owner_b, &format!("JDel B {stamp}")).await;
    let currency = insert_owned_currency(&admin, company_a, (stamp % 1_000_000) as i32).await;
    let journal = insert_owned_journal(&admin, company_a, currency, &format!("JDel {stamp}")).await;

    let app = connect_subject(&admin, &admin_url).await;
    assert!(
        !can_delete_journal(&app, owner_b, company_b, journal).await,
        "member of B must not delete A's journal"
    );
    assert!(
        can_delete_journal(&app, owner_a, company_a, journal).await,
        "owner of A may delete A's journal"
    );

    let _ = admin
        .execute_unprepared(
            "ALTER TABLE company_memberships DISABLE TRIGGER rls_membership_keep_owner",
        )
        .await;
    cleanup_companies(&admin, &[company_a, company_b], &[currency], 0).await;
    let _ = admin
        .execute_unprepared(
            "ALTER TABLE company_memberships ENABLE TRIGGER rls_membership_keep_owner",
        )
        .await;
    for user_id in [owner_a, owner_b] {
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM users WHERE id = $1",
                [user_id.into()],
            ))
            .await;
    }
}

#[tokio::test]
async fn superuser_set_role_hides_other_companies() {
    let Some(admin_url) = configured_database_url() else {
        return;
    };
    let admin = connect_admin(&admin_url).await;
    let ready: bool = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT to_regclass('public.companies') IS NOT NULL
                    AND to_regclass('public.company_memberships') IS NOT NULL AS ready"
                .to_owned(),
        ))
        .await
        .expect("schema probe")
        .expect("schema probe row")
        .try_get("", "ready")
        .expect("ready flag");
    if !ready {
        eprintln!("skipping rls isolation test: companies missing");
        return;
    }

    admin
        .execute_unprepared(website_plugin_rls::install_sql())
        .await
        .expect("install policies");

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let owner = insert_user(&admin, &format!("rls-role-{stamp}@example.com")).await;
    let outsider = insert_user(&admin, &format!("rls-role-out-{stamp}@example.com")).await;
    let company = insert_company(&admin, owner, &format!("Role Co {stamp}")).await;
    let other = insert_company(&admin, outsider, &format!("Other Co {stamp}")).await;

    let as_role = visible_company_ids(&admin, owner, true).await;
    assert!(
        as_role.contains(&company),
        "request role should see the caller's company: {as_role:?}"
    );
    assert!(
        !as_role.contains(&other),
        "request role must hide companies the caller does not belong to: {as_role:?}"
    );
    let as_super = visible_company_ids(&admin, owner, false).await;
    assert!(
        as_super.contains(&other),
        "superuser login still sees every company before SET ROLE: {as_super:?}"
    );

    let _ = admin
        .execute_unprepared(
            "ALTER TABLE company_memberships DISABLE TRIGGER rls_membership_keep_owner",
        )
        .await;
    cleanup_companies(&admin, &[company, other], &[], 0).await;
    let _ = admin
        .execute_unprepared(
            "ALTER TABLE company_memberships ENABLE TRIGGER rls_membership_keep_owner",
        )
        .await;
    for user_id in [owner, outsider] {
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM users WHERE id = $1",
                [user_id.into()],
            ))
            .await;
    }
}

async fn visible_company_ids(admin: &DatabaseConnection, user_id: i64, set_role: bool) -> Vec<i64> {
    let mut conn = admin
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.rls_bypass', '', true)")
        .execute(&mut *conn)
        .await
        .expect("clear bypass");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', '', true)")
        .execute(&mut *conn)
        .await
        .expect("clear company");
    if set_role {
        sea_orm::sqlx::query("SET LOCAL ROLE lariv_rls")
            .execute(&mut *conn)
            .await
            .expect("set role");
    }
    let rows = sea_orm::sqlx::query("SELECT id FROM companies ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .expect("list companies");
    let ids = rows
        .iter()
        .map(|row| row.try_get("id").expect("id"))
        .collect();
    conn.execute("ROLLBACK").await.expect("rollback");
    ids
}

fn configured_database_url() -> Option<String> {
    for key in ["DATABASE_URL", "ADMIN_DATABASE_URL"] {
        if let Ok(url) = std::env::var(key) {
            if !url.is_empty() {
                return Some(url);
            }
        }
    }
    None
}

async fn connect_admin(url: &str) -> DatabaseConnection {
    website_plugin_rls::connect_privileged(url)
        .await
        .expect("database connection")
}

/// Connection that policies apply to.
///
/// A superuser or `BYPASSRLS` role never sees policies, so those URLs get a
/// disposable non-owner login. Any other user is used as-is.
async fn connect_subject(admin: &DatabaseConnection, url: &str) -> DatabaseConnection {
    let row = admin
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user".to_owned(),
        ))
        .await
        .expect("role attributes")
        .expect("role row");
    let superuser: bool = row.try_get("", "rolsuper").expect("rolsuper");
    let bypass: bool = row.try_get("", "rolbypassrls").expect("rolbypassrls");
    if !superuser && !bypass {
        return Database::connect(url).await.expect("subject connection");
    }
    let password = quote_literal(&url_password(url));
    admin
        .execute_unprepared(&format!(
            "DO $lariv_rls_role$ BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'lariv_rls_test') THEN
                    CREATE ROLE lariv_rls_test LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD {password};
                ELSE
                    ALTER ROLE lariv_rls_test WITH LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD {password};
                END IF;
            END $lariv_rls_role$;"
        ))
        .await
        .expect("subject role");
    let database = url_database(url);
    if !database.is_empty() {
        let database = quote_ident(&database);
        admin
            .execute_unprepared(&format!(
                "GRANT CONNECT ON DATABASE {database} TO lariv_rls_test"
            ))
            .await
            .expect("grant connect");
    }
    admin
        .execute_unprepared(
            "GRANT USAGE ON SCHEMA public TO lariv_rls_test;
             GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO lariv_rls_test;
             GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_find_user_by_email(text) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_signup_user(text, text, text, bytea, bytea, text) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_is_company_member(bigint) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_is_company_owner(bigint) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_company_has_members(bigint) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_current_company_id() TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_resolve_active_company() TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_is_superuser() TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_user_role(bigint) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_is_bypass() TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_row_visible_optional_company(regclass, bigint[]) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_filesystem_pref_visible(regclass, bigint) TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_own_new_row_optional_company() TO lariv_rls_test;
             GRANT EXECUTE ON FUNCTION rls_public_website_vnode(bigint) TO lariv_rls_test;",
        )
        .await
        .expect("grant subject");
    Database::connect(&replace_user(url, "lariv_rls_test"))
        .await
        .expect("subject connection")
}

fn url_password(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else {
        return String::new();
    };
    let Some((userinfo, _)) = rest.split_once('@') else {
        return String::new();
    };
    userinfo
        .split_once(':')
        .map(|(_, password)| password.to_owned())
        .unwrap_or_default()
}

fn url_database(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else {
        return String::new();
    };
    let host = rest.split_once('@').map(|(_, host)| host).unwrap_or(rest);
    host.split_once('/')
        .map(|(_, database)| database.split('?').next().unwrap_or("").to_owned())
        .unwrap_or_default()
}

fn replace_user(url: &str, username: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let Some((userinfo, host)) = rest.split_once('@') else {
        return format!("{scheme}://{username}@{rest}");
    };
    match userinfo.split_once(':') {
        Some((_, password)) => format!("{scheme}://{username}:{password}@{host}"),
        None => format!("{scheme}://{username}@{host}"),
    }
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn insert_user(admin: &DatabaseConnection, email: &str) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO users (created_at, updated_at, name, email, phone, role, timezone)
             VALUES (now(), now(), 'RLS test', $1, $2, 'unassigned', 'Asia/Kolkata')
             RETURNING id",
            [email.into(), email.into()],
        ))
        .await
        .expect("insert user")
        .expect("user id");
    row.try_get("", "id").expect("id")
}

async fn insert_company(admin: &DatabaseConnection, owner_id: i64, name: &str) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO companies (created_at, updated_at, name)
             VALUES (now(), now(), $1)
             RETURNING id",
            [name.into()],
        ))
        .await
        .expect("insert company")
        .expect("company id");
    let company_id: i64 = row.try_get("", "id").expect("id");
    add_member(admin, company_id, owner_id, true).await;
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO company_user_prefs (user_id, active_company_id, updated_at)
             VALUES ($1, $2, now())
             ON CONFLICT (user_id) DO UPDATE
             SET active_company_id = EXCLUDED.active_company_id,
                 updated_at = EXCLUDED.updated_at",
            [owner_id.into(), company_id.into()],
        ))
        .await
        .expect("set active company");
    company_id
}

async fn add_member(admin: &DatabaseConnection, company_id: i64, user_id: i64, owned_by: bool) {
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO company_memberships (created_at, updated_at, user_id, company_id, owned_by)
             VALUES (now(), now(), $1, $2, $3)
             ON CONFLICT DO NOTHING",
            [user_id.into(), company_id.into(), owned_by.into()],
        ))
        .await
        .expect("add member");
}

async fn insert_owned_currency(admin: &DatabaseConnection, company_id: i64, code: i32) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO currencies (created_at, updated_at, code, name, symbol, minor_unit)
             VALUES (now(), now(), $1, 'RLS test', 'T', 2)
             RETURNING id",
            [code.into()],
        ))
        .await
        .expect("insert currency")
        .expect("currency id");
    let id: i64 = row.try_get("", "id").expect("id");
    let owners = currency_owners_table(admin).await;
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!("INSERT INTO {owners} (record_id, company_id) VALUES ($1, $2)
                     ON CONFLICT DO NOTHING"),
            [id.into(), company_id.into()],
        ))
        .await
        .expect("own currency");
    id
}

async fn insert_owned_journal(
    admin: &DatabaseConnection,
    company_id: i64,
    currency_id: i64,
    name: &str,
) -> i64 {
    let row = admin
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO journals (created_at, updated_at, name, is_active, currency_id, is_mutable)
             VALUES (now(), now(), $1, true, $2, true)
             RETURNING id",
            [name.into(), currency_id.into()],
        ))
        .await
        .expect("insert journal")
        .expect("journal id");
    let id: i64 = row.try_get("", "id").expect("id");
    let owners = journal_owners_table(admin).await;
    admin
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!("INSERT INTO {owners} (record_id, company_id) VALUES ($1, $2)
                     ON CONFLICT DO NOTHING"),
            [id.into(), company_id.into()],
        ))
        .await
        .expect("own journal");
    id
}

async fn can_delete_journal(
    app: &DatabaseConnection,
    user_id: i64,
    company_id: i64,
    journal_id: i64,
) -> bool {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set company");
    let result = sea_orm::sqlx::query("DELETE FROM journals WHERE id = $1")
        .bind(journal_id)
        .execute(&mut *conn)
        .await;
    let _ = conn.execute("ROLLBACK").await;
    result.map(|r| r.rows_affected() > 0).unwrap_or(false)
}

async fn insert_currency(
    app: &DatabaseConnection,
    user_id: i64,
    company_id: i64,
    code: i32,
) -> i64 {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set company");
    let row = sea_orm::sqlx::query(
        "INSERT INTO currencies (created_at, updated_at, code, name, symbol, minor_unit)
         VALUES (now(), now(), $1, 'RLS test', 'T', 2)
         RETURNING id",
    )
    .bind(code)
    .fetch_one(&mut *conn)
    .await
    .expect("insert currency");
    let id: i64 = row.try_get("id").expect("id");
    conn.execute("COMMIT").await.expect("commit");
    id
}

async fn count_currencies(
    app: &DatabaseConnection,
    user_id: i64,
    company_id: i64,
    code: i32,
) -> i64 {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    let user = if user_id == 0 {
        String::new()
    } else {
        user_id.to_string()
    };
    let company = if company_id == 0 {
        String::new()
    } else {
        company_id.to_string()
    };
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user)
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company)
        .execute(&mut *conn)
        .await
        .expect("set company");
    let row = sea_orm::sqlx::query("SELECT count(*)::bigint AS n FROM currencies WHERE code = $1")
        .bind(code)
        .fetch_one(&mut *conn)
        .await
        .expect("count");
    let n: i64 = row.try_get("n").expect("n");
    conn.execute("ROLLBACK").await.expect("rollback");
    n
}

async fn can_update_company(app: &DatabaseConnection, user_id: i64, company_id: i64) -> bool {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set company");
    let result = sea_orm::sqlx::query("UPDATE companies SET name = name WHERE id = $1")
        .bind(company_id)
        .execute(&mut *conn)
        .await;
    let _ = conn.execute("ROLLBACK").await;
    result.map(|r| r.rows_affected() > 0).unwrap_or(false)
}

async fn can_add_member(
    app: &DatabaseConnection,
    actor: i64,
    company_id: i64,
    grantee: i64,
) -> bool {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(actor.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set company");
    let result = sea_orm::sqlx::query(
        "INSERT INTO company_memberships (created_at, updated_at, user_id, company_id, owned_by)
         VALUES (now(), now(), $1, $2, false)",
    )
    .bind(grantee)
    .bind(company_id)
    .execute(&mut *conn)
    .await;
    let _ = conn.execute("ROLLBACK").await;
    result.is_ok()
}

async fn count_owned(
    app: &DatabaseConnection,
    user_id: i64,
    company_id: i64,
    record_id: i64,
) -> i64 {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    sea_orm::sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut *conn)
        .await
        .expect("set company");
    let row =
        sea_orm::sqlx::query("SELECT count(*)::bigint AS n FROM payment_preferences WHERE id = $1")
            .bind(record_id)
            .fetch_one(&mut *conn)
            .await
            .expect("count owned");
    let n: i64 = row.try_get("n").expect("n");
    conn.execute("ROLLBACK").await.expect("rollback");
    n
}

async fn visible_user(app: &DatabaseConnection, actor: i64, other: i64) -> i64 {
    let mut conn = app
        .get_postgres_connection_pool()
        .acquire()
        .await
        .expect("acquire");
    conn.execute("BEGIN").await.expect("begin");
    sea_orm::sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(actor.to_string())
        .execute(&mut *conn)
        .await
        .expect("set user");
    let row = sea_orm::sqlx::query("SELECT count(*)::bigint AS n FROM users WHERE id = $1")
        .bind(other)
        .fetch_one(&mut *conn)
        .await
        .expect("count users");
    let n: i64 = row.try_get("n").expect("n");
    conn.execute("ROLLBACK").await.expect("rollback");
    n
}

async fn cleanup_companies(
    admin: &DatabaseConnection,
    companies: &[i64],
    currency_ids: &[i64],
    code: i32,
) {
    for id in currency_ids {
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM currencies WHERE id = $1",
                [(*id).into()],
            ))
            .await;
    }
    if code != 0 {
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM currencies WHERE code = $1",
                [code.into()],
            ))
            .await;
    }
    for company_id in companies {
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM company_user_prefs WHERE active_company_id = $1 OR user_id IN (
                    SELECT user_id FROM company_memberships WHERE company_id = $1
                 )",
                [(*company_id).into()],
            ))
            .await;
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM company_memberships WHERE company_id = $1",
                [(*company_id).into()],
            ))
            .await;
        let _ = admin
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM companies WHERE id = $1",
                [(*company_id).into()],
            ))
            .await;
    }
}
