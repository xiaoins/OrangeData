use super::dialect::{check_ident, qualify, Ctx, Dialect, Quote, Sql};

pub struct MySqlDialect;

/// The database a metadata lookup should target: the selected one, or the
/// session's current one when the tree has not picked a catalog yet.
fn db(ctx: &Ctx) -> String {
    ctx.database.clone()
}

/// `TABLE_SCHEMA` predicate plus its bound value; `DATABASE()` keeps the server's
/// own selection when no catalog was chosen.
fn schema_pred(ctx: &Ctx) -> (String, Vec<String>) {
    if ctx.database.trim().is_empty() {
        ("TABLE_SCHEMA = DATABASE()".to_string(), vec![])
    } else {
        ("TABLE_SCHEMA = ?".to_string(), vec![ctx.database.clone()])
    }
}

fn qualified(ctx: &Ctx) -> String {
    let table = check_ident(&ctx.table, "表名").unwrap_or_else(|_| ctx.table.clone());
    if ctx.database.trim().is_empty() {
        return qualify(Quote::Backtick, &[&table]);
    }
    let d = check_ident(&db(ctx), "数据库名").unwrap_or_else(|_| db(ctx));
    qualify(Quote::Backtick, &[&d, &table])
}

impl Dialect for MySqlDialect {
    fn name(&self) -> &'static str {
        "mysql"
    }
    fn quote(&self) -> Quote {
        Quote::Backtick
    }
    fn has_schemas(&self) -> bool {
        false
    }
    fn paging(&self, limit: u64, offset: u64) -> String {
        format!("LIMIT {limit} OFFSET {offset}")
    }
    fn table_name(&self, ctx: &Ctx) -> String {
        qualified(ctx)
    }
    fn cast_text(&self, col: &str) -> String {
        format!("CAST({col} AS CHAR)")
    }
    fn compare(&self, col: &str, _ty: &str) -> String {
        format!("{col} = ?")
    }
    fn assign(&self, col: &str, _ty: &str) -> String {
        format!("{col} = ?")
    }
    fn id_type(&self) -> &'static str {
        "BIGINT"
    }
    fn auto_increment(&self) -> &'static str {
        "AUTO_INCREMENT"
    }
    fn ddl_key(&self) -> Option<&'static str> {
        Some("Create Table")
    }
    fn version_sql(&self) -> Sql {
        Sql::new("SELECT VERSION() AS version")
    }
    fn current_db_sql(&self) -> Option<Sql> {
        Some(Sql::new("SELECT DATABASE() AS name"))
    }
    fn count_sql(&self, ctx: &Ctx) -> Sql {
        Sql::new(format!("SELECT COUNT(*) AS row_count FROM {}", qualified(ctx)))
    }
    fn databases_sql(&self, _ctx: &Ctx) -> Sql {
        Sql::new(
            "SELECT SCHEMA_NAME AS name FROM information_schema.SCHEMATA \
             WHERE SCHEMA_NAME NOT IN ('information_schema','performance_schema','sys') ORDER BY name",
        )
    }
    fn schemas_sql(&self, _ctx: &Ctx) -> Sql {
        Sql::new("SELECT DATABASE() AS name")
    }
    fn tables_sql(&self, ctx: &Ctx) -> Sql {
        let (pred, args) = schema_pred(ctx);
        Sql::with(
            format!(
                "SELECT TABLE_NAME AS name, CASE WHEN TABLE_TYPE = 'VIEW' THEN 'view' ELSE 'table' END AS kind, \
                 TABLE_ROWS AS row_estimate, (DATA_LENGTH + INDEX_LENGTH) AS size_bytes, TABLE_COMMENT AS comment \
                 FROM information_schema.TABLES WHERE {pred} ORDER BY TABLE_NAME"
            ),
            args,
        )
    }
    fn columns_sql(&self, ctx: &Ctx) -> Sql {
        let (pred, mut args) = schema_pred(ctx);
        args.push(ctx.table.clone());
        Sql::with(
            format!(
                "SELECT ORDINAL_POSITION AS ord, COLUMN_NAME AS name, COLUMN_TYPE AS type, \
                 CHARACTER_MAXIMUM_LENGTH AS size, CASE WHEN IS_NULLABLE = 'NO' THEN 0 ELSE 1 END AS nullable, \
                 COLUMN_DEFAULT AS default_value, COLUMN_COMMENT AS comment, EXTRA AS extra \
                 FROM information_schema.COLUMNS WHERE {pred} AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION"
            ),
            args,
        )
    }
    fn pk_sql(&self, ctx: &Ctx) -> Sql {
        let (pred, mut args) = schema_pred(ctx);
        args.push(ctx.table.clone());
        Sql::with(
            format!(
                "SELECT GROUP_CONCAT(COLUMN_NAME ORDER BY ORDINAL_POSITION) AS pk_columns \
                 FROM information_schema.KEY_COLUMN_USAGE WHERE {pred} AND TABLE_NAME = ? AND CONSTRAINT_NAME = 'PRIMARY'"
            ),
            args,
        )
    }
    fn fk_sql(&self, ctx: &Ctx) -> Sql {
        let (pred, mut args) = schema_pred(ctx);
        args.push(ctx.table.clone());
        Sql::with(
            format!(
                "SELECT k.CONSTRAINT_NAME AS fk_name, k.COLUMN_NAME AS column_name, \
                 k.REFERENCED_TABLE_NAME AS ref_table, k.REFERENCED_COLUMN_NAME AS ref_column, \
                 r.DELETE_RULE AS on_delete, r.UPDATE_RULE AS on_update \
                 FROM information_schema.KEY_COLUMN_USAGE k \
                 JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
                   ON r.CONSTRAINT_SCHEMA = k.TABLE_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME \
                 WHERE {pred} AND k.TABLE_NAME = ? AND k.REFERENCED_TABLE_NAME IS NOT NULL \
                 ORDER BY fk_name, k.ORDINAL_POSITION"
            ),
            args,
        )
    }
    fn index_sql(&self, ctx: &Ctx) -> Sql {
        let (pred, mut args) = schema_pred(ctx);
        args.push(ctx.table.clone());
        Sql::with(
            format!(
                "SELECT INDEX_NAME AS index_name, CASE WHEN NON_UNIQUE = 0 THEN 1 ELSE 0 END AS is_unique, \
                 CASE WHEN INDEX_NAME = 'PRIMARY' THEN 1 ELSE 0 END AS is_primary, INDEX_TYPE AS index_kind, \
                 GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX) AS column_list \
                 FROM information_schema.STATISTICS WHERE {pred} AND TABLE_NAME = ? \
                 GROUP BY INDEX_NAME, NON_UNIQUE, INDEX_TYPE ORDER BY is_primary DESC, INDEX_NAME"
            ),
            args,
        )
    }
    fn ddl_sql(&self, ctx: &Ctx) -> Option<Sql> {
        Some(Sql::new(format!("SHOW CREATE TABLE {}", qualified(ctx))))
    }
    fn set_schema_sql(&self, _schema: &str) -> Option<String> {
        None
    }
}
