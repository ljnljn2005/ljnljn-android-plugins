//! Android `contacts2.db` calls-table statistics (LPP v2).
//!
//! Port of the v1 `ljnljn.android-contacts2.file` analyzer. The read-only SQL
//! layer (`stats`) is carried over unchanged: it opens the host-materialized
//! copy with `SQLITE_OPEN_READ_ONLY`, executes only fixed SQL text, and never
//! feeds evidence SQL into SQLite.
//!
//! Preserved semantics:
//! - `total_rows` is the row count of `calls`;
//! - `distinct_rows` is `COUNT(*)` over `SELECT DISTINCT` of the dedup key
//!   `(number, date, type, duration)`, shortened to the columns that exist;
//! - `duplicate_rows = total_rows - distinct_rows` (duplicates are collapsed,
//!   never deleted from the evidence);
//! - per-number call counts and duration sums, `type` distribution, and the
//!   magnitude-only `date` unit classification are unchanged.
//!
//! `date` is still never time-zone-shifted. When the unit is unambiguous
//! (seconds or milliseconds since the Unix epoch) the summary additionally
//! carries `date_min_utc` / `date_max_utc` RFC 3339 projections; the raw
//! values and the classification basis stay in the record. `mixed` and
//! `unknown` produce no timestamp at all.
//!
//! A database that cannot be opened or parsed is reported as a warning plus
//! an explicit `android.contacts2.error` record; a missing table, a view and
//! an empty table are reported as an explicit summary status.

pub mod stats;

#[cfg(test)]
mod tests;

use lovelycarve_plugin::{export_plugin, FileRef, Plugin, Record, Result, Run, Timestamp};

use crate::stats::{
    ColumnInfo, DateUnit, NumberStat, RawNum, Stats, TypeStat, DATE_UNIT_BASIS, MAX_DB_BYTES,
};

/// Analyzer id (must match `plugin.toml`).
pub const ANALYZER: &str = "contacts2_calls";
/// At most this many limitation records are emitted per input (v1 parity).
pub const MAX_LIMITATIONS: usize = 64;

/// One `calls` table overview per input file.
#[derive(Record)]
#[record(
    id = "android.contacts2.summary",
    label = "contacts2.db calls 概览",
    title = "{file}"
)]
struct CallsSummary {
    #[field(label = "文件", key)]
    file: String,
    #[field(
        label = "状态",
        kind = "enum",
        choices = "ok=有通话记录,empty_table=表为空,missing_table=没有 calls 表,view=calls 是视图,error=数据库错误"
    )]
    status: String,
    #[field(label = "表名")]
    table: Option<String>,
    #[field(label = "对象类型")]
    table_kind: Option<String>,
    #[field(label = "是否查询行数据")]
    table_queried: bool,
    #[field(label = "总行数")]
    total_rows: i64,
    #[field(label = "_id 为 NULL 行数")]
    id_null: Option<i64>,
    #[field(label = "_id 非整型行数")]
    id_non_integer: Option<i64>,
    #[field(label = "_id 重复取值数")]
    id_duplicate_values: Option<i64>,
    #[field(label = "去重后行数")]
    distinct_rows: Option<i64>,
    #[field(label = "去重键", kind = "list")]
    distinct_key: Vec<String>,
    #[field(label = "重复行数")]
    duplicate_rows: Option<i64>,
    #[field(label = "号码数")]
    number_count: Option<i64>,
    #[field(label = "号码明细被截断")]
    numbers_truncated: bool,
    #[field(label = "type 取值数")]
    type_count: Option<i64>,
    #[field(label = "type 明细被截断")]
    types_truncated: bool,
    #[field(label = "最早 date（原始值）")]
    date_min_raw: Option<String>,
    #[field(label = "最晚 date（原始值）")]
    date_max_raw: Option<String>,
    #[field(label = "最早 date（UTC 投影）")]
    date_min_utc: Option<Timestamp>,
    #[field(label = "最晚 date（UTC 投影）")]
    date_max_utc: Option<Timestamp>,
    #[field(
        label = "date 单位判定",
        kind = "enum",
        choices = "seconds=秒,milliseconds=毫秒,mixed=混合,unknown=未知"
    )]
    date_unit: String,
    #[field(label = "date 单位判定依据")]
    date_unit_basis: String,
    #[field(label = "date 为 NULL 行数")]
    date_null: Option<i64>,
    #[field(label = "date 非数值行数")]
    date_non_numeric: Option<i64>,
    #[field(label = "date = 0 行数")]
    date_zero: Option<i64>,
    #[field(label = "date < 0 行数")]
    date_negative: Option<i64>,
    #[field(label = "duration 合计（原始单位）")]
    duration_total_raw: Option<i64>,
    #[field(label = "duration 为 NULL 行数")]
    duration_null: Option<i64>,
    #[field(label = "非整型 duration 行数")]
    duration_non_integer: Option<i64>,
    #[field(label = "name 非空行数")]
    name_nonempty: Option<i64>,
    #[field(label = "cached_name 非空行数")]
    cached_name_nonempty: Option<i64>,
    #[field(label = "存在的列", kind = "list")]
    columns_present: Vec<String>,
    #[field(label = "缺失的列", kind = "list")]
    columns_missing: Vec<String>,
    #[field(label = "日志模式提示")]
    journal_hint: Option<String>,
    #[field(label = "库内表/视图")]
    objects_seen: Vec<String>,
    #[field(label = "库内对象被截断")]
    objects_more: bool,
    #[field(label = "限制与覆盖说明", kind = "list")]
    limitations: Vec<String>,
}

/// Presence and declared type of one expected `calls` column.
#[derive(Record)]
#[record(
    id = "android.contacts2.column",
    label = "calls 列检查",
    title = "{file} · {name}"
)]
struct ColumnRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "列名", key)]
    name: String,
    #[field(label = "是否存在")]
    present: bool,
    #[field(label = "声明类型")]
    declared_type: Option<String>,
    #[field(label = "列序号")]
    position: Option<i64>,
}

/// One phone number group.
#[derive(Record)]
#[record(
    id = "android.contacts2.number",
    label = "按号码统计",
    title = "{number}"
)]
struct NumberRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "号码（原始值）", key)]
    number: String,
    #[field(label = "号码原始类型")]
    raw_type: String,
    #[field(label = "号码展示是否截断")]
    truncated: bool,
    #[field(label = "通话次数")]
    call_count: i64,
    #[field(label = "总时长（原始单位）")]
    duration_total_raw: Option<i64>,
    #[field(label = "非整型 duration 行数")]
    duration_non_integer: Option<i64>,
}

/// One `type` value of the distribution.
#[derive(Record)]
#[record(
    id = "android.contacts2.type",
    label = "type 分布",
    title = "{type_raw}"
)]
struct TypeRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "type 原始值", key)]
    type_raw: String,
    #[field(label = "type 原始类型")]
    raw_type: String,
    #[field(label = "type 数值")]
    code: Option<i64>,
    #[field(label = "type 含义")]
    label: Option<String>,
    #[field(label = "通话次数")]
    call_count: i64,
}

/// One interpretive note from the SQL layer (duplicates, missing columns, …).
#[derive(Record)]
#[record(id = "android.contacts2.note", label = "统计说明", title = "{title}")]
struct NoteRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "标题", key)]
    title: String,
    #[field(label = "级别", kind = "enum", choices = "info=信息,low=低,medium=中")]
    level: String,
    #[field(label = "说明")]
    detail: String,
}

/// One coverage/limitation statement from the SQL layer.
#[derive(Record)]
#[record(
    id = "android.contacts2.limitation",
    label = "限制与覆盖说明",
    title = "{text}"
)]
struct LimitationRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "序号", key)]
    index: i64,
    #[field(label = "限制说明")]
    text: String,
}

/// A database that could not be opened or queried at all.
#[derive(Record)]
#[record(
    id = "android.contacts2.error",
    label = "contacts2.db 读取失败",
    title = "{file}"
)]
struct ErrorRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "错误代码")]
    code: String,
    #[field(label = "错误说明")]
    message: String,
}

fn contacts2(run: &Run) -> Result<()> {
    if run.inputs().is_empty() {
        run.warn(
            "INPUT_NOT_FOUND",
            "没有选中任何 contacts2.db（分析器声明扩展名 db、sqlite）",
        )?;
        run.set_summary("未找到可分析的 contacts2.db");
        return Ok(());
    }

    let mut files = 0usize;
    let mut summaries = Vec::new();
    for file in run.inputs() {
        run.check_cancelled()?;
        files += 1;
        let display = display_name(file);
        if file.size() == 0 {
            emit_error(
                run,
                file,
                &display,
                "DB_ERROR",
                "文件为空（0 字节），不是有效的 SQLite 3 数据库",
            )?;
            continue;
        }
        if file.size() > MAX_DB_BYTES {
            emit_error(
                run,
                file,
                &display,
                "RESOURCE_LIMIT",
                &format!(
                    "文件大小 {} 字节超过本插件上限 {MAX_DB_BYTES} 字节，未物化",
                    file.size()
                ),
            )?;
            continue;
        }

        // rusqlite needs a path; the host copy is read-only and released on drop.
        let materialized = match file.materialize() {
            Ok(materialized) => materialized,
            Err(error) if error.is_cancelled() => return Err(error),
            Err(error) => {
                emit_error(
                    run,
                    file,
                    &display,
                    &error.code,
                    &format!("无法取得宿主物化副本：{}", error.message),
                )?;
                continue;
            }
        };
        let outcome = stats::analyze_path(materialized.path(), &|| run.is_cancelled());
        let stats = match outcome {
            Ok(stats) => stats,
            Err(stats::Failure::Cancelled) => return Err(lovelycarve_plugin::Error::cancelled()),
            Err(stats::Failure::Db(message)) => {
                emit_error(run, file, &display, "DB_ERROR", &message)?;
                continue;
            }
        };

        summaries.push(format!("{}: {}", display, status_text(&stats)));
        emit_stats(run, file, &display, &stats)?;
        if stats.table_name.is_none() {
            run.warn_file(
                file,
                "CALLS_TABLE_NOT_FOUND",
                format!("{display}: 未找到名为 calls 的表或视图"),
            )?;
        } else if stats.table_kind.as_deref() == Some("view") {
            run.warn_file(
                file,
                "CALLS_IS_VIEW",
                format!("{display}: calls 是视图，为避免执行检材内 SQL 未查询"),
            )?;
        }
    }

    run.set_summary(format!(
        "分析 {files} 个数据库；{}",
        if summaries.is_empty() {
            "没有可统计的 calls 表".to_owned()
        } else {
            summaries.join("；")
        }
    ));
    Ok(())
}

fn emit_stats(run: &Run, file: &FileRef, display: &str, stats: &Stats) -> Result<()> {
    let status = if stats.table_name.is_none() {
        "missing_table"
    } else if stats.table_kind.as_deref() == Some("view") {
        "view"
    } else if stats.total_rows == 0 {
        "empty_table"
    } else {
        "ok"
    };
    let date_min_utc = date_projection(stats.date_min, stats.date_unit);
    let date_max_utc = date_projection(stats.date_max, stats.date_unit);
    let present: Vec<String> = stats
        .columns
        .iter()
        .filter(|column| column.present)
        .map(|column| column.name.to_owned())
        .collect();
    let missing: Vec<String> = stats
        .columns
        .iter()
        .filter(|column| !column.present)
        .map(|column| column.name.to_owned())
        .collect();
    let record = CallsSummary {
        file: display.to_owned(),
        status: status.to_owned(),
        table: stats.table_name.clone(),
        table_kind: stats.table_kind.clone(),
        table_queried: stats.table_queried,
        total_rows: stats.total_rows,
        id_null: stats.id_null,
        id_non_integer: stats.id_non_integer,
        id_duplicate_values: stats.id_duplicate_values,
        distinct_rows: stats.distinct_rows,
        distinct_key: stats
            .distinct_key
            .iter()
            .map(|key| (*key).to_owned())
            .collect(),
        duplicate_rows: stats
            .distinct_rows
            .map(|distinct| (stats.total_rows - distinct).max(0)),
        number_count: stats.number_count,
        numbers_truncated: stats.numbers_truncated,
        type_count: stats.type_count,
        types_truncated: stats.types_truncated,
        date_min_raw: stats.date_min.map(RawNum::display),
        date_max_raw: stats.date_max.map(RawNum::display),
        date_min_utc,
        date_max_utc,
        date_unit: stats.date_unit.as_str().to_owned(),
        date_unit_basis: DATE_UNIT_BASIS.to_owned(),
        date_null: stats.date_null,
        date_non_numeric: stats.date_non_numeric,
        date_zero: stats.date_zero,
        date_negative: stats.date_negative,
        duration_total_raw: stats.duration_total,
        duration_null: stats.duration_null,
        duration_non_integer: stats.duration_non_integer,
        name_nonempty: stats.name_nonempty,
        cached_name_nonempty: stats.cached_name_nonempty,
        columns_present: present,
        columns_missing: missing,
        journal_hint: stats.journal_hint.clone(),
        objects_seen: stats.objects.clone(),
        objects_more: stats.objects_more,
        limitations: stats.limitations.clone(),
    };
    run.record(&record)
        .from(file)
        .note(format!("calls 表：{status}"))
        .emit()?;

    for column in &stats.columns {
        emit_column(run, file, display, column)?;
    }
    for number in &stats.numbers {
        emit_number(run, file, display, number)?;
    }
    for entry in &stats.types {
        emit_type(run, file, display, entry)?;
    }
    for (offset, limitation) in stats.limitations.iter().take(MAX_LIMITATIONS).enumerate() {
        let record = LimitationRecord {
            file: display.to_owned(),
            index: offset as i64 + 1,
            text: limitation.clone(),
        };
        run.record(&record)
            .from(file)
            .note(format!("限制 {}", offset + 1))
            .emit()?;
    }
    if stats.limitations.len() > MAX_LIMITATIONS {
        let record = LimitationRecord {
            file: display.to_owned(),
            index: MAX_LIMITATIONS as i64 + 1,
            text: format!(
                "另有 {} 条限制说明未展开。",
                stats.limitations.len() - MAX_LIMITATIONS
            ),
        };
        run.record(&record).from(file).emit()?;
    }
    for note in &stats.notes {
        let record = NoteRecord {
            file: display.to_owned(),
            title: note.title.clone(),
            level: match note.level {
                stats::NoteLevel::Info => "info",
                stats::NoteLevel::Low => "low",
                stats::NoteLevel::Medium => "medium",
            }
            .to_owned(),
            detail: note.detail.clone(),
        };
        run.record(&record).from(file).emit()?;
    }
    Ok(())
}

fn emit_column(run: &Run, file: &FileRef, display: &str, column: &ColumnInfo) -> Result<()> {
    let record = ColumnRecord {
        file: display.to_owned(),
        name: column.name.to_owned(),
        present: column.present,
        declared_type: column.declared_type.clone(),
        position: column.position,
    };
    run.record(&record)
        .from(file)
        .note(format!(
            "calls.{}（{}）",
            column.name,
            if column.present { "存在" } else { "缺失" }
        ))
        .emit()
}

fn emit_number(run: &Run, file: &FileRef, display: &str, number: &NumberStat) -> Result<()> {
    let record = NumberRecord {
        file: display.to_owned(),
        number: number.display.clone(),
        raw_type: number.raw_type.to_owned(),
        truncated: number.truncated,
        call_count: number.call_count,
        duration_total_raw: number.duration_total,
        duration_non_integer: number.duration_non_integer,
    };
    run.record(&record)
        .from(file)
        .note(format!("calls.number = {}", number.display))
        .emit()
}

fn emit_type(run: &Run, file: &FileRef, display: &str, entry: &TypeStat) -> Result<()> {
    let record = TypeRecord {
        file: display.to_owned(),
        type_raw: entry.display.clone(),
        raw_type: entry.raw_type.to_owned(),
        code: entry.code,
        label: entry.label.map(str::to_owned),
        call_count: entry.count,
    };
    run.record(&record)
        .from(file)
        .note(format!("calls.type = {}", entry.display))
        .emit()
}

fn emit_error(run: &Run, file: &FileRef, display: &str, code: &str, message: &str) -> Result<()> {
    run.warn_file(file, code, format!("{display}: {message}"))?;
    let record = ErrorRecord {
        file: display.to_owned(),
        code: code.to_owned(),
        message: message.to_owned(),
    };
    run.record(&record).from(file).emit()
}

fn date_projection(raw: Option<RawNum>, unit: DateUnit) -> Option<Timestamp> {
    let value = match raw? {
        RawNum::Int(value) => value,
        // A REAL epoch value is accepted only when it converts exactly enough
        // for a second/ms timestamp; fractions of a millisecond are dropped.
        RawNum::Real(value) if value.is_finite() => value as i64,
        RawNum::Real(_) => return None,
    };
    match unit {
        DateUnit::Seconds => Some(Timestamp::from_unix(value)),
        DateUnit::Milliseconds => Some(Timestamp::from_unix_millis(value)),
        DateUnit::Mixed | DateUnit::Unknown => None,
    }
}

fn status_text(stats: &Stats) -> String {
    if stats.table_name.is_none() {
        return "未找到 calls 表".to_owned();
    }
    if stats.table_kind.as_deref() == Some("view") {
        return "calls 是视图，未查询".to_owned();
    }
    if stats.total_rows == 0 {
        return "calls 表为空".to_owned();
    }
    let distinct = stats
        .distinct_rows
        .map(|value| format!("，去重后 {value} 行"))
        .unwrap_or_default();
    let numbers = match stats.number_count {
        Some(count) => format!("号码 {count} 个"),
        None => "number 列缺失".to_owned(),
    };
    let types = match stats.type_count {
        Some(count) => format!("type {count} 类"),
        None => "type 列缺失".to_owned(),
    };
    format!(
        "calls 共 {} 行{distinct}，{numbers}，{types}",
        stats.total_rows
    )
}

fn display_name(file: &FileRef) -> String {
    let name = file.name().trim();
    if !name.is_empty() {
        return name.replace('\\', "/");
    }
    file.view_path()
        .or_else(|| file.root_path())
        .map(|path| path.replace('\\', "/"))
        .unwrap_or_else(|| format!("file:{}", file.id()))
}

fn plugin() -> Plugin {
    Plugin::new("org.ljnljn.android-contacts2")
        .record::<CallsSummary>()
        .record::<ColumnRecord>()
        .record::<NumberRecord>()
        .record::<TypeRecord>()
        .record::<NoteRecord>()
        .record::<LimitationRecord>()
        .record::<ErrorRecord>()
        .analyzer(ANALYZER, contacts2)
}

export_plugin!(plugin);
