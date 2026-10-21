//! 只读 SQLite 统计：`calls` 表的行数、去重、号码、类型分布与 `date` 数量级画像。
//!
//! 设计约束：
//! * 只执行写死在代码里的固定查询，不读取、不执行检材内的 SQL 文本；
//! * 表名来自 `sqlite_master` 但会做标识符转义；列名只使用白名单常量；
//! * 缺表 / 视图 / 缺列 / 空表都是“可解释的零结果”，不是 panic；
//! * 真正的坏文件（非 SQLite 3、空文件、SQL 查询失败）返回 [`Failure::Db`]，
//!   由分析器转成明确的 LPP 错误；
//! * `date` 只按数量级判定“秒 / 毫秒”，不换算绝对时间，不推断时区。

use std::io::Read;
use std::path::Path;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

/// `calls` 表中本插件关心的列（顺序即输出顺序）。
pub const EXPECTED_COLUMNS: [&str; 7] = [
    "_id",
    "number",
    "date",
    "duration",
    "type",
    "name",
    "cached_name",
];

/// 去重键：按 `(number, date, type, duration)` 去重。
pub const DEDUP_COLUMNS: [&str; 4] = ["number", "date", "type", "duration"];

/// 号码分组最多输出的行数（按通话次数降序）；summary 中仍给出全量组数。
pub const MAX_NUMBER_ROWS: usize = 5_000;
/// `type` 分布最多输出的行数。
pub const MAX_TYPE_ROWS: usize = 256;
/// 缺表时列出的库内对象上限。
pub const MAX_LISTED_OBJECTS: usize = 20;
/// 物化副本大小上限（宿主单文件 materialize 上限为 4 GiB）。
pub const MAX_DB_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// `date` 单位判定依据（原样写进结果，便于复核）。
pub const DATE_UNIT_BASIS: &str = "按数量级判定：|值| ≥ 100000000000（1e11）视为毫秒；1000000000（1e9）≤ |值| < 1e11 视为秒；|值| < 1e9 不判定。两端点归类不同记为 mixed。仅按数量级判断，不换算绝对时间，不推断时区/夏令时。";

const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// `date` 原始数值的数量级判定结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateUnit {
    Seconds,
    Milliseconds,
    Mixed,
    Unknown,
}

impl DateUnit {
    /// 写入属性的机器可读值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Seconds => "seconds",
            Self::Milliseconds => "milliseconds",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
        }
    }

    /// 摘要里的中文说明。
    pub fn label(self) -> &'static str {
        match self {
            Self::Seconds => "秒",
            Self::Milliseconds => "毫秒",
            Self::Mixed => "mixed（两端点数量级不一致）",
            Self::Unknown => "unknown（无法判定）",
        }
    }
}

/// `date` 的原始数值（SQLite INTEGER 或 REAL）。
#[derive(Clone, Copy, Debug)]
pub enum RawNum {
    Int(i64),
    Real(f64),
}

impl RawNum {
    pub fn display(self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Real(value) => {
                if value.fract() == 0.0 {
                    format!("{value:.0}")
                } else {
                    format!("{value}")
                }
            }
        }
    }

    fn magnitude(self) -> Option<f64> {
        let magnitude = match self {
            Self::Int(value) => value as f64,
            Self::Real(value) => value,
        }
        .abs();
        magnitude.is_finite().then_some(magnitude)
    }
}

/// 结果里的备注级别（映射到 LPP finding severity）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoteLevel {
    Info,
    Low,
    Medium,
}

/// 一条可展示的备注。
#[derive(Clone, Debug)]
pub struct Note {
    pub level: NoteLevel,
    pub title: String,
    pub detail: String,
}

/// `calls` 表某列的存在性与声明类型。
#[derive(Clone, Debug)]
pub struct ColumnInfo {
    pub name: &'static str,
    pub present: bool,
    pub declared_type: Option<String>,
    pub position: Option<i64>,
}

/// 一个号码分组的统计。
#[derive(Clone, Debug)]
pub struct NumberStat {
    pub display: String,
    pub raw_type: &'static str,
    pub truncated: bool,
    pub call_count: i64,
    /// `duration` 列存在时：所有整型 duration 的合计（原始单位）。
    pub duration_total: Option<i64>,
    /// `duration` 列存在时：非 NULL 且非整型的行数（不参与合计）。
    pub duration_non_integer: Option<i64>,
}

/// 一个 `type` 取值的分布。
#[derive(Clone, Debug)]
pub struct TypeStat {
    pub display: String,
    pub raw_type: &'static str,
    pub code: Option<i64>,
    pub label: Option<&'static str>,
    pub count: i64,
}

/// 一次分析的完整统计结果。
#[derive(Clone, Debug)]
pub struct Stats {
    /// `calls` 对象的实际名称；缺表时为 `None`。
    pub table_name: Option<String>,
    /// `"table"` 或 `"view"`；缺表时为 `None`。
    pub table_kind: Option<String>,
    /// 是否真的查询了行数据（视图不查询）。
    pub table_queried: bool,
    /// 缺表时列出的库内表/视图（最多 [`MAX_LISTED_OBJECTS`] 个）。
    pub objects: Vec<String>,
    pub objects_more: bool,
    pub columns: Vec<ColumnInfo>,
    pub total_rows: i64,
    /// `_id` 列存在时：NULL 行数。
    pub id_null: Option<i64>,
    /// `_id` 列存在时：非 NULL 且非整型的行数。
    pub id_non_integer: Option<i64>,
    /// `_id` 列存在时：出现多次的 `_id` 取值个数（不代表行数）。
    pub id_duplicate_values: Option<i64>,
    pub distinct_rows: Option<i64>,
    pub distinct_key: Vec<&'static str>,
    pub number_count: Option<i64>,
    pub numbers: Vec<NumberStat>,
    pub numbers_truncated: bool,
    pub type_count: Option<i64>,
    pub types: Vec<TypeStat>,
    pub types_truncated: bool,
    pub date_min: Option<RawNum>,
    pub date_max: Option<RawNum>,
    pub date_unit: DateUnit,
    pub date_null: Option<i64>,
    pub date_non_numeric: Option<i64>,
    pub date_zero: Option<i64>,
    pub date_negative: Option<i64>,
    pub duration_total: Option<i64>,
    pub duration_null: Option<i64>,
    pub duration_non_integer: Option<i64>,
    pub name_nonempty: Option<i64>,
    pub cached_name_nonempty: Option<i64>,
    /// SQLite 头里的 write/read version 提示（`wal` / `rollback` / `unknown`）。
    pub journal_hint: Option<String>,
    pub limitations: Vec<String>,
    pub notes: Vec<Note>,
}

impl Stats {
    fn base(columns: Vec<ColumnInfo>, journal_hint: Option<String>) -> Self {
        Self {
            table_name: None,
            table_kind: None,
            table_queried: false,
            objects: Vec::new(),
            objects_more: false,
            columns,
            total_rows: 0,
            id_null: None,
            id_non_integer: None,
            id_duplicate_values: None,
            distinct_rows: None,
            distinct_key: Vec::new(),
            number_count: None,
            numbers: Vec::new(),
            numbers_truncated: false,
            type_count: None,
            types: Vec::new(),
            types_truncated: false,
            date_min: None,
            date_max: None,
            date_unit: DateUnit::Unknown,
            date_null: None,
            date_non_numeric: None,
            date_zero: None,
            date_negative: None,
            duration_total: None,
            duration_null: None,
            duration_non_integer: None,
            name_nonempty: None,
            cached_name_nonempty: None,
            journal_hint,
            limitations: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// 追加所有结果都适用的覆盖范围说明（每个路径调用一次）。
    fn finish(&mut self) {
        if self.journal_hint.as_deref() == Some("wal") {
            self.limitations.push(
                "数据库头标记为 WAL 模式（write/read version = 2），但宿主只物化了主文件；未合并 \
                 -wal/-shm，尚未 checkpoint 的通话可能缺失。"
                    .to_owned(),
            );
        }
        self.limitations.push(
            "只读打开宿主物化的单文件副本（file.materialize），不写回检材、不创建永久索引。"
                .to_owned(),
        );
        self.limitations.push(
            "未处理 -wal / -shm / -journal 边车文件；若检材只提供 contacts2.db 单文件，边车中的\
             未合并记录不在统计范围内。"
                .to_owned(),
        );
        self.limitations.push(
            "date 只输出原始数值并按数量级判定秒/毫秒，不换算成绝对时间，不推断时区或夏令时；\
             判定结果仅供筛查参考，不能单独作为时间结论。"
                .to_owned(),
        );
        self.limitations.push(
            "duration 只累加整型值，按原始单位输出；Android 通常以秒为单位，但本插件不换算、\
             也不校验该语义。"
                .to_owned(),
        );
        self.limitations.push(
            "号码按原始值分组与展示，不做归一化（空格、+86、区号等）或归属地判断；未解析 \
             contacts 联系人表，也未把 name/cached_name 与号码关联。"
                .to_owned(),
        );
        self.limitations
            .push("只执行固定的只读查询，不执行检材内的 SQL 文本，不联网。".to_owned());
    }
}

/// 分析失败：取消是宿主请求，其余是明确的数据库错误。
#[derive(Debug)]
pub enum Failure {
    Cancelled,
    Db(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("cancelled"),
            Self::Db(message) => formatter.write_str(message),
        }
    }
}

/// 分析物化副本 `path`；`is_cancelled` 在主要查询之间轮询。
pub fn analyze_path(path: &Path, is_cancelled: &dyn Fn() -> bool) -> Result<Stats, Failure> {
    check_cancel(is_cancelled)?;
    let size = std::fs::metadata(path)
        .map_err(|error| Failure::Db(format!("无法读取物化副本元数据：{error}")))?
        .len();
    if size == 0 {
        return Err(Failure::Db(
            "文件为空（0 字节），不是有效的 SQLite 3 数据库".to_owned(),
        ));
    }
    if size > MAX_DB_BYTES {
        return Err(Failure::Db(format!(
            "文件大小 {size} 字节超过本插件上限 {MAX_DB_BYTES} 字节"
        )));
    }
    let journal_hint = check_sqlite_header(path, size)?;
    check_cancel(is_cancelled)?;

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        Failure::Db(format!(
            "SQLite 只读打开失败：{error}（文件头已通过 SQLite 3 校验，数据库可能损坏或需要日志恢复）"
        ))
    })?;
    connection
        .execute_batch(
            "PRAGMA query_only = ON; PRAGMA trusted_schema = OFF; PRAGMA temp_store = MEMORY;",
        )
        .map_err(|error| Failure::Db(format!("设置只读 PRAGMA 失败：{error}")))?;
    check_cancel(is_cancelled)?;

    let mut stats = Stats::base(absent_columns(), journal_hint);
    let found = find_calls(&connection)?;
    let Some((name, kind)) = found else {
        let (objects, objects_more) = list_objects(&connection)?;
        stats.objects = objects;
        stats.objects_more = objects_more;
        stats.limitations.push(
            "未找到名为 calls 的表（SQLite 对象名按 ASCII 大小写不敏感匹配）；未执行任何通话统计。"
                .to_owned(),
        );
        if stats.objects.is_empty() {
            stats.limitations.push("库内没有用户表或视图。".to_owned());
        } else {
            stats.limitations.push(format!(
                "库内表/视图（最多 {MAX_LISTED_OBJECTS} 个）：{}{}",
                stats.objects.join(", "),
                if stats.objects_more { " …" } else { "" }
            ));
        }
        stats.notes.push(Note {
            level: NoteLevel::Medium,
            title: "未找到 calls 表".to_owned(),
            detail: "contacts2.db 中不存在可查询的 calls 表，未生成通话统计。".to_owned(),
        });
        stats.finish();
        return Ok(stats);
    };

    stats.table_name = Some(name.clone());
    stats.table_kind = Some(kind.clone());
    stats.columns = read_columns(&connection, &name)?;

    if kind != "table" {
        stats.limitations.push(
            "名为 calls 的对象是视图（view），不是表；为避免执行检材内定义的 SQL，本插件只\
             接受表对象，未查询视图。"
                .to_owned(),
        );
        stats.notes.push(Note {
            level: NoteLevel::Medium,
            title: "calls 是视图，未查询".to_owned(),
            detail: "为遵守“不执行检材内 SQL 文本”的约束，视图 calls 不会被 SELECT。".to_owned(),
        });
        stats.finish();
        return Ok(stats);
    }

    stats.table_queried = true;
    let table = quote_identifier(&name);

    stats.total_rows = query_i64(
        &connection,
        &format!("SELECT COUNT(*) FROM {table}"),
        "统计 calls 总行数",
    )?;
    check_cancel(is_cancelled)?;
    collect_ids(&connection, &table, &mut stats)?;
    check_cancel(is_cancelled)?;

    // 去重键：只使用实际存在的列，并把“缺列导致键变短”写清楚。
    let key: Vec<&'static str> = DEDUP_COLUMNS
        .iter()
        .copied()
        .filter(|column| has_column(&stats.columns, column))
        .collect();
    stats.distinct_key = key.clone();
    if key.is_empty() {
        stats.distinct_rows = Some(stats.total_rows);
        stats.limitations.push(
            "number/date/type/duration 全部缺失，无法去重；去重行数按总行数计，未做实际去重。"
                .to_owned(),
        );
    } else {
        let key_sql = key
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        stats.distinct_rows = Some(query_i64(
            &connection,
            &format!("SELECT COUNT(*) FROM (SELECT DISTINCT {key_sql} FROM {table}) AS d"),
            "统计去重行数",
        )?);
        let missing: Vec<&str> = DEDUP_COLUMNS
            .iter()
            .copied()
            .filter(|column| !key.contains(column))
            .collect();
        if !missing.is_empty() {
            stats.limitations.push(format!(
                "去重键只使用实际存在的列（{}），缺少 {}；去重行数与四列 \
                 (number,date,type,duration) 去重不可直接比较。",
                key.join(", "),
                missing.join(", ")
            ));
        }
    }
    check_cancel(is_cancelled)?;

    if let Some(distinct) = stats.distinct_rows {
        let duplicates = (stats.total_rows - distinct).max(0);
        if duplicates > 0 {
            let key_label = if stats.distinct_key.is_empty() {
                "无可去重列".to_owned()
            } else {
                format!("({})", stats.distinct_key.join(","))
            };
            stats.notes.push(Note {
                level: NoteLevel::Info,
                title: "存在重复通话记录".to_owned(),
                detail: format!(
                    "总行数 {}，按 {key_label} 去重后 {} 行，重复 {} 行。",
                    stats.total_rows, distinct, duplicates
                ),
            });
        }
    }

    collect_numbers(&connection, &table, &mut stats, is_cancelled)?;
    check_cancel(is_cancelled)?;
    collect_types(&connection, &table, &mut stats, is_cancelled)?;
    check_cancel(is_cancelled)?;
    collect_dates(&connection, &table, &mut stats, is_cancelled)?;
    check_cancel(is_cancelled)?;
    collect_durations(&connection, &table, &mut stats)?;
    collect_names(&connection, &table, &mut stats)?;

    if stats.total_rows == 0 {
        stats
            .limitations
            .push("calls 表存在但为空（0 行）。".to_owned());
        stats.notes.push(Note {
            level: NoteLevel::Info,
            title: "calls 表为空".to_owned(),
            detail: "表结构与列检查已完成，但没有可统计的通话记录。".to_owned(),
        });
    }

    let missing_dims: Vec<&str> = DEDUP_COLUMNS
        .iter()
        .copied()
        .filter(|column| !has_column(&stats.columns, column))
        .collect();
    if !missing_dims.is_empty() {
        stats.notes.push(Note {
            level: NoteLevel::Low,
            title: "calls 表缺少关键列".to_owned(),
            detail: format!(
                "缺少：{}；对应维度未统计（不使用替代列或推断值）。",
                missing_dims.join(", ")
            ),
        });
    }
    let missing_all: Vec<&str> = EXPECTED_COLUMNS
        .iter()
        .copied()
        .filter(|column| !has_column(&stats.columns, column))
        .collect();
    if !missing_all.is_empty() {
        stats.limitations.push(format!(
            "calls 表缺少预期列：{}；按存在列查询，缺失维度不统计。",
            missing_all.join(", ")
        ));
    }

    stats.finish();
    Ok(stats)
}

/// 判断一个数值是否为合法 SQLite 文件头，并给出 WAL/回滚日志提示。
fn check_sqlite_header(path: &Path, size: u64) -> Result<Option<String>, Failure> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| Failure::Db(format!("无法打开物化副本：{error}")))?;
    let length = usize::try_from(size.min(100)).unwrap_or(100);
    let mut header = vec![0u8; length];
    if let Err(error) = file.read_exact(&mut header) {
        return Err(Failure::Db(format!(
            "读取文件头失败（需要 {length} 字节）：{error}"
        )));
    }
    if header.len() < SQLITE_MAGIC.len() || &header[..SQLITE_MAGIC.len()] != SQLITE_MAGIC {
        let prefix = header
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        return Err(Failure::Db(format!(
            "文件头不是 SQLite 3 魔数（期望 'SQLite format 3\\0'），实际前 {} 字节为 0x{prefix}；\
             可能是加密数据库（SQLCipher）、其它格式，或文件已损坏。",
            header.len().min(8)
        )));
    }
    let hint = if header.len() >= 20 {
        let write_version = header[18];
        let read_version = header[19];
        if write_version == 2 || read_version == 2 {
            "wal"
        } else if write_version == 1 || read_version == 1 {
            "rollback"
        } else {
            "unknown"
        }
    } else {
        "unknown"
    };
    Ok(Some(hint.to_owned()))
}

fn check_cancel(is_cancelled: &dyn Fn() -> bool) -> Result<(), Failure> {
    if is_cancelled() {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}

/// 查找 `calls` 表或视图（大小写不敏感，优先表）。
fn find_calls(connection: &Connection) -> Result<Option<(String, String)>, Failure> {
    let mut statement = connection
        .prepare(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table', 'view') AND lower(name) = 'calls' \
             ORDER BY CASE type WHEN 'table' THEN 0 ELSE 1 END, name LIMIT 1",
        )
        .map_err(|error| Failure::Db(format!("查询 sqlite_master 失败：{error}")))?;
    let mut rows = statement
        .query([])
        .map_err(|error| Failure::Db(format!("查询 sqlite_master 失败：{error}")))?;
    match rows
        .next()
        .map_err(|error| Failure::Db(format!("读取 sqlite_master 失败：{error}")))?
    {
        Some(row) => {
            let name: String = row
                .get(0)
                .map_err(|error| Failure::Db(format!("读取表名失败：{error}")))?;
            let kind: String = row
                .get(1)
                .map_err(|error| Failure::Db(format!("读取对象类型失败：{error}")))?;
            Ok(Some((name, kind)))
        }
        None => Ok(None),
    }
}

/// 缺表时列出库内对象（跳过 `sqlite_` 内部对象）；第二个返回值表示是否截断。
fn list_objects(connection: &Connection) -> Result<(Vec<String>, bool), Failure> {
    let mut statement = connection
        .prepare(
            "SELECT type, name FROM sqlite_master \
             WHERE type IN ('table', 'view') ORDER BY type, name LIMIT 256",
        )
        .map_err(|error| Failure::Db(format!("列出库内对象失败：{error}")))?;
    let mut rows = statement
        .query([])
        .map_err(|error| Failure::Db(format!("列出库内对象失败：{error}")))?;
    let mut objects = Vec::new();
    let mut more = false;
    while let Some(row) = rows
        .next()
        .map_err(|error| Failure::Db(format!("列出库内对象失败：{error}")))?
    {
        let kind: String = row
            .get(0)
            .map_err(|error| Failure::Db(format!("读取对象类型失败：{error}")))?;
        let name: String = row
            .get(1)
            .map_err(|error| Failure::Db(format!("读取对象名失败：{error}")))?;
        if name.to_ascii_lowercase().starts_with("sqlite_") {
            continue;
        }
        if objects.len() >= MAX_LISTED_OBJECTS {
            more = true;
            continue;
        }
        objects.push(format!("{kind}:{name}"));
    }
    Ok((objects, more))
}

/// 读取 `calls` 上 7 个预期列的存在性、声明类型与序号。
fn read_columns(connection: &Connection, table: &str) -> Result<Vec<ColumnInfo>, Failure> {
    let mut found: Vec<(String, Option<String>, i64)> = Vec::new();
    let mut statement = connection
        .prepare("SELECT name, type, cid FROM pragma_table_info(?1)")
        .map_err(|error| Failure::Db(format!("读取列信息失败：{error}")))?;
    let mut rows = statement
        .query([table])
        .map_err(|error| Failure::Db(format!("读取列信息失败：{error}")))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| Failure::Db(format!("读取列信息失败：{error}")))?
    {
        let name: String = row
            .get(0)
            .map_err(|error| Failure::Db(format!("读取列名失败：{error}")))?;
        let declared: Option<String> = row
            .get(1)
            .map_err(|error| Failure::Db(format!("读取列类型失败：{error}")))?;
        let cid: i64 = row
            .get(2)
            .map_err(|error| Failure::Db(format!("读取列序号失败：{error}")))?;
        found.push((name, declared, cid));
    }
    Ok(EXPECTED_COLUMNS
        .iter()
        .map(|expected| {
            let hit = found
                .iter()
                .find(|(name, _, _)| name.eq_ignore_ascii_case(expected));
            ColumnInfo {
                name: expected,
                present: hit.is_some(),
                declared_type: hit.and_then(|(_, declared, _)| declared.clone()),
                position: hit.map(|(_, _, cid)| *cid),
            }
        })
        .collect())
}

fn absent_columns() -> Vec<ColumnInfo> {
    EXPECTED_COLUMNS
        .iter()
        .map(|name| ColumnInfo {
            name,
            present: false,
            declared_type: None,
            position: None,
        })
        .collect()
}

fn has_column(columns: &[ColumnInfo], name: &str) -> bool {
    columns
        .iter()
        .any(|column| column.name == name && column.present)
}

/// `_id` 的行标识完整性统计（NULL / 非整型 / 重复取值）。
fn collect_ids(connection: &Connection, table: &str, stats: &mut Stats) -> Result<(), Failure> {
    if !has_column(&stats.columns, "_id") {
        stats
            .limitations
            .push("_id 列缺失，未做行标识完整性检查。".to_owned());
        return Ok(());
    }
    let sql = format!(
        "SELECT \
           COALESCE(SUM(CASE WHEN \"_id\" IS NULL THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN \"_id\" IS NOT NULL AND typeof(\"_id\") <> 'integer' \
             THEN 1 ELSE 0 END), 0), \
           (SELECT COUNT(*) FROM \
             (SELECT \"_id\" FROM {table} GROUP BY \"_id\" HAVING COUNT(*) > 1) AS duplicated) \
         FROM {table}"
    );
    let (nulls, non_integer, duplicate_values) = connection
        .query_row(&sql, [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|error| Failure::Db(format!("统计 _id 完整性失败：{error}")))?;
    stats.id_null = Some(nulls);
    stats.id_non_integer = Some(non_integer);
    stats.id_duplicate_values = Some(duplicate_values);
    if nulls > 0 {
        stats.limitations.push(format!(
            "_id 为 NULL 的 {nulls} 行不满足“唯一行标识”的假设。"
        ));
    }
    if non_integer > 0 {
        stats.limitations.push(format!(
            "_id 为非整型的 {non_integer} 行（TEXT/BLOB/REAL），仍按原始值参与分组统计。"
        ));
    }
    if duplicate_values > 0 {
        stats.notes.push(Note {
            level: NoteLevel::Low,
            title: "_id 存在重复取值".to_owned(),
            detail: format!(
                "{duplicate_values} 个 _id 取值出现多次；该库的 _id 可能不是唯一主键，\
                 本插件仍按业务键去重。"
            ),
        });
    }
    Ok(())
}

fn collect_numbers(
    connection: &Connection,
    table: &str,
    stats: &mut Stats,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    if !has_column(&stats.columns, "number") {
        stats
            .limitations
            .push("number 列缺失，未统计每个号码的通话次数与总时长。".to_owned());
        return Ok(());
    }
    let duration_expr = if has_column(&stats.columns, "duration") {
        "COALESCE(SUM(CASE WHEN typeof(\"duration\") = 'integer' THEN \"duration\" ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN \"duration\" IS NOT NULL AND typeof(\"duration\") <> 'integer' \
         THEN 1 ELSE 0 END), 0)"
    } else {
        "NULL, NULL"
    };
    let sql = format!(
        "SELECT \"number\", COUNT(*), {duration_expr} FROM {table} \
         GROUP BY \"number\" ORDER BY 2 DESC LIMIT {}",
        MAX_NUMBER_ROWS + 1
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| Failure::Db(format!("查询号码分组失败：{error}")))?;
    let mut rows = statement
        .query([])
        .map_err(|error| Failure::Db(format!("查询号码分组失败：{error}")))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| Failure::Db(format!("读取号码分组失败：{error}")))?
    {
        let raw = row
            .get_ref(0)
            .map_err(|error| Failure::Db(format!("读取号码失败：{error}")))?;
        let (display, raw_type, truncated) = display_sql_value(raw, 128);
        let call_count: i64 = row
            .get(1)
            .map_err(|error| Failure::Db(format!("读取号码通话次数失败：{error}")))?;
        let duration_total: Option<i64> = row
            .get(2)
            .map_err(|error| Failure::Db(format!("读取号码总时长失败：{error}")))?;
        let duration_non_integer: Option<i64> = row
            .get(3)
            .map_err(|error| Failure::Db(format!("读取号码时长类型失败：{error}")))?;
        stats.numbers.push(NumberStat {
            display,
            raw_type,
            truncated,
            call_count,
            duration_total,
            duration_non_integer,
        });
        if stats.numbers.len() > MAX_NUMBER_ROWS {
            break;
        }
        check_cancel(is_cancelled)?;
    }
    if stats.numbers.len() > MAX_NUMBER_ROWS {
        stats.numbers.truncate(MAX_NUMBER_ROWS);
        stats.numbers_truncated = true;
        stats.limitations.push(format!(
            "号码分组超过 {MAX_NUMBER_ROWS} 个，明细只输出通话次数最多的 {MAX_NUMBER_ROWS} 个；\
             号码总数仍为全量统计。"
        ));
    }
    stats.number_count = Some(query_i64(
        connection,
        &format!("SELECT COUNT(*) FROM (SELECT 1 FROM {table} GROUP BY \"number\") AS g"),
        "统计号码总数",
    )?);
    if stats.numbers.iter().any(|number| number.truncated) {
        stats
            .limitations
            .push("部分号码文本超过 128 个字符，展示时已截断；分组仍按原始值进行。".to_owned());
    }
    if stats
        .numbers
        .iter()
        .any(|number| number.duration_non_integer.unwrap_or(0) > 0)
    {
        stats
            .limitations
            .push("部分号码存在非整型 duration（REAL/TEXT/BLOB），未计入总时长。".to_owned());
    }
    Ok(())
}

fn collect_types(
    connection: &Connection,
    table: &str,
    stats: &mut Stats,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    if !has_column(&stats.columns, "type") {
        stats
            .limitations
            .push("type 列缺失，未统计通话类型分布。".to_owned());
        return Ok(());
    }
    let sql = format!(
        "SELECT \"type\", COUNT(*) FROM {table} \
         GROUP BY \"type\" ORDER BY 2 DESC LIMIT {}",
        MAX_TYPE_ROWS + 1
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| Failure::Db(format!("查询 type 分布失败：{error}")))?;
    let mut rows = statement
        .query([])
        .map_err(|error| Failure::Db(format!("查询 type 分布失败：{error}")))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| Failure::Db(format!("读取 type 分布失败：{error}")))?
    {
        let raw = row
            .get_ref(0)
            .map_err(|error| Failure::Db(format!("读取 type 失败：{error}")))?;
        let (display, raw_type, _) = display_sql_value(raw, 64);
        let code = match raw {
            ValueRef::Integer(value) => Some(value),
            _ => None,
        };
        let label = code.and_then(type_label);
        let count: i64 = row
            .get(1)
            .map_err(|error| Failure::Db(format!("读取 type 计数失败：{error}")))?;
        stats.types.push(TypeStat {
            display,
            raw_type,
            code,
            label,
            count,
        });
        if stats.types.len() > MAX_TYPE_ROWS {
            break;
        }
        check_cancel(is_cancelled)?;
    }
    if stats.types.len() > MAX_TYPE_ROWS {
        stats.types.truncate(MAX_TYPE_ROWS);
        stats.types_truncated = true;
        stats.limitations.push(format!(
            "type 取值超过 {MAX_TYPE_ROWS} 个，明细已截断；类型总数仍为全量统计。"
        ));
    }
    stats.type_count = Some(query_i64(
        connection,
        &format!("SELECT COUNT(*) FROM (SELECT 1 FROM {table} GROUP BY \"type\") AS g"),
        "统计 type 总数",
    )?);
    if stats
        .types
        .iter()
        .any(|entry| entry.code.is_none() || entry.label.is_none())
    {
        stats.limitations.push(
            "存在未映射的 type 取值（非整数或不在 1~7 的 AOSP CallLog 常量内）；原始值已保留，\
             未做猜测归类。"
                .to_owned(),
        );
    }
    Ok(())
}

fn collect_dates(
    connection: &Connection,
    table: &str,
    stats: &mut Stats,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    if !has_column(&stats.columns, "date") {
        stats
            .limitations
            .push("date 列缺失，未输出最早/最晚时间。".to_owned());
        return Ok(());
    }
    let sql = format!(
        "SELECT \
           MIN(CASE WHEN typeof(\"date\") IN ('integer', 'real') THEN \"date\" END), \
           MAX(CASE WHEN typeof(\"date\") IN ('integer', 'real') THEN \"date\" END), \
           COALESCE(SUM(CASE WHEN \"date\" IS NULL THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN \"date\" IS NOT NULL AND typeof(\"date\") NOT IN \
             ('integer', 'real') THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN typeof(\"date\") IN ('integer', 'real') AND \"date\" = 0 \
             THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN typeof(\"date\") IN ('integer', 'real') AND \"date\" < 0 \
             THEN 1 ELSE 0 END), 0) \
         FROM {table}"
    );
    let (min, max, nulls, non_numeric, zeros, negatives) = connection
        .query_row(&sql, [], |row| {
            Ok((
                raw_num(row.get_ref(0)?),
                raw_num(row.get_ref(1)?),
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|error| Failure::Db(format!("统计 date 范围失败：{error}")))?;
    check_cancel(is_cancelled)?;

    stats.date_min = min;
    stats.date_max = max;
    stats.date_null = Some(nulls);
    stats.date_non_numeric = Some(non_numeric);
    stats.date_zero = Some(zeros);
    stats.date_negative = Some(negatives);
    stats.date_unit = classify_date_unit(min, max);

    let endpoint_unclassified = min.is_some_and(|value| classify_magnitude(value).is_none())
        || max.is_some_and(|value| classify_magnitude(value).is_none());
    if stats.date_unit != DateUnit::Unknown && endpoint_unclassified {
        stats.limitations.push(
            "date 端点含 0 或过小值，单位判定只依据可判定的端点；该判定不覆盖这些异常值。"
                .to_owned(),
        );
    }

    if min.is_none() && max.is_none() && stats.total_rows > 0 {
        stats
            .limitations
            .push("date 列存在，但没有数值型取值，无法给出最早/最晚时间。".to_owned());
        stats.notes.push(Note {
            level: NoteLevel::Low,
            title: "date 没有数值型取值".to_owned(),
            detail: "所有 date 均为 NULL 或非数值类型，未做单位判定。".to_owned(),
        });
    }
    if nulls > 0 {
        stats.limitations.push(format!(
            "date 为 NULL 的 {nulls} 行不参与最早/最晚与单位判定。"
        ));
    }
    if non_numeric > 0 {
        stats.limitations.push(format!(
            "date 为非数值类型（TEXT/BLOB）的 {non_numeric} 行未参与统计，原值保留在库内。"
        ));
        stats.notes.push(Note {
            level: NoteLevel::Low,
            title: "date 含非数值取值".to_owned(),
            detail: format!("{non_numeric} 行 date 不是 INTEGER/REAL，未做解析或猜测。"),
        });
    }
    if zeros > 0 {
        stats.limitations.push(format!(
            "date = 0 的 {zeros} 行（Android 中常表示未知/未设置），仍按 0 参与范围统计。"
        ));
    }
    if negatives > 0 {
        stats.limitations.push(format!(
            "date < 0 的 {negatives} 行（1970 年之前或异常值），按原始负数保留。"
        ));
    }
    if stats.date_unit == DateUnit::Mixed {
        stats.notes.push(Note {
            level: NoteLevel::Low,
            title: "date 数量级不一致".to_owned(),
            detail: "最早/最晚端点分别落在秒和毫秒区间；可能混用了不同单位，未做换算。".to_owned(),
        });
    } else if stats.date_unit == DateUnit::Unknown && (min.is_some() || max.is_some()) {
        stats.notes.push(Note {
            level: NoteLevel::Low,
            title: "date 单位无法判定".to_owned(),
            detail: format!(
                "原始范围 {}..{} 的数量级不在秒/毫秒判定区间内。",
                min.map(RawNum::display).unwrap_or_else(|| "无".to_owned()),
                max.map(RawNum::display).unwrap_or_else(|| "无".to_owned())
            ),
        });
    }
    Ok(())
}

fn collect_durations(
    connection: &Connection,
    table: &str,
    stats: &mut Stats,
) -> Result<(), Failure> {
    if !has_column(&stats.columns, "duration") {
        stats
            .limitations
            .push("duration 列缺失，未统计总时长。".to_owned());
        return Ok(());
    }
    let sql = format!(
        "SELECT \
           COALESCE(SUM(CASE WHEN typeof(\"duration\") = 'integer' THEN \"duration\" ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN \"duration\" IS NULL THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN \"duration\" IS NOT NULL AND typeof(\"duration\") <> 'integer' \
             THEN 1 ELSE 0 END), 0) \
         FROM {table}"
    );
    let (total, nulls, non_integer) = connection
        .query_row(&sql, [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|error| Failure::Db(format!("统计 duration 失败：{error}")))?;
    stats.duration_total = Some(total);
    stats.duration_null = Some(nulls);
    stats.duration_non_integer = Some(non_integer);
    if nulls > 0 {
        stats
            .limitations
            .push(format!("duration 为 NULL 的 {nulls} 行未计入总时长。"));
    }
    if non_integer > 0 {
        stats.limitations.push(format!(
            "duration 为非整型（REAL/TEXT/BLOB）的 {non_integer} 行未计入总时长，未做类型转换。"
        ));
        stats.notes.push(Note {
            level: NoteLevel::Low,
            title: "duration 含非整型取值".to_owned(),
            detail: format!("{non_integer} 行 duration 不是 INTEGER，未计入合计。"),
        });
    }
    Ok(())
}

fn collect_names(connection: &Connection, table: &str, stats: &mut Stats) -> Result<(), Failure> {
    for column in ["name", "cached_name"] {
        if !has_column(&stats.columns, column) {
            continue;
        }
        let sql = format!(
            "SELECT COALESCE(SUM(CASE WHEN typeof(\"{column}\") = 'text' \
             AND length(trim(\"{column}\")) > 0 THEN 1 ELSE 0 END), 0) FROM {table}"
        );
        let count = query_i64(connection, &sql, &format!("统计 {column} 非空行数"))?;
        if column == "name" {
            stats.name_nonempty = Some(count);
        } else {
            stats.cached_name_nonempty = Some(count);
        }
    }
    Ok(())
}

fn query_i64(connection: &Connection, sql: &str, context: &str) -> Result<i64, Failure> {
    connection
        .query_row(sql, [], |row| row.get::<_, i64>(0))
        .map_err(|error| Failure::Db(format!("{context}失败：{error}")))
}

fn raw_num(value: ValueRef<'_>) -> Option<RawNum> {
    match value {
        ValueRef::Integer(value) => Some(RawNum::Int(value)),
        ValueRef::Real(value) if value.is_finite() => Some(RawNum::Real(value)),
        _ => None,
    }
}

fn classify_magnitude(value: RawNum) -> Option<DateUnit> {
    let magnitude = value.magnitude()?;
    if magnitude >= 100_000_000_000.0 {
        Some(DateUnit::Milliseconds)
    } else if magnitude >= 1_000_000_000.0 {
        Some(DateUnit::Seconds)
    } else {
        None
    }
}

fn classify_date_unit(min: Option<RawNum>, max: Option<RawNum>) -> DateUnit {
    let lower = min.and_then(classify_magnitude);
    let upper = max.and_then(classify_magnitude);
    match (lower, upper) {
        (Some(left), Some(right)) if left == right => left,
        (Some(_), Some(_)) => DateUnit::Mixed,
        (Some(unit), None) | (None, Some(unit)) => unit,
        (None, None) => DateUnit::Unknown,
    }
}

/// AOSP `CallLog.Calls` 常量映射；未知值不猜测，由调用方保留原始值。
pub fn type_label(code: i64) -> Option<&'static str> {
    match code {
        1 => Some("来电（INCOMING）"),
        2 => Some("去电（OUTGOING）"),
        3 => Some("未接（MISSED）"),
        4 => Some("语音信箱（VOICEMAIL）"),
        5 => Some("拒接（REJECTED）"),
        6 => Some("拦截（BLOCKED）"),
        7 => Some("外部接听（ANSWERED_EXTERNALLY）"),
        _ => None,
    }
}

/// 把 SQLite 值转成可展示文本（不展开 BLOB，文本按字符截断）。
fn display_sql_value(value: ValueRef<'_>, max_chars: usize) -> (String, &'static str, bool) {
    match value {
        ValueRef::Null => ("(NULL)".to_owned(), "null", false),
        ValueRef::Integer(value) => (value.to_string(), "integer", false),
        ValueRef::Real(value) => (format!("{value}"), "real", false),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            let count = text.chars().count();
            if count > max_chars {
                let prefix: String = text.chars().take(max_chars).collect();
                (format!("{prefix}…"), "text", true)
            } else {
                (text.into_owned(), "text", false)
            }
        }
        ValueRef::Blob(bytes) => {
            let prefix = bytes
                .iter()
                .take(16)
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let truncated = bytes.len() > 16;
            (
                format!("blob:0x{prefix}{}", if truncated { "…" } else { "" }),
                "blob",
                truncated,
            )
        }
    }
}

/// SQLite 标识符转义（表名来自 sqlite_master，仍按不可信输入处理）。
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
