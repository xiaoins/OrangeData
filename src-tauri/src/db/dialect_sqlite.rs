use super::dialect::{qualify, check_ident, Ctx, Dialect, Quote, Sql};

pub struct SqliteDialect;

impl SqliteDialect {
    fn schema(ctx: &Ctx) -> String {
        if ctx.database.is_empty() {
            "main".to_string()
        } else {
            ctx.database.clone()
        }
    }
    /// pragma_* table valued functions take (table_name, schema_name).
    fn tvf_args(ctx: &Ctx) -> Vec<String> {
        vec![ctx.table.clone(), Self::schema(ctx)]
    }
    fn qualified(ctx: &Ctx) -> String {
        let schema = Self::schema(ctx);
        let table = check_ident(&ctx.table, "表名").unwrap_or_else(|_| ctx.table.clone());
        qualify(Quote::Double, &[&schema, &table])
    }
}

impl Dialect for SqliteDialect {
    fn name(&self) -> &'static str {
        "sqlite"
    }
    fn quote(&self) -> Quote {
        Quote::Double
    }
    fn has_schemas(&self) -> bool {
        false
    }
    fn exact_counts(&self) -> bool {
        true
    }
    fn paging(&self, limit: u64, offset: u64) -> String {
        format!("LIMIT {limit} OFFSET {offset}")
    }
    fn table_name(&self, ctx: &Ctx) -> String {
        Self::qualified(ctx)
    }
    fn cast_text(&self, col: &str) -> String {
        format!("CAST({col} AS TEXT)")
    }
    fn compare(&self, col: &str, _ty: &str) -> String {
        format!("{col} = ?")
    }
    fn assign(&self, col: &str, _ty: &str) -> String {
        format!("{col} = ?")
    }
    fn id_type(&self) -> &'static str {
        "INTEGER"
    }
    fn auto_increment(&self) -> &'static str {
        "PRIMARY KEY AUTOINCREMENT"
    }
    fn ddl_key(&self) -> Option<&'static str> {
        Some("ddl")
    }
    fn version_sql(&self) -> Sql {
        Sql::new("SELECT sqlite_version() AS version")
    }
    fn current_db_sql(&self) -> Option<Sql> {
        None
    }
    fn count_sql(&self, ctx: &Ctx) -> Sql {
        Sql::new(format!("SELECT COUNT(*) AS row_count FROM {}", Self::qualified(ctx)))
    }
    fn databases_sql(&self, _ctx: &Ctx) -> Sql {
        Sql::new("SELECT name AS name, file AS comment FROM pragma_database_list() ORDER BY seq")
    }
    fn schemas_sql(&self, ctx: &Ctx) -> Sql {
        self.databases_sql(ctx)
    }
    fn tables_sql(&self, ctx: &Ctx) -> Sql {
        let s = qualify(Quote::Double, &[&Self::schema(ctx)]);
        Sql::new(format!(
            "SELECT m.name AS name, CASE WHEN m.type = 'view' THEN 'view' ELSE 'table' END AS kind, \
             NULL AS row_estimate, NULL AS size_bytes, NULL AS comment \
             FROM {s}.sqlite_master m \
             WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' ORDER BY m.name"
        ))
    }
    fn columns_sql(&self, ctx: &Ctx) -> Sql {
        Sql::with(
            "SELECT cid AS ord, name AS name, type AS type, NULL AS size, (1 - \"notnull\") AS nullable, \
             dflt_value AS default_value, '' AS comment, \
             CASE WHEN pk > 0 THEN 'primary key' ELSE '' END AS extra \
             FROM pragma_table_info(?, ?) ORDER BY cid"
                .to_string(),
            Self::tvf_args(ctx),
        )
    }
    fn pk_sql(&self, ctx: &Ctx) -> Sql {
        Sql::with(
            "SELECT GROUP_CONCAT(name, ', ') AS pk_columns FROM pragma_table_info(?, ?) WHERE pk > 0"
                .to_string(),
            Self::tvf_args(ctx),
        )
    }
    fn fk_sql(&self, ctx: &Ctx) -> Sql {
        Sql::with(
            "SELECT 'fk_' || id AS fk_name, seq AS seq, \"from\" AS column_name, \"table\" AS ref_table, \
             \"to\" AS ref_column, on_update AS on_update, on_delete AS on_delete \
             FROM pragma_foreign_key_list(?, ?) ORDER BY id, seq"
                .to_string(),
            Self::tvf_args(ctx),
        )
    }
    fn index_sql(&self, ctx: &Ctx) -> Sql {
        Sql::with(
            "SELECT il.name AS index_name, il.\"unique\" AS is_unique, \
             CASE WHEN il.origin = 'pk' THEN 1 ELSE 0 END AS is_primary, il.\"type\" AS index_kind, \
             (SELECT GROUP_CONCAT(i.name, ', ') FROM pragma_index_info(il.name) i) AS column_list \
             FROM pragma_index_list(?, ?) il ORDER BY is_primary DESC, il.name"
                .to_string(),
            Self::tvf_args(ctx),
        )
    }
    fn ddl_sql(&self, ctx: &Ctx) -> Option<Sql> {
        let s = qualify(Quote::Double, &[&Self::schema(ctx)]);
        Some(Sql::with(
            format!("SELECT sql AS ddl FROM {s}.sqlite_master WHERE name = ? AND sql IS NOT NULL"),
            vec![ctx.table.clone()],
        ))
    }
    fn set_schema_sql(&self, _schema: &str) -> Option<String> {
        None
    }
}
