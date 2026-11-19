//! Copy the template company's chart, journals, taxes, and preferences.

use std::collections::{HashMap, HashSet, VecDeque};

use lariv_plugin_users::error::UsersError;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, TransactionTrait};
use serde_json::{Map, Value};

pub async fn clone_catalogs(
    db: &DatabaseConnection,
    template_company: i64,
    new_company: i64,
) -> Result<(), UsersError> {
    if template_company == new_company {
        return Ok(());
    }
    let txn = db.begin().await.map_err(UsersError::from)?;
    let copied = async {
        // Uniqueness and the owner trigger follow the session company. Stamp the
        // destination so a second company is not rejected for codes the active
        // company already owns.
        txn.execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT set_config('app.company_id', $1::text, true)",
            [new_company.to_string().into()],
        ))
        .await?;
        let tables = catalog_tables(&txn).await?;
        if tables.is_empty() {
            return Ok(());
        }
        let fks = foreign_keys(&txn, &tables).await?;
        let order = topo_order(&tables, &fks)?;
        let mut ids: HashMap<(String, i64), i64> = HashMap::new();
        for table in order {
            // Keyed catalogs fill in missing codes. Skipping the whole table
            // left companies that already owned the GST fragment without the
            // rest of the chart. Preference rows have no natural key, so an
            // existing row still means "already cloned".
            if dedupe_key_column(&table).is_none()
                && company_owns_any(&txn, new_company, &table).await?
            {
                continue;
            }
            // A later table (tax links that point at another company's tax)
            // must not roll back the chart and currencies already copied.
            txn.execute_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                "SAVEPOINT catalog_copy".to_owned(),
            ))
            .await?;
            if let Err(err) =
                copy_table(&txn, &table, template_company, new_company, &fks, &mut ids).await
            {
                txn.execute_raw(Statement::from_string(
                    DatabaseBackend::Postgres,
                    "ROLLBACK TO SAVEPOINT catalog_copy".to_owned(),
                ))
                .await?;
                txn.execute_raw(Statement::from_string(
                    DatabaseBackend::Postgres,
                    "RELEASE SAVEPOINT catalog_copy".to_owned(),
                ))
                .await?;
                if table == "accounts" || table == "currencies" {
                    return Err(err);
                }
                tracing::warn!(%err, %table, "skipped catalog table");
                continue;
            }
            txn.execute_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                "RELEASE SAVEPOINT catalog_copy".to_owned(),
            ))
            .await?;
        }
        Ok(())
    }
    .await;
    match copied {
        Ok(()) => txn.commit().await.map_err(UsersError::from),
        Err(err) => {
            let _ = txn.rollback().await;
            Err(err)
        }
    }
}

fn is_catalog_name(name: &str) -> bool {
    // Nodes/logs stay out of the finance catalog clone. Preferences are
    // company-scoped and clone with the other *_preferences tables.
    if matches!(name, "filesystem_nodes" | "logs" | "global_preferences") {
        return false;
    }
    // Currencies stay in the clone. Journals require `currency_id`, and dropping
    // currencies nulls that column and rolls the whole catalog copy back.
    // Same-code rows are collapsed before insert.
    matches!(
        name,
        "currencies" | "accounts" | "journals" | "taxes" | "fiscal_years"
    ) || name.ends_with("_preferences")
        || name.ends_with("_templates")
        || name.contains("journal_entry_item_template")
        || name.contains("preference_journal")
}

/// Natural key column used to keep one template row per business key when cloning.
fn dedupe_key_column(table: &str) -> Option<&'static str> {
    match table {
        "accounts" | "currencies" => Some("code"),
        "journals" | "taxes" => Some("name"),
        _ => None,
    }
}

async fn catalog_tables(db: &impl ConnectionTrait) -> Result<Vec<String>, UsersError> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT c.relname AS name
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public' AND c.relkind = 'r'
             ORDER BY c.relname"
                .to_owned(),
        ))
        .await?;
    let mut names = Vec::new();
    for row in rows {
        let name: String = row.try_get("", "name")?;
        if is_safe_ident(&name) && is_catalog_name(&name) && !is_owners_table(&name) {
            names.push(name);
        }
    }
    let mut selected: HashSet<String> = names.into_iter().collect();
    let all = all_tables(db).await?;
    let mut grew = true;
    while grew {
        grew = false;
        for table in &all {
            if selected.contains(table) || !is_safe_ident(table) || !is_catalog_child(table) {
                continue;
            }
            let edges: Vec<_> = fks_for_all(db, table).await?;
            if edges.is_empty() {
                continue;
            }
            let only_catalog = edges.iter().all(|(_, dst)| selected.contains(dst));
            if only_catalog {
                selected.insert(table.clone());
                grew = true;
            }
        }
    }
    // A name match can still point outside the catalog (mdBook files, product
    // lines). Copying those nulls a required foreign key and rolls back the
    // accounts and currencies that were already inserted.
    let mut pruned = true;
    while pruned {
        pruned = false;
        let current: Vec<String> = selected.iter().cloned().collect();
        for table in current {
            let edges = fks_for_all(db, &table).await?;
            let external = edges
                .iter()
                .any(|(_, dst)| dst != &table && !selected.contains(dst));
            if external {
                selected.remove(&table);
                pruned = true;
            }
        }
    }
    let mut out: Vec<_> = selected.into_iter().collect();
    out.sort();
    Ok(out)
}

fn is_owners_table(name: &str) -> bool {
    name.starts_with("rlsown_") || name.ends_with("_owners")
}

fn is_catalog_child(name: &str) -> bool {
    name != "global_preferences"
        && !is_owners_table(name)
        && (name.contains("preference")
            || name.contains("template")
            || name.ends_with("_tax")
            || name.ends_with("_taxes"))
}

async fn all_tables(db: &impl ConnectionTrait) -> Result<Vec<String>, UsersError> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT c.relname AS name
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public' AND c.relkind = 'r'
               AND c.relname NOT LIKE '%\\_owners' ESCAPE '\\'
               AND c.relname NOT IN ('users', 'seaql_migrations', 'roles')
             ORDER BY 1"
                .to_owned(),
        ))
        .await?;
    let mut names = Vec::new();
    for row in rows {
        names.push(row.try_get("", "name")?);
    }
    Ok(names)
}

async fn fks_for_all(
    db: &impl ConnectionTrait,
    table: &str,
) -> Result<Vec<(String, String)>, UsersError> {
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT src.attname AS col, dst_cls.relname AS dst
             FROM pg_constraint c
             JOIN pg_class src_cls ON src_cls.oid = c.conrelid
             JOIN pg_class dst_cls ON dst_cls.oid = c.confrelid
             JOIN unnest(c.conkey) WITH ORDINALITY AS ck(attnum, ord) ON true
             JOIN pg_attribute src
               ON src.attrelid = src_cls.oid AND src.attnum = ck.attnum
             WHERE c.contype = 'f' AND src_cls.relname = $1",
            [table.into()],
        ))
        .await?;
    let mut edges = Vec::new();
    for row in rows {
        let col: String = row.try_get("", "col")?;
        let dst: String = row.try_get("", "dst")?;
        edges.push((col, dst));
    }
    Ok(edges)
}

async fn company_owns_any(
    db: &impl ConnectionTrait,
    company_id: i64,
    table: &str,
) -> Result<bool, UsersError> {
    if !is_safe_ident(table) {
        return Ok(false);
    }
    let owners = owners_table(db, table).await?;
    let sql = format!("SELECT EXISTS (SELECT 1 FROM {owners} WHERE company_id = $1) AS owned");
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [company_id.into()],
        ))
        .await?;
    Ok(row
        .map(|row| row.try_get::<bool>("", "owned"))
        .transpose()?
        .unwrap_or(false))
}

/// `Some(generated)` when the table's primary key is a single `id` column.
/// `generated` means the database assigns the next id.
async fn surrogate_id(db: &impl ConnectionTrait, table: &str) -> Result<Option<bool>, UsersError> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT string_agg(a.attname, ',' ORDER BY a.attnum) AS cols,
                    COALESCE(bool_or(
                        a.attname = 'id'
                        AND (
                            a.attidentity <> ''
                            OR COALESCE(pg_get_expr(d.adbin, d.adrelid), '') LIKE 'nextval(%'
                        )
                    ), false) AS generated
             FROM pg_index i
             JOIN pg_attribute a
               ON a.attrelid = i.indrelid AND a.attnum = ANY (i.indkey)
             LEFT JOIN pg_attrdef d
               ON d.adrelid = a.attrelid AND d.adnum = a.attnum
             WHERE i.indrelid = format('public.%I', $1)::regclass
               AND i.indisprimary",
            [table.into()],
        ))
        .await?
        .ok_or_else(|| UsersError::validation(format!("{table} has no primary key")))?;
    let cols: String = row.try_get("", "cols")?;
    let generated: bool = row.try_get("", "generated")?;
    if cols != "id" {
        return Ok(None);
    }
    Ok(Some(generated))
}

async fn foreign_keys(
    db: &impl ConnectionTrait,
    tables: &[String],
) -> Result<HashMap<String, Vec<(String, String)>>, UsersError> {
    let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for table in tables {
        let mut edges = Vec::new();
        for (col, dst) in fks_for_all(db, table).await? {
            if tables.iter().any(|name| name == &dst) {
                edges.push((col, dst));
            }
        }
        map.insert(table.clone(), edges);
    }
    Ok(map)
}

fn topo_order(
    tables: &[String],
    fks: &HashMap<String, Vec<(String, String)>>,
) -> Result<Vec<String>, UsersError> {
    let mut indegree: HashMap<&str, usize> = tables.iter().map(|t| (t.as_str(), 0)).collect();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for table in tables {
        for (_, dst) in fks.get(table).into_iter().flatten() {
            if dst == table {
                continue;
            }
            if let Some(degree) = indegree.get_mut(table.as_str()) {
                *degree += 1;
            }
            outgoing
                .entry(dst.as_str())
                .or_default()
                .push(table.as_str());
        }
    }
    let mut queue: VecDeque<&str> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(name, _)| *name)
        .collect();
    let mut ordered = Vec::new();
    while let Some(name) = queue.pop_front() {
        ordered.push(name.to_owned());
        for child in outgoing.get(name).into_iter().flatten() {
            if let Some(degree) = indegree.get_mut(child) {
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    queue.push_back(child);
                }
            }
        }
    }
    if ordered.len() != tables.len() {
        return Err(UsersError::validation(
            "catalog tables have a foreign-key cycle",
        ));
    }
    Ok(ordered)
}

async fn copy_table(
    db: &impl ConnectionTrait,
    table: &str,
    template_company: i64,
    new_company: i64,
    fks: &HashMap<String, Vec<(String, String)>>,
    ids: &mut HashMap<(String, i64), i64>,
) -> Result<(), UsersError> {
    let owners = owners_table(db, table).await?;
    let surrogate = surrogate_id(db, table).await?;
    let rows = if surrogate.is_some() {
        db.query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "SELECT to_jsonb(t) AS row FROM {table} t
                     JOIN {owners} o ON o.record_id = t.id
                     WHERE o.company_id = $1
                     ORDER BY t.id"
            ),
            [template_company.into()],
        ))
        .await?
    } else {
        let pk = owner_columns(db, &owners).await?;
        let pred = pk
            .iter()
            .map(|col| format!("o.{col} = t.{col}"))
            .collect::<Vec<_>>()
            .join(" AND ");
        db.query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "SELECT to_jsonb(t) AS row FROM {table} t
                     JOIN {owners} o ON {pred}
                     WHERE o.company_id = $1"
            ),
            [template_company.into()],
        ))
        .await?
    };
    let mut pending = Vec::new();
    for row in rows {
        let value: serde_json::Value = row.try_get("", "row")?;
        pending.push(value);
    }
    if let Some(key) = dedupe_key_column(table) {
        pending = dedupe_rows_by_key(pending, key);
    }
    let self_fks: Vec<String> = fks
        .get(table)
        .into_iter()
        .flatten()
        .filter(|(_, dst)| dst == table)
        .map(|(col, _)| col.clone())
        .collect();
    let source_ids: HashSet<i64> = pending
        .iter()
        .filter_map(|row| json_i64(row.get("id")))
        .collect();
    let required = not_null_columns(db, table).await?;
    if surrogate == Some(true) {
        // Explicit inserts can leave a sequence sitting on an id that already exists.
        sync_id_sequence(db, table).await?;
    }
    let mut guard = pending.len().saturating_add(1);
    while !pending.is_empty() {
        if guard == 0 {
            return Err(UsersError::validation(format!(
                "could not order rows in {table}"
            )));
        }
        guard -= 1;
        let mut next = Vec::new();
        let mut progressed = false;
        for row in pending {
            if !self_fks_ready(&row, &self_fks, &source_ids, table, ids) {
                next.push(row);
                continue;
            }
            // A required foreign key that does not belong to the template
            // company cannot be remapped. Nulling it fails the insert and
            // would discard the chart copied earlier in this transaction.
            if required_fk_unmapped(&row, table, fks, &required, ids) {
                progressed = true;
                continue;
            }
            if let Some(key) = dedupe_key_column(table)
                && destination_owns_key(db, table, &owners, new_company, key, &row).await?
            {
                // Still record id remaps so FKs from later rows resolve.
                if let Some(old_id) = json_i64(row.get("id"))
                    && let Some(existing) =
                        existing_owned_id(db, table, &owners, new_company, key, &row).await?
                {
                    ids.insert((table.to_owned(), old_id), existing);
                }
                progressed = true;
                continue;
            }
            insert_row(db, table, &owners, new_company, surrogate, row, fks, ids).await?;
            progressed = true;
        }
        if !progressed && !next.is_empty() {
            return Err(UsersError::validation(format!(
                "could not order rows in {table}"
            )));
        }
        pending = next;
    }
    Ok(())
}

/// Keep the lowest-id row for each natural key value (string or number).
fn dedupe_rows_by_key(rows: Vec<Value>, key: &str) -> Vec<Value> {
    let mut seen: HashMap<String, Value> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for row in rows {
        let Some(key_value) = row.get(key) else {
            continue;
        };
        let fingerprint = match key_value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            other => other.to_string(),
        };
        let id = json_i64(row.get("id")).unwrap_or(i64::MAX);
        match seen.get(&fingerprint) {
            Some(existing) => {
                let existing_id = json_i64(existing.get("id")).unwrap_or(i64::MAX);
                if id < existing_id {
                    seen.insert(fingerprint, row);
                }
            }
            None => {
                order.push(fingerprint.clone());
                seen.insert(fingerprint, row);
            }
        }
    }
    order.into_iter().filter_map(|k| seen.remove(&k)).collect()
}

async fn destination_owns_key(
    db: &impl ConnectionTrait,
    table: &str,
    owners: &str,
    company_id: i64,
    key: &str,
    row: &Value,
) -> Result<bool, UsersError> {
    Ok(existing_owned_id(db, table, owners, company_id, key, row)
        .await?
        .is_some())
}

async fn existing_owned_id(
    db: &impl ConnectionTrait,
    table: &str,
    owners: &str,
    company_id: i64,
    key: &str,
    row: &Value,
) -> Result<Option<i64>, UsersError> {
    if !is_safe_ident(table) || !is_safe_ident(owners) || !is_safe_ident(key) {
        return Ok(None);
    }
    let Some(key_value) = row.get(key) else {
        return Ok(None);
    };
    let (sql, params) = match key_value {
        Value::String(s) => (
            format!(
                "SELECT t.id AS id FROM {table} t
                 JOIN {owners} o ON o.record_id = t.id
                 WHERE o.company_id = $1 AND t.{key} = $2
                 ORDER BY t.id LIMIT 1"
            ),
            vec![company_id.into(), s.clone().into()],
        ),
        Value::Number(n) => {
            let Some(num) = n.as_i64() else {
                return Ok(None);
            };
            (
                format!(
                    "SELECT t.id AS id FROM {table} t
                     JOIN {owners} o ON o.record_id = t.id
                     WHERE o.company_id = $1 AND t.{key} = $2
                     ORDER BY t.id LIMIT 1"
                ),
                vec![company_id.into(), num.into()],
            )
        }
        _ => return Ok(None),
    };
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

/// Drop same-key duplicate owned rows on a company, keeping the lowest id.
///
/// Used to clean a polluted template company before cloning so new companies
/// do not inherit hundreds of INR / General / GST duplicates.
pub async fn collapse_duplicate_catalog(
    db: &DatabaseConnection,
    company_id: i64,
) -> Result<(), UsersError> {
    for (table, key) in [
        ("currencies", "code"),
        ("accounts", "code"),
        ("journals", "name"),
        ("taxes", "name"),
    ] {
        if !table_exists(db, table).await? {
            continue;
        }
        let owners = owners_table(db, table).await?;
        let sql = format!(
            "WITH ranked AS (
                 SELECT t.id AS id,
                        ROW_NUMBER() OVER (
                            PARTITION BY t.{key} ORDER BY t.id
                        ) AS rn
                 FROM {table} t
                 JOIN {owners} o ON o.record_id = t.id
                 WHERE o.company_id = $1
             ),
             doomed AS (SELECT id FROM ranked WHERE rn > 1)
             DELETE FROM {owners} o
             USING doomed d
             WHERE o.record_id = d.id AND o.company_id = $1"
        );
        db.execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [company_id.into()],
        ))
        .await?;
    }
    Ok(())
}

async fn table_exists(db: &impl ConnectionTrait, table: &str) -> Result<bool, UsersError> {
    if !is_safe_ident(table) {
        return Ok(false);
    }
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT to_regclass(format('public.%I', $1)) IS NOT NULL AS present",
            [table.into()],
        ))
        .await?;
    Ok(row
        .map(|row| row.try_get::<bool>("", "present"))
        .transpose()?
        .unwrap_or(false))
}

fn self_fks_ready(
    row: &Value,
    self_fks: &[String],
    source_ids: &HashSet<i64>,
    table: &str,
    ids: &HashMap<(String, i64), i64>,
) -> bool {
    for col in self_fks {
        let Some(old) = json_i64(row.get(col)) else {
            continue;
        };
        if source_ids.contains(&old) && !ids.contains_key(&(table.to_owned(), old)) {
            return false;
        }
    }
    true
}

async fn insert_row(
    db: &impl ConnectionTrait,
    table: &str,
    owners: &str,
    new_company: i64,
    surrogate: Option<bool>,
    mut row: Value,
    fks: &HashMap<String, Vec<(String, String)>>,
    ids: &mut HashMap<(String, i64), i64>,
) -> Result<(), UsersError> {
    let old_id = json_i64(row.get("id"));
    let object = row
        .as_object_mut()
        .ok_or_else(|| UsersError::validation(format!("{table} row was not an object")))?;
    if let Some(edges) = fks.get(table) {
        for (col, dst) in edges {
            remap_fk(object, col, dst, ids)?;
        }
    }
    if let Some(generated) = surrogate {
        if generated {
            object.remove("id");
        } else {
            let next = next_manual_id(db, table).await?;
            object.insert("id".to_owned(), Value::from(next));
        }
        let columns = insert_columns(db, table, generated).await?;
        let collist = columns.join(", ");
        let payload = if generated {
            "$1::jsonb - 'id'"
        } else {
            "$1::jsonb"
        };
        let sql = format!(
            "INSERT INTO {table} ({collist})
             SELECT {collist} FROM jsonb_populate_record(NULL::{table}, {payload})
             RETURNING id"
        );
        let inserted = db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                [Value::Object(object.clone()).into()],
            ))
            .await
            .map_err(|err| {
                UsersError::validation(format!("insert into {table} (columns {collist}): {err}"))
            })?
            .ok_or_else(|| UsersError::validation(format!("insert into {table} returned no id")))?;
        let new_id: i64 = inserted.try_get("", "id")?;
        if let Some(old_id) = old_id {
            ids.insert((table.to_owned(), old_id), new_id);
        }
        db.execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "INSERT INTO {owners} (record_id, company_id) VALUES ($1, $2)
                     ON CONFLICT DO NOTHING"
            ),
            [new_id.into(), new_company.into()],
        ))
        .await?;
    } else {
        let columns = insert_columns(db, table, false).await?;
        let collist = columns.join(", ");
        let sql = format!(
            "INSERT INTO {table} ({collist})
             SELECT {collist} FROM jsonb_populate_record(NULL::{table}, $1::jsonb)"
        );
        db.execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [Value::Object(object.clone()).into()],
        ))
        .await
        .map_err(|err| {
            UsersError::validation(format!(
                "insert into {table} (kept primary key, columns {collist}): {err}"
            ))
        })?;
        let pk = owner_columns(db, owners).await?;
        let values = pk
            .iter()
            .map(|col| json_i64(object.get(col)).unwrap_or(0))
            .collect::<Vec<_>>();
        let mut placeholders = Vec::new();
        let mut params: Vec<sea_orm::Value> = Vec::new();
        for (index, value) in values.iter().enumerate() {
            placeholders.push(format!("${}", index + 1));
            params.push((*value).into());
        }
        placeholders.push(format!("${}", values.len() + 1));
        params.push(new_company.into());
        let collist = pk.join(", ");
        db.execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!(
                "INSERT INTO {owners} ({collist}, company_id) VALUES ({})
                     ON CONFLICT DO NOTHING",
                placeholders.join(", ")
            ),
            params,
        ))
        .await?;
    }
    Ok(())
}

async fn not_null_columns(
    db: &impl ConnectionTrait,
    table: &str,
) -> Result<HashSet<String>, UsersError> {
    if !is_safe_ident(table) {
        return Ok(HashSet::new());
    }
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT a.attname AS name
             FROM pg_attribute a
             WHERE a.attrelid = to_regclass(format('public.%I', $1))
               AND a.attnum > 0
               AND NOT a.attisdropped
               AND a.attnotnull",
            [table.into()],
        ))
        .await?;
    let mut names = HashSet::new();
    for row in rows {
        names.insert(row.try_get("", "name")?);
    }
    Ok(names)
}

/// True when a NOT NULL catalog foreign key has no cloned target yet.
fn required_fk_unmapped(
    row: &Value,
    table: &str,
    fks: &HashMap<String, Vec<(String, String)>>,
    required: &HashSet<String>,
    ids: &HashMap<(String, i64), i64>,
) -> bool {
    let Some(edges) = fks.get(table) else {
        return false;
    };
    edges.iter().any(|(col, dst)| {
        required.contains(col)
            && json_i64(row.get(col)).is_some_and(|old| !ids.contains_key(&(dst.clone(), old)))
    })
}

fn remap_fk(
    object: &mut Map<String, Value>,
    col: &str,
    dst: &str,
    ids: &HashMap<(String, i64), i64>,
) -> Result<(), UsersError> {
    let Some(old) = json_i64(object.get(col)) else {
        return Ok(());
    };
    match ids.get(&(dst.to_owned(), old)) {
        Some(new_id) => {
            object.insert(col.to_owned(), Value::from(*new_id));
            Ok(())
        }
        None => {
            object.insert(col.to_owned(), Value::Null);
            Ok(())
        }
    }
}

fn json_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}

async fn sync_id_sequence(db: &impl ConnectionTrait, table: &str) -> Result<(), UsersError> {
    if !is_safe_ident(table) {
        return Err(UsersError::validation(format!("unsafe table {table}")));
    }
    let sql = format!(
        "SELECT CASE
            WHEN pg_get_serial_sequence('public.{table}', 'id') IS NULL THEN 0::bigint
            ELSE setval(
                pg_get_serial_sequence('public.{table}', 'id'),
                COALESCE((SELECT MAX(id) FROM {table}), 1),
                EXISTS (SELECT 1 FROM {table})
            )
         END"
    );
    db.execute_raw(Statement::from_string(DatabaseBackend::Postgres, sql))
        .await?;
    Ok(())
}

async fn next_manual_id(db: &impl ConnectionTrait, table: &str) -> Result<i64, UsersError> {
    if !is_safe_ident(table) {
        return Err(UsersError::validation(format!("unsafe table {table}")));
    }
    let row = db
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            format!("SELECT COALESCE(MAX(id), 0) + 1 AS id FROM {table}"),
        ))
        .await?
        .ok_or_else(|| UsersError::validation(format!("could not allocate an id in {table}")))?;
    row.try_get("", "id").map_err(UsersError::from)
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

async fn owner_columns(db: &impl ConnectionTrait, owners: &str) -> Result<Vec<String>, UsersError> {
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT attname AS name
             FROM pg_attribute
             WHERE attrelid = to_regclass($1)
               AND attnum > 0
               AND NOT attisdropped
               AND attname <> 'company_id'
             ORDER BY attnum",
            [format!("public.{owners}").into()],
        ))
        .await?;
    let mut names = Vec::new();
    for row in rows {
        let name: String = row.try_get("", "name")?;
        if !is_safe_ident(&name) {
            return Err(UsersError::validation(format!("unsafe column {name}")));
        }
        names.push(name);
    }
    Ok(names)
}

async fn insert_columns(
    db: &impl ConnectionTrait,
    table: &str,
    skip_id: bool,
) -> Result<Vec<String>, UsersError> {
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT attname AS name
             FROM pg_attribute
             WHERE attrelid = to_regclass($1)
               AND attnum > 0
               AND NOT attisdropped
             ORDER BY attnum",
            [format!("public.{table}").into()],
        ))
        .await?;
    let mut names = Vec::new();
    for row in rows {
        let name: String = row.try_get("", "name")?;
        if skip_id && name == "id" {
            continue;
        }
        if !is_safe_ident(&name) {
            return Err(UsersError::validation(format!("unsafe column {name}")));
        }
        names.push(name);
    }
    Ok(names)
}

fn is_safe_ident(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
