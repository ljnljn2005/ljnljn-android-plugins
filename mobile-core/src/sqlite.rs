//! Read-only SQLite access over a private, disposable copy.
//!
//! Mobile databases are often in WAL mode. The artifact plugins therefore copy
//! the main database and its `-wal` / `-shm` / `-journal` sidecars into one
//! temporary directory before opening it. The evidence view is never opened
//! writable and the copy is removed when the analysis ends.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lovelycarve_plugin::{FileRef, Value, View};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

/// Upper bound for one database copy (the host materialize limit is 4 GiB).
pub const MAX_DB_BYTES: u64 = 1024 * 1024 * 1024;
/// Rows returned by one query before a `truncated` flag is set.
pub const MAX_ROWS: usize = 100_000;
/// Approximate bytes of cell data buffered for one query.
pub const MAX_RESULT_BYTES: usize = 128 * 1024 * 1024;
/// Longest single text cell kept in a record.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";
static COPY_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub enum DbValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(usize),
}

impl DbValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Real(value) if value.is_finite() => Some(value.round() as i64),
            Self::Text(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<DbValue>>,
    pub truncated: bool,
}

impl QueryResult {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(name))
    }

    pub fn value<'a>(&self, row: &'a [DbValue], name: &str) -> Option<&'a DbValue> {
        self.column(name).and_then(|index| row.get(index))
    }
}

struct DbCopy {
    dir: PathBuf,
}

impl Drop for DbCopy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A materialized copy and its read-only connection.
pub struct Db {
    // `conn` is declared before `copy` so it is dropped before the directory.
    conn: Connection,
    _copy: DbCopy,
}

impl Db {
    pub fn open(file: &FileRef, view: Option<&View>) -> Result<Self, String> {
        let size = file.size();
        if size == 0 {
            return Err("文件为空（0 字节），不是 SQLite 数据库".to_owned());
        }
        if size > MAX_DB_BYTES {
            return Err(format!(
                "数据库大小 {size} 字节超过本插件上限 {MAX_DB_BYTES} 字节"
            ));
        }
        let header = file
            .read_at(0, SQLITE_MAGIC.len())
            .map_err(|error| format!("读取文件头失败：{}", error.message))?;
        if header.len() < SQLITE_MAGIC.len() || &header[..SQLITE_MAGIC.len()] != SQLITE_MAGIC {
            return Err("文件头不是 SQLite 3（可能是加密库、WAL 边车或非数据库）".to_owned());
        }

        let id = COPY_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "ljnljn-mobile-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).map_err(|error| format!("创建临时目录失败：{error}"))?;
        let copy = DbCopy { dir: dir.clone() };
        let path = dir.join("main.db");

        let materialized = file
            .materialize()
            .map_err(|error| format!("物化数据库失败：{}", error.message))?;
        fs::copy(materialized.path(), &path)
            .map_err(|error| format!("复制数据库到临时目录失败：{error}"))?;

        if let Some(view) = view {
            let source = file
                .view_path()
                .map(str::to_owned)
                .unwrap_or_else(|| file.name().to_owned());
            for suffix in ["-wal", "-shm", "-journal"] {
                copy_sidecar(view, &source, suffix, &dir);
            }
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = match Connection::open_with_flags(&path, flags) {
            Ok(conn) => conn,
            Err(_) => Connection::open(&path)
                .map_err(|error| format!("SQLite 打开失败（只读与可写副本均失败）：{error}"))?,
        };
        conn.execute_batch(
            "PRAGMA query_only = ON; PRAGMA trusted_schema = OFF; \
             PRAGMA temp_store = FILE; PRAGMA busy_timeout = 2000;",
        )
        .map_err(|error| format!("设置只读 PRAGMA 失败：{error}"))?;
        Ok(Self {
            conn,
            _copy: copy,
        })
    }

    pub fn tables(&self) -> Result<HashSet<String>, String> {
        let mut statement = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view')")
            .map_err(|error| format!("读取库对象失败：{error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| format!("读取库对象失败：{error}"))?;
        let mut names = HashSet::new();
        for row in rows {
            let name = row.map_err(|error| format!("读取库对象失败：{error}"))?;
            names.insert(name.to_ascii_lowercase());
        }
        Ok(names)
    }

    pub fn query(&self, sql: &str) -> Result<QueryResult, String> {
        let mut statement = self
            .conn
            .prepare(sql)
            .map_err(|error| format!("SQL 准备失败：{error}"))?;
        let columns: Vec<String> = statement
            .column_names()
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let mut rows = statement
            .query([])
            .map_err(|error| format!("SQL 执行失败：{error}"))?;
        let mut values = Vec::new();
        let mut bytes = 0usize;
        let mut truncated = false;
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    if values.len() >= MAX_ROWS || bytes >= MAX_RESULT_BYTES {
                        truncated = true;
                        break;
                    }
                    let mut cells = Vec::with_capacity(columns.len());
                    for index in 0..columns.len() {
                        let value = row
                            .get_ref(index)
                            .map_err(|error| format!("读取列 {index} 失败：{error}"))?;
                        bytes += cell_size(&value);
                        cells.push(convert(value));
                    }
                    values.push(cells);
                }
                Ok(None) => break,
                Err(error) => return Err(format!("遍历结果失败：{error}")),
            }
        }
        Ok(QueryResult {
            columns,
            rows: values,
            truncated,
        })
    }
}

fn copy_sidecar(view: &View, source: &str, suffix: &str, dir: &Path) {
    let sidecar_path = format!("{source}{suffix}");
    let Ok(Some(sidecar)) = view.open(&sidecar_path) else {
        return;
    };
    let Ok(materialized) = sidecar.materialize() else {
        return;
    };
    let _ = fs::copy(
        materialized.path(),
        dir.join(format!("main.db{suffix}")),
    );
}

fn convert(value: ValueRef<'_>) -> DbValue {
    match value {
        ValueRef::Null => DbValue::Null,
        ValueRef::Integer(value) => DbValue::Integer(value),
        ValueRef::Real(value) => DbValue::Real(value),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes).into_owned();
            DbValue::Text(truncate(text))
        }
        ValueRef::Blob(bytes) => DbValue::Blob(bytes.len()),
    }
}

fn cell_size(value: &ValueRef<'_>) -> usize {
    match value {
        ValueRef::Text(bytes) => bytes.len().min(MAX_TEXT_BYTES),
        ValueRef::Blob(bytes) => bytes.len().min(MAX_TEXT_BYTES),
        _ => 16,
    }
}

fn truncate(mut text: String) -> String {
    if text.len() <= MAX_TEXT_BYTES {
        return text;
    }
    let mut end = MAX_TEXT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str("…<truncated>");
    text
}

/// Converts a cell to the JSON value stored in `mobile.artifact.data`.
pub fn to_json(value: &DbValue) -> Value {
    match value {
        DbValue::Null => Value::Nil,
        DbValue::Integer(value) => Value::from(*value),
        DbValue::Real(value) => Value::F64(*value),
        DbValue::Text(value) => Value::from(value.as_str()),
        DbValue::Blob(size) => Value::from(format!("<blob {size} bytes>").as_str()),
    }
}

/// Human-readable cell used for record keys and titles.
pub fn display(value: &DbValue) -> String {
    match value {
        DbValue::Null => String::new(),
        DbValue::Integer(value) => value.to_string(),
        DbValue::Real(value) => value.to_string(),
        DbValue::Text(value) => value.clone(),
        DbValue::Blob(size) => format!("<blob {size} bytes>"),
    }
}
