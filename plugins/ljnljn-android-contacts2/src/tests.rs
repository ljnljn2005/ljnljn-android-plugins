//! 单元测试：全部使用临时目录里的合成 `contacts2.db`，不读取真实案件数据。
//!
//! 覆盖：正常表与重复记录、空表、缺列、缺表、坏文件/空文件、截断损坏、
//! 秒/毫秒/mixed/unknown 时间数量级、非数值与 BLOB/NULL 取值、calls 为视图、
//! 表名/列名大小写、结果树与属性契约、号码明细截断、取消。

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use lovelycarve_plugin::{AnalysisResult, Artifact, FileAnalyzer, Value};
use rusqlite::Connection;

use crate::render::{self, ART_COLUMN, ART_LIMITATION, ART_NUMBER, ART_SUMMARY, ART_TYPE};
use crate::stats::{self, DateUnit, Failure, Stats};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    root: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ljnljn-contacts2-{tag}-{}-{nanos}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("创建测试临时目录");
        Self { root }
    }

    fn db(&self) -> PathBuf {
        self.root.join("contacts2.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Windows 上仍有打开的连接时删除会失败；测试里连接都放在独立作用域内。
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn exec(dir: &TempDir, sql: &str) {
    let connection = Connection::open(dir.db()).expect("打开合成数据库");
    connection.execute_batch(sql).expect("执行合成 SQL");
}

fn analyze(dir: &TempDir) -> Stats {
    stats::analyze_path(&dir.db(), &|| false).expect("分析合成数据库")
}

fn analyze_error(dir: &TempDir) -> String {
    match stats::analyze_path(&dir.db(), &|| false) {
        Err(Failure::Db(message)) => message,
        Err(Failure::Cancelled) => panic!("未请求取消却返回 Cancelled"),
        Ok(stats) => panic!("预期数据库错误，实际得到 {} 行", stats.total_rows),
    }
}

fn build(stats: &Stats) -> AnalysisResult {
    render::build_result(stats, "contacts2.db", "file-1")
}

fn of_type<'a>(result: &'a AnalysisResult, artifact_type: &str) -> Vec<&'a Artifact> {
    result
        .artifacts
        .iter()
        .filter(|artifact| artifact.artifact_type == artifact_type)
        .collect()
}

fn only<'a>(result: &'a AnalysisResult, artifact_type: &str) -> &'a Artifact {
    let matches = of_type(result, artifact_type);
    assert_eq!(matches.len(), 1, "{artifact_type} artifact 数量");
    matches[0]
}

fn attr<'a>(artifact: &'a Artifact, key: &str) -> &'a Value {
    artifact
        .attributes
        .get(key)
        .unwrap_or_else(|| panic!("缺少属性 {key}"))
}

fn attr_i64(artifact: &Artifact, key: &str) -> i64 {
    attr(artifact, key)
        .as_i64()
        .unwrap_or_else(|| panic!("属性 {key} 不是 i64：{:?}", attr(artifact, key)))
}

fn attr_str<'a>(artifact: &'a Artifact, key: &str) -> &'a str {
    attr(artifact, key)
        .as_str()
        .unwrap_or_else(|| panic!("属性 {key} 不是字符串"))
}

fn attr_bool(artifact: &Artifact, key: &str) -> bool {
    attr(artifact, key)
        .as_bool()
        .unwrap_or_else(|| panic!("属性 {key} 不是 bool"))
}

const FULL_SCHEMA: &str = "CREATE TABLE calls (\
    _id INTEGER PRIMARY KEY, number TEXT, date INTEGER, duration INTEGER, \
    type INTEGER, name TEXT, cached_name TEXT)";

fn full_database(dir: &TempDir) {
    exec(
        dir,
        &format!(
            "{FULL_SCHEMA};
             INSERT INTO calls VALUES
               (1, '13800138000', 1700000000000, 60, 1, 'Alice', 'Alice'),
               (2, '13900139000', 1700000060000, 30, 2, 'Bob', 'Bob'),
               (3, '13800138000', 1700000120000, 0, 3, NULL, 'Alice'),
               (4, '13800138000', 1700000000000, 60, 1, 'Alice', 'Alice'),
               (5, '13800138000', 1700000180000, 600, 5, 'Alice', 'Alice');"
        ),
    );
}

#[test]
fn normal_database_reports_rows_dedup_numbers_types_and_date_unit() {
    let dir = TempDir::new("normal");
    full_database(&dir);
    let stats = analyze(&dir);

    assert_eq!(stats.table_name.as_deref(), Some("calls"));
    assert_eq!(stats.table_kind.as_deref(), Some("table"));
    assert!(stats.table_queried);
    assert_eq!(stats.total_rows, 5);
    assert_eq!(stats.id_null, Some(0));
    assert_eq!(stats.id_non_integer, Some(0));
    assert_eq!(stats.id_duplicate_values, Some(0));
    assert_eq!(stats.distinct_rows, Some(4));
    assert_eq!(
        stats.distinct_key,
        vec!["number", "date", "type", "duration"]
    );
    assert_eq!(stats.number_count, Some(2));
    assert_eq!(stats.numbers.len(), 2);

    // 按通话次数降序，13800138000 有 4 通（含 1 条完全重复行）。
    let top = &stats.numbers[0];
    assert_eq!(top.display, "13800138000");
    assert_eq!(top.raw_type, "text");
    assert_eq!(top.call_count, 4);
    assert_eq!(top.duration_total, Some(720));
    assert_eq!(top.duration_non_integer, Some(0));
    assert_eq!(stats.numbers[1].display, "13900139000");
    assert_eq!(stats.numbers[1].call_count, 1);
    assert_eq!(stats.numbers[1].duration_total, Some(30));

    assert_eq!(stats.type_count, Some(4));
    let labels: Vec<_> = stats.types.iter().map(|entry| entry.label).collect();
    assert!(labels.contains(&Some("来电（INCOMING）")));
    assert!(labels.contains(&Some("去电（OUTGOING）")));
    assert!(labels.contains(&Some("未接（MISSED）")));
    assert!(labels.contains(&Some("拒接（REJECTED）")));

    assert_eq!(stats.date_min.unwrap().display(), "1700000000000");
    assert_eq!(stats.date_max.unwrap().display(), "1700000180000");
    assert_eq!(stats.date_unit, DateUnit::Milliseconds);
    assert_eq!(stats.date_null, Some(0));
    assert_eq!(stats.date_non_numeric, Some(0));
    assert_eq!(stats.duration_total, Some(750));
    assert_eq!(stats.name_nonempty, Some(4));
    assert_eq!(stats.cached_name_nonempty, Some(5));

    let result = build(&stats);
    let summary = result.summary.as_deref().expect("summary");
    assert!(summary.contains("共 5 行"), "summary={summary}");
    assert!(summary.contains("去重后 4 行"), "summary={summary}");
    assert!(summary.contains("重复 1"), "summary={summary}");
    assert!(summary.contains("毫秒"), "summary={summary}");
    assert!(result
        .findings
        .iter()
        .any(|finding| finding.title.contains("重复")));

    let overview = only(&result, ART_SUMMARY);
    assert_eq!(attr_i64(overview, "calls.total_rows"), 5);
    assert_eq!(attr_i64(overview, "calls.distinct_rows"), 4);
    assert_eq!(attr_i64(overview, "calls.duplicate_rows"), 1);
    assert_eq!(attr_i64(overview, "calls.number_count"), 2);
    assert_eq!(attr_i64(overview, "calls.type_count"), 4);
    assert_eq!(attr_str(overview, "calls.date_unit"), "milliseconds");
    assert!(attr_str(overview, "calls.date_unit_basis").contains("1e11"));
    assert_eq!(attr_i64(overview, "calls.date_min_raw"), 1700000000000);
    assert_eq!(attr_i64(overview, "calls.date_max_raw"), 1700000180000);
    assert_eq!(attr_i64(overview, "calls.duration_total_raw"), 750);
    assert_eq!(attr_str(overview, "calls.columns_missing"), "");

    let numbers = of_type(&result, ART_NUMBER);
    assert_eq!(numbers.len(), 2);
    assert_eq!(attr_str(numbers[0], "calls.number"), "13800138000");
    assert_eq!(attr_i64(numbers[0], "calls.call_count"), 4);
    assert_eq!(attr_i64(numbers[0], "calls.duration_total_raw"), 720);

    let types = of_type(&result, ART_TYPE);
    assert_eq!(types.len(), 4);
    let incoming = types
        .iter()
        .find(|artifact| attr_str(artifact, "calls.type_raw") == "1")
        .expect("type=1 artifact");
    assert_eq!(attr_i64(incoming, "calls.type_code"), 1);
    assert_eq!(attr_str(incoming, "calls.type_label"), "来电（INCOMING）");
    assert_eq!(attr_i64(incoming, "calls.call_count"), 2);
}

#[test]
fn empty_table_is_zero_result_with_limitations() {
    let dir = TempDir::new("empty");
    exec(&dir, FULL_SCHEMA);
    let stats = analyze(&dir);

    assert_eq!(stats.total_rows, 0);
    assert_eq!(stats.distinct_rows, Some(0));
    assert_eq!(stats.number_count, Some(0));
    assert_eq!(stats.type_count, Some(0));
    assert!(stats.numbers.is_empty());
    assert!(stats.types.is_empty());
    assert_eq!(stats.date_unit, DateUnit::Unknown);
    assert!(stats.date_min.is_none());
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("为空（0 行）")));
    assert!(stats.notes.iter().any(|note| note.title.contains("为空")));

    let result = build(&stats);
    assert!(result
        .summary
        .as_deref()
        .unwrap_or_default()
        .contains("为空"));
    assert!(of_type(&result, ART_NUMBER).is_empty());
    assert!(of_type(&result, ART_TYPE).is_empty());
    // 列检查仍然完整输出 7 列。
    assert_eq!(of_type(&result, ART_COLUMN).len(), 7);
}

#[test]
fn missing_columns_are_reported_and_available_columns_still_queried() {
    let dir = TempDir::new("missing-cols");
    exec(
        &dir,
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, number TEXT, duration INTEGER);
         INSERT INTO calls VALUES (1, '100', 10), (2, '100', 20), (3, NULL, 5);",
    );
    let stats = analyze(&dir);

    assert_eq!(stats.total_rows, 3);
    assert_eq!(stats.distinct_key, vec!["number", "duration"]);
    assert_eq!(stats.distinct_rows, Some(3));
    assert_eq!(stats.number_count, Some(2));
    assert_eq!(stats.numbers[0].display, "100");
    assert_eq!(stats.numbers[0].call_count, 2);
    assert_eq!(stats.numbers[0].duration_total, Some(30));
    assert_eq!(stats.numbers[1].display, "(NULL)");
    assert_eq!(stats.numbers[1].raw_type, "null");
    assert_eq!(stats.type_count, None);
    assert!(stats.types.is_empty());
    assert!(stats.date_min.is_none());
    assert_eq!(stats.date_unit, DateUnit::Unknown);
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("date 列缺失")));
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("type 列缺失")));
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("缺少预期列")));
    assert!(stats
        .notes
        .iter()
        .any(|note| note.title.contains("缺少关键列")));

    let result = build(&stats);
    let overview = only(&result, ART_SUMMARY);
    assert_eq!(attr_str(overview, "calls.distinct_key"), "number,duration");
    assert_eq!(
        attr_str(overview, "calls.columns_present"),
        "_id,number,duration"
    );
    assert_eq!(
        attr_str(overview, "calls.columns_missing"),
        "date,type,name,cached_name"
    );
    let columns = of_type(&result, ART_COLUMN);
    assert_eq!(columns.len(), 7);
    let date = columns
        .iter()
        .find(|artifact| attr_str(artifact, "calls.column_name") == "date")
        .expect("date column artifact");
    assert!(!attr_bool(date, "calls.column_present"));
}

#[test]
fn id_integrity_counts_are_reported() {
    let dir = TempDir::new("id-integrity");
    exec(
        &dir,
        "CREATE TABLE calls (_id, number);
         INSERT INTO calls VALUES (1, 'a'), (1, 'b'), (NULL, 'c'), ('x', 'd');",
    );
    let stats = analyze(&dir);
    assert_eq!(stats.total_rows, 4);
    assert_eq!(stats.id_null, Some(1));
    assert_eq!(stats.id_non_integer, Some(1));
    assert_eq!(stats.id_duplicate_values, Some(1));
    assert!(stats
        .notes
        .iter()
        .any(|note| note.title.contains("_id 存在重复取值")));

    let result = build(&stats);
    let overview = only(&result, ART_SUMMARY);
    assert_eq!(attr_i64(overview, "calls.id_null"), 1);
    assert_eq!(attr_i64(overview, "calls.id_non_integer"), 1);
    assert_eq!(attr_i64(overview, "calls.id_duplicate_values"), 1);
}

#[test]
fn missing_calls_table_is_zero_result_not_error() {
    let dir = TempDir::new("no-calls");
    exec(
        &dir,
        "CREATE TABLE contacts (_id INTEGER PRIMARY KEY, display_name TEXT);
         CREATE TABLE raw_events (id INTEGER, payload TEXT);
         INSERT INTO contacts VALUES (1, 'Alice');",
    );
    let stats = analyze(&dir);

    assert!(stats.table_name.is_none());
    assert!(stats.table_kind.is_none());
    assert!(!stats.table_queried);
    assert_eq!(stats.total_rows, 0);
    assert_eq!(stats.objects, vec!["table:contacts", "table:raw_events"]);
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("未找到名为 calls 的表")));
    assert!(stats
        .notes
        .iter()
        .any(|note| note.title.contains("未找到 calls 表")));

    let result = build(&stats);
    assert!(result
        .summary
        .as_deref()
        .unwrap_or_default()
        .contains("未找到 calls 表"));
    assert!(result
        .findings
        .iter()
        .any(|finding| finding.severity == lovelycarve_plugin::Severity::Medium));
    assert!(of_type(&result, ART_NUMBER).is_empty());
    let overview = only(&result, ART_SUMMARY);
    assert!(attr_str(overview, "calls.objects_seen").contains("contacts"));
}

#[test]
fn non_sqlite_and_empty_files_return_explicit_db_errors() {
    let garbage = TempDir::new("garbage");
    fs::write(garbage.db(), b"this is definitely not a sqlite database").expect("写坏文件");
    let message = analyze_error(&garbage);
    assert!(message.contains("SQLite 3"), "message={message}");
    assert!(message.contains("SQLCipher"), "message={message}");

    let empty = TempDir::new("empty-file");
    fs::write(empty.db(), b"").expect("写空文件");
    let message = analyze_error(&empty);
    assert!(message.contains("0 字节"), "message={message}");
}

#[test]
fn truncated_or_corrupted_database_returns_error_without_panic() {
    let dir = TempDir::new("corrupt");
    full_database(&dir);
    let path = dir.db();
    let mut bytes = fs::read(&path).expect("读取合成库");
    assert!(bytes.len() > 4096);
    // 保留文件头与第一页大小信息，清零其后的页内容：模拟损坏的数据库。
    for byte in &mut bytes[100..] {
        *byte = 0;
    }
    fs::write(&path, bytes).expect("写损坏库");
    let message = analyze_error(&dir);
    assert!(!message.is_empty(), "错误消息不应为空");
}

#[test]
fn cancellation_is_reported_before_any_query() {
    let dir = TempDir::new("cancel");
    full_database(&dir);
    match stats::analyze_path(&dir.db(), &|| true) {
        Err(Failure::Cancelled) => {}
        other => panic!("预期 Cancelled，实际 {other:?}"),
    }
}

#[test]
fn date_seconds_milliseconds_mixed_and_unknown_units() {
    let seconds = TempDir::new("date-seconds");
    exec(
        &seconds,
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, date INTEGER);
         INSERT INTO calls VALUES (1, 1700000000), (2, 1700000100);",
    );
    let stats = analyze(&seconds);
    assert_eq!(stats.date_unit, DateUnit::Seconds);
    assert_eq!(stats.date_min.unwrap().display(), "1700000000");
    assert_eq!(stats.date_max.unwrap().display(), "1700000100");

    let mixed = TempDir::new("date-mixed");
    exec(
        &mixed,
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, date INTEGER);
         INSERT INTO calls VALUES (1, 1700000000), (2, 1700000000000);",
    );
    let stats = analyze(&mixed);
    assert_eq!(stats.date_unit, DateUnit::Mixed);
    assert!(stats
        .notes
        .iter()
        .any(|note| note.title.contains("数量级不一致")));
    let result = build(&stats);
    assert_eq!(
        attr_str(only(&result, ART_SUMMARY), "calls.date_unit"),
        "mixed"
    );

    let unknown = TempDir::new("date-unknown");
    exec(
        &unknown,
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, date INTEGER);
         INSERT INTO calls VALUES (1, 100), (2, 200);",
    );
    let stats = analyze(&unknown);
    assert_eq!(stats.date_unit, DateUnit::Unknown);
    assert!(stats
        .notes
        .iter()
        .any(|note| note.title.contains("无法判定")));

    // 0 端点不参与数量级判定，但仍保留在原始范围内。
    let zero_endpoint = TempDir::new("date-zero");
    exec(
        &zero_endpoint,
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, date INTEGER);
         INSERT INTO calls VALUES (1, 0), (2, 1700000000000);",
    );
    let stats = analyze(&zero_endpoint);
    assert_eq!(stats.date_unit, DateUnit::Milliseconds);
    assert_eq!(stats.date_min.unwrap().display(), "0");
    assert_eq!(stats.date_zero, Some(1));
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("端点含 0 或过小值")));
}

#[test]
fn non_numeric_null_and_blob_values_do_not_panic() {
    let dir = TempDir::new("weird-values");
    exec(
        &dir,
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, number, date, duration, type, name, cached_name);
         INSERT INTO calls VALUES
           (1, NULL, 'not-a-date', 'abc', 'incoming', '', ''),
           (2, x'0102', 1700000000000, 30, 99, 'Alice', 'Alice');",
    );
    let stats = analyze(&dir);

    assert_eq!(stats.total_rows, 2);
    assert_eq!(stats.distinct_rows, Some(2));
    assert_eq!(stats.number_count, Some(2));
    assert!(stats
        .numbers
        .iter()
        .any(|number| number.display == "(NULL)" && number.raw_type == "null"));
    assert!(stats
        .numbers
        .iter()
        .any(|number| number.display.starts_with("blob:0x0102") && number.raw_type == "blob"));

    assert_eq!(stats.date_min.unwrap().display(), "1700000000000");
    assert_eq!(stats.date_max.unwrap().display(), "1700000000000");
    assert_eq!(stats.date_non_numeric, Some(1));
    assert_eq!(stats.date_unit, DateUnit::Milliseconds);
    assert_eq!(stats.duration_total, Some(30));
    assert_eq!(stats.duration_non_integer, Some(1));
    assert_eq!(stats.name_nonempty, Some(1));
    assert_eq!(stats.cached_name_nonempty, Some(1));
    assert!(stats.limitations.iter().any(|line| line.contains("非整型")));
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("未映射的 type")));

    let result = build(&stats);
    let types = of_type(&result, ART_TYPE);
    assert_eq!(types.len(), 2);
    let numeric = types
        .iter()
        .find(|artifact| attr_str(artifact, "calls.type_raw") == "99")
        .expect("type=99 artifact");
    assert_eq!(attr_i64(numeric, "calls.type_code"), 99);
    assert!(matches!(attr(numeric, "calls.type_label"), Value::Nil));
}

#[test]
fn calls_as_view_is_not_queried() {
    let dir = TempDir::new("view");
    exec(
        &dir,
        &format!(
            "{FULL_SCHEMA};
             INSERT INTO calls VALUES (1, '100', 1700000000000, 10, 1, 'A', 'A');
             ALTER TABLE calls RENAME TO base_calls;
             CREATE VIEW calls AS SELECT * FROM base_calls;"
        ),
    );
    let stats = analyze(&dir);

    assert_eq!(stats.table_name.as_deref(), Some("calls"));
    assert_eq!(stats.table_kind.as_deref(), Some("view"));
    assert!(!stats.table_queried);
    assert_eq!(stats.total_rows, 0);
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("视图（view）")));
    assert!(stats
        .notes
        .iter()
        .any(|note| note.title.contains("calls 是视图")));

    let result = build(&stats);
    assert!(result
        .summary
        .as_deref()
        .unwrap_or_default()
        .contains("视图"));
    assert!(of_type(&result, ART_NUMBER).is_empty());
}

#[test]
fn table_and_column_names_match_case_insensitively() {
    let dir = TempDir::new("case");
    exec(
        &dir,
        "CREATE TABLE Calls (\
           _id INTEGER PRIMARY KEY, Number TEXT, Date INTEGER, Duration INTEGER, \
           Type INTEGER, Name TEXT, Cached_Name TEXT);
         INSERT INTO Calls VALUES (1, '10086', 1700000000000, 42, 2, 'Carol', 'Carol');",
    );
    let stats = analyze(&dir);

    assert_eq!(stats.table_name.as_deref(), Some("Calls"));
    assert_eq!(stats.total_rows, 1);
    assert_eq!(stats.number_count, Some(1));
    assert_eq!(stats.numbers[0].display, "10086");
    assert_eq!(stats.numbers[0].duration_total, Some(42));
    assert_eq!(stats.type_count, Some(1));
    assert_eq!(stats.date_unit, DateUnit::Milliseconds);
    assert_eq!(stats.name_nonempty, Some(1));
    assert!(stats.columns.iter().all(|column| column.present));
}

#[test]
fn result_tree_and_attributes_follow_the_lpp_contract() {
    let dir = TempDir::new("contract");
    full_database(&dir);
    let stats = analyze(&dir);
    let result = build(&stats);

    assert!(!result.summary.as_deref().unwrap_or_default().is_empty());
    assert!(result.timeline.is_empty());
    assert_eq!(result.nodes.len(), 5);
    let node_types: Vec<&str> = result
        .nodes
        .iter()
        .map(|node| node.node_type.as_str())
        .collect();
    assert_eq!(
        node_types,
        vec![
            ART_SUMMARY,
            ART_COLUMN,
            ART_NUMBER,
            ART_TYPE,
            ART_LIMITATION
        ]
    );

    for artifact in &result.artifacts {
        assert!(
            node_types.contains(&artifact.artifact_type.as_str()),
            "artifact type {} 没有对应结果树节点",
            artifact.artifact_type
        );
        for key in ["source.path", "source.file_id", "source.root"] {
            assert!(
                artifact.attributes.contains_key(key),
                "{} 缺少 {key}",
                artifact.artifact_type
            );
        }
        for (key, value) in artifact.attributes.iter() {
            assert!(
                key.chars().all(|ch| ch.is_ascii_lowercase()
                    || ch.is_ascii_digit()
                    || ch == '.'
                    || ch == '_'),
                "属性键不是小写命名空间：{key}"
            );
            assert!(key.contains('.'), "属性键缺少命名空间：{key}");
            if let Some(number) = value.as_f64() {
                assert!(number.is_finite(), "属性 {key} 是非有限浮点");
            }
            assert!(
                !matches!(value, Value::Binary(_)),
                "属性 {key} 不应是二进制"
            );
        }
    }

    for node in &result.nodes {
        let artifacts = of_type(&result, &node.node_type);
        if artifacts.is_empty() {
            continue;
        }
        for column in &node.columns {
            assert!(
                artifacts
                    .iter()
                    .any(|artifact| artifact.attributes.contains_key(&column.key)),
                "节点 {} 的列 {} 没有绑定任何 artifact 属性",
                node.node_type,
                column.key
            );
        }
    }

    assert!(result.artifacts.len() < 10_000);
    assert!(!result.findings.is_empty());

    // 结果可被 MessagePack 编码/解码（宿主 LPP v1 的传输格式）。
    let encoded = lovelycarve_plugin::value::encode(&result.to_value()).expect("编码结果");
    let decoded = lovelycarve_plugin::value::decode(&encoded).expect("解码结果");
    assert!(decoded.as_map().is_some());
}

#[test]
fn number_detail_is_capped_and_still_reports_the_full_group_count() {
    let dir = TempDir::new("many-numbers");
    {
        let connection = Connection::open(dir.db()).expect("打开合成数据库");
        connection
            .execute_batch(
                "CREATE TABLE calls (_id INTEGER PRIMARY KEY, number TEXT, date INTEGER, \
                 duration INTEGER, type INTEGER, name TEXT, cached_name TEXT);",
            )
            .expect("建表");
        let transaction = connection.unchecked_transaction().expect("事务");
        {
            let mut statement = transaction
                .prepare("INSERT INTO calls (number, date, duration, type) VALUES (?1, ?2, ?3, 1)")
                .expect("准备插入");
            for index in 0..(stats::MAX_NUMBER_ROWS + 1) {
                statement
                    .execute(rusqlite::params![
                        format!("num-{index}"),
                        1700000000000_i64,
                        1_i64
                    ])
                    .expect("插入号码");
            }
        }
        transaction.commit().expect("提交");
    }

    let stats = analyze(&dir);
    assert_eq!(
        stats.number_count,
        Some((stats::MAX_NUMBER_ROWS + 1) as i64)
    );
    assert_eq!(stats.numbers.len(), stats::MAX_NUMBER_ROWS);
    assert!(stats.numbers_truncated);
    assert!(stats
        .limitations
        .iter()
        .any(|line| line.contains("号码分组超过")));

    let result = build(&stats);
    assert_eq!(of_type(&result, ART_NUMBER).len(), stats::MAX_NUMBER_ROWS);
    assert!(result.artifacts.len() < 10_000);
}

#[test]
fn analyzer_materializes_the_file_through_the_sdk_abi() {
    let dir = TempDir::new("host");
    full_database(&dir);
    let raw = crate::test_host::analyze_via_host(&dir.db());

    let summary = field_str(&raw, "summary").expect("summary");
    assert!(summary.contains("共 5 行"), "summary={summary}");
    let artifacts = field(&raw, "artifacts")
        .and_then(Value::as_array)
        .expect("artifacts");
    assert!(!artifacts.is_empty());
    assert!(artifacts
        .iter()
        .any(|artifact| field_str(artifact, "type") == Some(ART_SUMMARY)));
    let nodes = field(&raw, "nodes")
        .and_then(Value::as_array)
        .expect("nodes");
    assert_eq!(nodes.len(), 5);
    let findings = field(&raw, "findings")
        .and_then(Value::as_array)
        .expect("findings");
    assert!(!findings.is_empty());
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_map()?.iter().find_map(|(entry_key, entry_value)| {
        (entry_key.as_str() == Some(key)).then_some(entry_value)
    })
}

fn field_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    field(value, key).and_then(Value::as_str)
}

#[test]
fn identity_and_manifest_stay_in_sync() {
    let manifest = include_str!("../plugin.toml");
    let id_line = format!("id = \"{}\"", crate::PLUGIN_ID);
    let name_line = format!("name = \"{}\"", crate::PLUGIN_NAME);
    let version_line = format!("version = \"{}\"", crate::PLUGIN_VERSION);
    let analyzer_line = format!("analyzer = \"{}\"", crate::ANALYZER_FILE);
    assert!(manifest.contains(id_line.as_str()));
    assert!(manifest.contains(name_line.as_str()));
    assert!(manifest.contains(version_line.as_str()));
    assert!(manifest.contains(analyzer_line.as_str()));
    assert!(manifest.contains("isolation = \"process\""));
    assert!(manifest.contains("permissions = [\"emit_artifacts\", \"file_materialize\"]"));
    assert!(manifest.contains("type = \"file_analyzer\""));
    assert!(manifest.contains("extensions = [\"db\", \"sqlite\"]"));

    let analyzer = crate::analyzer::Contacts2Calls;
    assert_eq!(analyzer.analyzer(), crate::ANALYZER_FILE);
    assert_eq!(crate::PLUGIN_VERSION, "0.1.0");
}
