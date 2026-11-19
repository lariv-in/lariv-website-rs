//! Copy the Indian GST defaults onto every company.
//!
//! `finance_indian` inserts the chart fragment, taxes, ledger, and preferences
//! once. Row-level security then assigns those rows to the template company.
//! Other companies keep their own books, so each one needs its own copy, including
//! companies created after the seed has already run.

use lariv_plugin_users::error::UsersError;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};

/// Liability group the GST accounts hang from, then the GST group and its leaves.
const GST_ACCOUNT_CODES: &[i32] = &[20000, 20500, 20501, 20502, 20503];

const GST_TAX_NAMES: &[&str] = &[
    "CGST 2.5%",
    "SGST 2.5%",
    "IGST 5%",
    "CGST 6%",
    "SGST 6%",
    "IGST 12%",
    "CGST 9%",
    "SGST 9%",
    "IGST 18%",
    "CGST 14%",
    "SGST 14%",
    "IGST 28%",
    "CGST 1.5%",
    "SGST 1.5%",
    "IGST 3%",
    "CGST 0.125%",
    "SGST 0.125%",
    "IGST 0.25%",
    "CGST 0.75%",
    "SGST 0.75%",
    "IGST 1.5%",
];

const DEFAULT_PRODUCT_TAXES: &[&str] = &["CGST 9%", "SGST 9%"];

const INR_CODE: i32 = 356;

/// Give every company the template company's Indian defaults.
pub async fn ensure_indian_defaults_for_all(
    db: &DatabaseConnection,
    template_company: i64,
) -> Result<(), UsersError> {
    if !table_exists(db, "companies").await? || !table_exists(db, "accounts").await? {
        return Ok(());
    }
    let rows = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT id FROM companies ORDER BY id".to_owned(),
        ))
        .await?;
    for row in rows {
        let company_id: i64 = row.try_get("", "id")?;
        ensure_indian_defaults(db, template_company, company_id).await?;
    }
    Ok(())
}

/// Copy any missing Indian default rows from `template_company` onto `company_id`.
pub async fn ensure_indian_defaults(
    db: &DatabaseConnection,
    template_company: i64,
    company_id: i64,
) -> Result<(), UsersError> {
    if !table_exists(db, "accounts").await? {
        return Ok(());
    }
    // Stamp company so `rls_unique_per_owner` applies on admin-pool inserts.
    with_company_guc(db, company_id, async {
        let currency_id = ensure_inr(db, template_company, company_id).await?;
        ensure_gst_accounts(db, template_company, company_id).await?;
        ensure_gst_taxes(db, template_company, company_id).await?;
        if let Some(currency_id) = currency_id {
            ensure_general_journal(db, company_id, currency_id).await?;
            ensure_accounting_preferences(db, company_id, currency_id).await?;
        }
        ensure_preference_row(db, "invoice_preferences", company_id).await?;
        ensure_preference_row(db, "payment_preferences", company_id).await?;
        ensure_preference_row(db, "product_preferences", company_id).await?;
        ensure_product_tax_links(db, company_id).await?;
        Ok(())
    })
    .await
}

async fn with_company_guc<T, F>(
    db: &DatabaseConnection,
    company_id: i64,
    fut: F,
) -> Result<T, UsersError>
where
    F: std::future::Future<Output = Result<T, UsersError>>,
{
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT set_config('app.company_id', $1::text, true)",
        [company_id.to_string().into()],
    ))
    .await?;
    let result = fut.await;
    let _ = db
        .execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT set_config('app.company_id', '', true)".to_owned(),
        ))
        .await;
    result
}

async fn ensure_inr(
    db: &impl ConnectionTrait,
    template_company: i64,
    company_id: i64,
) -> Result<Option<i64>, UsersError> {
    if !table_exists(db, "currencies").await? {
        return Ok(None);
    }
    let owners = owners_table(db, "currencies").await?;
    if let Some(id) = owned_id(
        db,
        &owners,
        "currencies",
        company_id,
        "t.code = $2",
        &[INR_CODE.into()],
    )
    .await?
    {
        return Ok(Some(id));
    }
    let sql = format!(
        "INSERT INTO currencies (created_at, updated_at, code, name, symbol, minor_unit)
         SELECT now(), now(), c.code, c.name, c.symbol, c.minor_unit
         FROM currencies c
         JOIN {owners} o ON o.record_id = c.id
         WHERE o.company_id = $1 AND c.code = $2
         ORDER BY c.id
         LIMIT 1
         RETURNING id"
    );
    let Some(row) = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [template_company.into(), INR_CODE.into()],
        ))
        .await?
    else {
        tracing::warn!("template company has no INR currency to copy");
        return Ok(None);
    };
    let id: i64 = row.try_get("", "id")?;
    own_record(db, &owners, id, company_id).await?;
    Ok(Some(id))
}

async fn ensure_gst_accounts(
    db: &impl ConnectionTrait,
    template_company: i64,
    company_id: i64,
) -> Result<(), UsersError> {
    let owners = owners_table(db, "accounts").await?;
    for code in GST_ACCOUNT_CODES {
        if owned_id(
            db,
            &owners,
            "accounts",
            company_id,
            "t.code = $2",
            &[(*code).into()],
        )
        .await?
        .is_some()
        {
            continue;
        }
        let parent = template_parent_code(db, &owners, template_company, *code).await?;
        let parent_id = match parent {
            Some(parent_code) => {
                owned_id(
                    db,
                    &owners,
                    "accounts",
                    company_id,
                    "t.code = $2",
                    &[parent_code.into()],
                )
                .await?
            }
            None => None,
        };
        let sql = format!(
            "INSERT INTO accounts (
                 created_at, updated_at, name, code, is_group, balance_type, parent_id
             )
             SELECT now(), now(), a.name, a.code, a.is_group, a.balance_type, $3
             FROM accounts a
             JOIN {owners} o ON o.record_id = a.id
             WHERE o.company_id = $1 AND a.code = $2
             ORDER BY a.id
             LIMIT 1
             RETURNING id"
        );
        let Some(row) = db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                [template_company.into(), (*code).into(), parent_id.into()],
            ))
            .await?
        else {
            tracing::warn!(code, "template company has no account to copy");
            continue;
        };
        let id: i64 = row.try_get("", "id")?;
        own_record(db, &owners, id, company_id).await?;
    }
    Ok(())
}

async fn template_parent_code(
    db: &impl ConnectionTrait,
    owners: &str,
    template_company: i64,
    code: i32,
) -> Result<Option<i32>, UsersError> {
    let sql = format!(
        "SELECT p.code AS parent_code
         FROM accounts a
         JOIN {owners} o ON o.record_id = a.id
         LEFT JOIN accounts p ON p.id = a.parent_id
         WHERE o.company_id = $1 AND a.code = $2
         ORDER BY a.id
         LIMIT 1"
    );
    let Some(row) = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [template_company.into(), code.into()],
        ))
        .await?
    else {
        return Ok(None);
    };
    let parent: Option<i32> = row.try_get("", "parent_code")?;
    Ok(parent)
}

async fn ensure_gst_taxes(
    db: &impl ConnectionTrait,
    template_company: i64,
    company_id: i64,
) -> Result<(), UsersError> {
    if !table_exists(db, "taxes").await? {
        return Ok(());
    }
    let owners = owners_table(db, "taxes").await?;
    for name in GST_TAX_NAMES {
        if owned_id(
            db,
            &owners,
            "taxes",
            company_id,
            "t.name = $2",
            &[(*name).into()],
        )
        .await?
        .is_some()
        {
            continue;
        }
        let sql = format!(
            "INSERT INTO taxes (created_at, updated_at, name, percentage, tax_type)
             SELECT now(), now(), t.name, t.percentage, t.tax_type
             FROM taxes t
             JOIN {owners} o ON o.record_id = t.id
             WHERE o.company_id = $1 AND t.name = $2
             ORDER BY t.id
             LIMIT 1
             RETURNING id"
        );
        let Some(row) = db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                [template_company.into(), (*name).into()],
            ))
            .await?
        else {
            tracing::warn!(name, "template company has no tax to copy");
            continue;
        };
        let id: i64 = row.try_get("", "id")?;
        own_record(db, &owners, id, company_id).await?;
    }
    Ok(())
}

async fn ensure_general_journal(
    db: &impl ConnectionTrait,
    company_id: i64,
    currency_id: i64,
) -> Result<(), UsersError> {
    if !table_exists(db, "journals").await? {
        return Ok(());
    }
    let owners = owners_table(db, "journals").await?;
    if owned_id(
        db,
        &owners,
        "journals",
        company_id,
        "t.name = $2",
        &["General".into()],
    )
    .await?
    .is_some()
    {
        return Ok(());
    }
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO journals (
                 created_at, updated_at, name, is_active, currency_id, is_mutable
             )
             VALUES (now(), now(), 'General', true, $1, false)
             RETURNING id"
                .to_owned(),
            [currency_id.into()],
        ))
        .await?
        .ok_or_else(|| UsersError::validation("inserting the General journal returned no id"))?;
    let id: i64 = row.try_get("", "id")?;
    own_record(db, &owners, id, company_id).await?;
    Ok(())
}

async fn ensure_accounting_preferences(
    db: &impl ConnectionTrait,
    company_id: i64,
    currency_id: i64,
) -> Result<(), UsersError> {
    if !table_exists(db, "accounting_preferences").await? {
        return Ok(());
    }
    let owners = owners_table(db, "accounting_preferences").await?;
    let existing = owned_id(
        db,
        &owners,
        "accounting_preferences",
        company_id,
        "TRUE",
        &[],
    )
    .await?;
    if existing.is_some() {
        let sql = format!(
            "UPDATE accounting_preferences a
             SET default_currency_id = $2, updated_at = now()
             FROM {owners} o
             WHERE o.record_id = a.id
               AND o.company_id = $1
               AND a.default_currency_id IS NULL"
        );
        db.execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [company_id.into(), currency_id.into()],
        ))
        .await?;
        return Ok(());
    }
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO accounting_preferences (
                 created_at, updated_at, default_currency_id
             )
             VALUES (now(), now(), $1)
             RETURNING id"
                .to_owned(),
            [currency_id.into()],
        ))
        .await?
        .ok_or_else(|| UsersError::validation("inserting accounting preferences returned no id"))?;
    let id: i64 = row.try_get("", "id")?;
    own_record(db, &owners, id, company_id).await?;
    Ok(())
}

async fn ensure_preference_row(
    db: &impl ConnectionTrait,
    table: &str,
    company_id: i64,
) -> Result<(), UsersError> {
    if !is_safe_ident(table) || !table_exists(db, table).await? {
        return Ok(());
    }
    let owners = owners_table(db, table).await?;
    if owned_id(db, &owners, table, company_id, "TRUE", &[])
        .await?
        .is_some()
    {
        return Ok(());
    }
    let sql =
        format!("INSERT INTO {table} (created_at, updated_at) VALUES (now(), now()) RETURNING id");
    let row = db
        .query_one_raw(Statement::from_string(DatabaseBackend::Postgres, sql))
        .await?
        .ok_or_else(|| UsersError::validation(format!("inserting {table} returned no id")))?;
    let id: i64 = row.try_get("", "id")?;
    own_record(db, &owners, id, company_id).await?;
    Ok(())
}

async fn ensure_product_tax_links(
    db: &impl ConnectionTrait,
    company_id: i64,
) -> Result<(), UsersError> {
    if !table_exists(db, "product_preferences_taxes").await? {
        return Ok(());
    }
    let pref_owners = owners_table(db, "product_preferences").await?;
    let tax_owners = owners_table(db, "taxes").await?;
    let link_owners = owners_table(db, "product_preferences_taxes").await?;
    let prefs = db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "SELECT t.id AS id
                 FROM product_preferences t
                 JOIN {pref_owners} o ON o.record_id = t.id
                 WHERE o.company_id = $1
                 ORDER BY t.id"
            ),
            [company_id.into()],
        ))
        .await?;
    for pref in prefs {
        let pref_id: i64 = pref.try_get("", "id")?;
        for name in DEFAULT_PRODUCT_TAXES {
            let Some(tax_id) = owned_id(
                db,
                &tax_owners,
                "taxes",
                company_id,
                "t.name = $2",
                &[(*name).into()],
            )
            .await?
            else {
                continue;
            };
            let inserted = db
                .query_one_raw(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    "INSERT INTO product_preferences_taxes (product_preferences_id, tax_id)
                     VALUES ($1, $2)
                     ON CONFLICT DO NOTHING
                     RETURNING product_preferences_id"
                        .to_owned(),
                    [pref_id.into(), tax_id.into()],
                ))
                .await?;
            if inserted.is_none() {
                continue;
            }
            db.execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                format!(
                    "INSERT INTO {link_owners} (product_preferences_id, tax_id, company_id)
                     VALUES ($1, $2, $3)
                     ON CONFLICT DO NOTHING"
                ),
                [pref_id.into(), tax_id.into(), company_id.into()],
            ))
            .await?;
        }
    }
    Ok(())
}

async fn owned_id(
    db: &impl ConnectionTrait,
    owners: &str,
    table: &str,
    company_id: i64,
    predicate: &str,
    extra: &[sea_orm::Value],
) -> Result<Option<i64>, UsersError> {
    if !is_safe_ident(table) || !is_safe_ident(owners) {
        return Err(UsersError::validation(format!("unsafe table {table}")));
    }
    let sql = format!(
        "SELECT t.id AS id
         FROM {table} t
         JOIN {owners} o ON o.record_id = t.id
         WHERE o.company_id = $1 AND {predicate}
         ORDER BY t.id
         LIMIT 1"
    );
    let mut params = Vec::with_capacity(extra.len() + 1);
    params.push(company_id.into());
    params.extend(extra.iter().cloned());
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            params,
        ))
        .await?;
    row.map(|row| row.try_get("", "id"))
        .transpose()
        .map_err(UsersError::from)
}

async fn own_record(
    db: &impl ConnectionTrait,
    owners: &str,
    record_id: i64,
    company_id: i64,
) -> Result<(), UsersError> {
    if !is_safe_ident(owners) {
        return Err(UsersError::validation(format!(
            "unsafe owners table {owners}"
        )));
    }
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        format!(
            "INSERT INTO {owners} (record_id, company_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING"
        ),
        [record_id.into(), company_id.into()],
    ))
    .await?;
    Ok(())
}

async fn owners_table(db: &impl ConnectionTrait, table: &str) -> Result<String, UsersError> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT rls_owners_name($1) AS name",
            [table.into()],
        ))
        .await?
        .ok_or_else(|| UsersError::validation(format!("no owners table for {table}")))?;
    let name: String = row.try_get("", "name")?;
    if !is_safe_ident(&name) {
        return Err(UsersError::validation(format!(
            "unsafe owners table {name}"
        )));
    }
    Ok(name)
}

async fn table_exists(db: &impl ConnectionTrait, table: &str) -> Result<bool, UsersError> {
    if !is_safe_ident(table) {
        return Ok(false);
    }
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT to_regclass($1) IS NOT NULL AS present",
            [format!("public.{table}").into()],
        ))
        .await?
        .ok_or_else(|| UsersError::validation(format!("could not look up {table}")))?;
    row.try_get("", "present").map_err(UsersError::from)
}

fn is_safe_ident(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
