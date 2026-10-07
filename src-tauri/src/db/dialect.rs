use crate::model::Res;

/// Identifier quoting style per engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quote {
    Double,
    Backtick,
}

/// A built statement plus the values to bind. All bound values travel as text;
/// engines that need a concrete type cast the placeholder (see `compare_expr`).
#[derive(Clone, Debug, Default)]
pub struct Sql {
    pub text: String,
    pub args: Vec<String>,
}

impl Sql {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), args: vec![] }
    }
    pub fn with(text: impl Into<String>, args: Vec<String>) -> Self {
        Self { text: text.into(), args }
    }
}

/// Where a metadata query should look.
#[derive(Clone, Debug, Default)]
pub struct Ctx {
    pub database: String,
    pub schema: String,
    pub table: String,
}

pub fn quote_ident(style: Quote, name: &str) -> String {
    match style {
        Quote::Double => format!("\"{}\"", name.replace('"', "\"\"")),
        Quote::Backtick => format!("`{}`", name.replace('`', "``")),
    }
}

pub fn qualify(style: Quote, parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| quote_ident(style, p))
        .collect::<Vec<_>>()
        .join(".")
}

/// Identifiers are interpolated into SQL (they can never be bound as parameters),
/// so they must pass a whitelist first.
pub fn check_ident(name: &str, what: &str) -> Res<String> {
    let s = name.trim();
    if s.is_empty() {
        return Err(format!("{what}不能为空"));
    }
    if s.chars().count() > 256 {
        return Err(format!("{what}过长"));
    }
    if s.contains('\0') || s.contains("--") || s.contains(';') || s.contains("/*") {
        return Err(format!("{what}含非法字符：{s}"));
    }
    Ok(s.to_string())
}

/// Consume a quoted run (`...`, `"..."`, `` `...` ``) including doubled quotes.
/// Returns the byte length including both delimiters.
fn scan_quoted(s: &str, q: u8) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 1usize;
    while i < b.len() {
        if b[i] == q {
            if i + 1 < b.len() && b[i + 1] == q {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn line_comment_len(s: &str) -> usize {
    match s.find('\n') {
        Some(off) => off + 1,
        None => s.len(),
    }
}

fn block_comment_len(s: &str) -> usize {
    match s.find("*/") {
        Some(off) => off + 2,
        None => s.len(),
    }
}

/// Rewrite `?` placeholders into PostgreSQL's `$n` form, skipping anything inside
/// string literals, quoted identifiers or comments.
pub fn to_dollar(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut rest = sql;
    let mut n = 0usize;
    loop {
        let b = rest.as_bytes();
        let mut idx = 0usize;
        while idx < b.len() && !matches!(b[idx], b'?' | b'\'' | b'"' | b'`' | b'-' | b'/') {
            idx += 1;
        }
        out.push_str(&rest[..idx]);
        if idx >= b.len() {
            break;
        }
        let c = b[idx];
        let tail = &rest[idx..];
        if c == b'?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
            rest = &rest[idx + 1..];
        } else if matches!(c, b'\'' | b'"' | b'`') {
            let end = scan_quoted(tail, c).unwrap_or(tail.len());
            out.push_str(&tail[..end]);
            rest = &tail[end..];
        } else if c == b'-' && tail.starts_with("--") {
            let end = line_comment_len(tail);
            out.push_str(&tail[..end]);
            rest = &tail[end..];
        } else if c == b'/' && tail.starts_with("/*") {
            let end = block_comment_len(tail);
            out.push_str(&tail[..end]);
            rest = &tail[end..];
        } else {
            out.push(c as char);
            rest = &rest[idx + 1..];
        }
    }
    out
}

/// Split a script into single statements on `;`, honouring quotes and comments.
pub fn split_statements(sql: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut rest = sql;
    loop {
        let b = rest.as_bytes();
        let mut idx = 0usize;
        while idx < b.len() && !matches!(b[idx], b';' | b'\'' | b'"' | b'`' | b'-' | b'#' | b'/') {
            idx += 1;
        }
        cur.push_str(&rest[..idx]);
        if idx >= b.len() {
            break;
        }
        let c = b[idx];
        let tail = &rest[idx..];
        if c == b';' {
            parts.push(std::mem::take(&mut cur));
            rest = &rest[idx + 1..];
        } else if matches!(c, b'\'' | b'"' | b'`') {
            let end = scan_quoted(tail, c).unwrap_or(tail.len());
            cur.push_str(&tail[..end]);
            rest = &tail[end..];
        } else if (c == b'-' && tail.starts_with("--")) || c == b'#' {
            let end = line_comment_len(tail);
            cur.push_str(&tail[..end]);
            rest = &tail[end..];
        } else if c == b'/' && tail.starts_with("/*") {
            let end = block_comment_len(tail);
            cur.push_str(&tail[..end]);
            rest = &tail[end..];
        } else {
            cur.push(c as char);
            rest = &rest[idx + 1..];
        }
    }
    parts.push(cur);
    parts
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn is_query(sql: &str) -> bool {
    let s = sql.trim_start().to_lowercase();
    ["select", "with", "show", "describe", "desc", "explain", "values", "pragma", "table"]
        .iter()
        .any(|p| s.starts_with(p))
}

/// Sanitise a catalog-reported column type before interpolating it into a CAST.
pub fn safe_cast_type(t: &str) -> String {
    let s = t.trim();
    let ok = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | ',' | '.' | '(' | ')' | '[' | ']' | '-' | '\t'))
        && !s.contains("--")
        && !s.contains(';');
    if ok && s.len() < 128 {
        s.to_string()
    } else {
        "text".to_string()
    }
}

/* ------------------------------- dialect contract ------------------------------- */

/// Everything engine-specific is a pure string builder here, so the whole
/// metadata / browsing / editing layer can be written once in `api.rs`.
pub trait Dialect: Send + Sync {
    fn name(&self) -> &'static str;
    fn quote(&self) -> Quote;
    /// True when tables live under a schema inside a database (PostgreSQL).
    fn has_schemas(&self) -> bool;
    /// No catalog statistics (SQLite): ask the table for an exact COUNT instead.
    fn exact_counts(&self) -> bool {
        false
    }
    fn paging(&self, limit: u64, offset: u64) -> String;
    /// Fully qualified, quoted table reference used in generated DML/DDL.
    fn table_name(&self, ctx: &Ctx) -> String;
    fn cast_text(&self, col: &str) -> String;
    /// Predicate against one bound text value, e.g. PG needs `= CAST(? AS int)`.
    fn compare(&self, col: &str, ty: &str) -> String;
    /// Same predicate for another operator; `op` must come from a whitelist.
    fn cmp(&self, col: &str, ty: &str, op: &str) -> String {
        if op == "=" {
            return self.compare(col, ty);
        }
        self.compare(col, ty).replace(" = ", &format!(" {op} "))
    }
    /// Assignment of one bound text value.
    fn assign(&self, col: &str, ty: &str) -> String;
    /// Placeholder for a bound value in a VALUES list.
    fn value_placeholder(&self, _ty: &str) -> String {
        "?".to_string()
    }
    fn id_type(&self) -> &'static str;
    fn auto_increment(&self) -> &'static str;
    /// Catalog column holding DDL text, when the engine can return it.
    fn ddl_key(&self) -> Option<&'static str> {
        None
    }
    fn version_sql(&self) -> Sql;
    fn current_db_sql(&self) -> Option<Sql>;
    fn count_sql(&self, ctx: &Ctx) -> Sql;
    fn databases_sql(&self, ctx: &Ctx) -> Sql;
    fn schemas_sql(&self, ctx: &Ctx) -> Sql;
    fn tables_sql(&self, ctx: &Ctx) -> Sql;
    fn columns_sql(&self, ctx: &Ctx) -> Sql;
    fn pk_sql(&self, ctx: &Ctx) -> Sql;
    fn fk_sql(&self, ctx: &Ctx) -> Sql;
    fn index_sql(&self, ctx: &Ctx) -> Sql;
    fn ddl_sql(&self, ctx: &Ctx) -> Option<Sql>;
    fn set_schema_sql(&self, schema: &str) -> Option<String>;
}
