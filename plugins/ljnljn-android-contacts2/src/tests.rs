//! Synthetic `contacts2.db` tests. Every database is created in a temporary
//! directory with `rusqlite`; no real case data is read.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lovelycarve_plugin::testing::{Fixture, Harness, TestRecord, TestRun};
use rusqlite::Connection;

const FULL_SCHEMA: &str = "CREATE TABLE calls (\
    _id INTEGER PRIMARY KEY, number TEXT, date INTEGER, duration INTEGER, \
    type INTEGER, name TEXT, cached_name TEXT)";

struct TempDir {
    root: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "ljnljn-contacts2-v2-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn db(&self) -> PathBuf {
        self.root.join("contacts2.db")
    }

    fn exec(&self, sql: &str) {
        let connection = Connection::open(self.db()).expect("open synthetic database");
        connection
            .execute_batch(sql)
            .expect("execute synthetic SQL");
        // The connection is dropped here so the host can copy the file.
    }

    fn write(&self, bytes: &[u8]) -> PathBuf {
        fs::write(self.db(), bytes).unwrap();
        self.db()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn harness() -> Harness {
    Harness::new(super::plugin)
}

fn run_db(path: &Path) -> TestRun {
    harness()
        .run_with(
            "contacts2_calls",
            Fixture::files(&[path]),
            Default::default(),
            Default::default(),
        )
        .expect("run")
}

fn full_database(dir: &TempDir) {
    dir.exec(&format!(
        "{FULL_SCHEMA};
         INSERT INTO calls VALUES
           (1, '13800138000', 1700000000000, 60, 1, 'Alice', 'Alice'),
           (2, '13900139000', 1700000060000, 30, 2, 'Bob', 'Bob'),
           (3, '13800138000', 1700000120000, 0, 3, NULL, 'Alice'),
           (4, '13800138000', 1700000000000, 60, 1, 'Alice', 'Alice'),
           (5, '13800138000', 1700000180000, 600, 5, 'Alice', 'Alice');"
    ));
}

fn only<'a>(run: &'a TestRun, record_type: &str) -> &'a TestRecord {
    let records = run.of_type(record_type);
    assert_eq!(records.len(), 1, "{record_type} count");
    records[0]
}

#[test]
fn normal_database_reports_dedup_numbers_types_and_utc_projection() {
    let dir = TempDir::new("normal");
    full_database(&dir);
    let run = run_db(&dir.db());

    let summary = only(&run, "android.contacts2.summary");
    assert_eq!(summary.text("status"), Some("ok"));
    assert_eq!(summary.text("table"), Some("calls"));
    assert_eq!(summary.text("table_kind"), Some("table"));
    assert_eq!(
        summary.get("table_queried").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(summary.int("total_rows"), Some(5));
    assert_eq!(summary.int("distinct_rows"), Some(4));
    assert_eq!(summary.int("duplicate_rows"), Some(1));
    assert_eq!(summary.int("number_count"), Some(2));
    assert_eq!(summary.int("type_count"), Some(4));
    assert_eq!(summary.text("date_unit"), Some("milliseconds"));
    assert_eq!(summary.text("date_min_raw"), Some("1700000000000"));
    assert_eq!(summary.text("date_max_raw"), Some("1700000180000"));
    assert_eq!(summary.text("date_min_utc"), Some("2023-11-14T22:13:20Z"));
    assert_eq!(summary.text("date_max_utc"), Some("2023-11-14T22:16:20Z"));
    assert_eq!(summary.int("duration_total_raw"), Some(750));
    assert_eq!(summary.int("name_nonempty"), Some(4));
    assert_eq!(summary.int("cached_name_nonempty"), Some(5));

    let key: Vec<&str> = summary
        .get("distinct_key")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(key, vec!["number", "date", "type", "duration"]);

    assert_eq!(run.count("android.contacts2.column"), 7);
    assert_eq!(run.count("android.contacts2.number"), 2);
    assert_eq!(run.count("android.contacts2.type"), 4);
    assert!(run.count("android.contacts2.limitation") > 0);

    let top = run
        .of_type("android.contacts2.number")
        .into_iter()
        .find(|record| record.text("number") == Some("13800138000"))
        .expect("top number");
    assert_eq!(top.int("call_count"), Some(4));
    assert_eq!(top.int("duration_total_raw"), Some(720));
    assert_eq!(top.text("raw_type"), Some("text"));
    assert!(top.sources[0]
        .note
        .as_deref()
        .unwrap_or("")
        .contains("calls.number"));

    let incoming = run
        .of_type("android.contacts2.type")
        .into_iter()
        .find(|record| record.text("type_raw") == Some("1"))
        .expect("incoming type");
    assert_eq!(incoming.int("code"), Some(1));
    assert_eq!(incoming.text("label"), Some("来电（INCOMING）"));
    assert_eq!(incoming.int("call_count"), Some(2));

    assert!(run
        .summary
        .as_deref()
        .unwrap_or("")
        .contains("calls 共 5 行"));
}

#[test]
fn missing_calls_table_is_an_explicit_status_with_warning() {
    let dir = TempDir::new("missing");
    dir.exec("CREATE TABLE contacts (_id INTEGER PRIMARY KEY, name TEXT)");
    let run = run_db(&dir.db());

    let summary = only(&run, "android.contacts2.summary");
    assert_eq!(summary.text("status"), Some("missing_table"));
    assert_eq!(summary.get("table").and_then(|v| v.as_str()), None);
    assert_eq!(summary.int("total_rows"), Some(0));
    let objects: Vec<&str> = summary
        .get("objects_seen")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(objects.contains(&"table:contacts"));
    assert!(run
        .warnings
        .iter()
        .any(|(code, _, _)| code == "CALLS_TABLE_NOT_FOUND"));
    assert!(run
        .of_type("android.contacts2.note")
        .iter()
        .any(|record| record.text("title") == Some("未找到 calls 表")));
}

#[test]
fn view_calls_is_not_queried() {
    let dir = TempDir::new("view");
    dir.exec(
        "CREATE TABLE raw_calls (_id INTEGER PRIMARY KEY, number TEXT, date INTEGER, duration INTEGER, type INTEGER);\
         CREATE VIEW calls AS SELECT _id, number, date, duration, type FROM raw_calls;\
         INSERT INTO raw_calls VALUES (1, '10086', 1700000000000, 5, 2);",
    );
    let run = run_db(&dir.db());

    let summary = only(&run, "android.contacts2.summary");
    assert_eq!(summary.text("status"), Some("view"));
    assert_eq!(summary.text("table_kind"), Some("view"));
    assert_eq!(
        summary.get("table_queried").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(summary.int("total_rows"), Some(0));
    assert!(run
        .warnings
        .iter()
        .any(|(code, _, _)| code == "CALLS_IS_VIEW"));
}

#[test]
fn empty_calls_table_is_reported() {
    let dir = TempDir::new("empty");
    dir.exec(FULL_SCHEMA);
    let run = run_db(&dir.db());

    let summary = only(&run, "android.contacts2.summary");
    assert_eq!(summary.text("status"), Some("empty_table"));
    assert_eq!(summary.int("total_rows"), Some(0));
    assert_eq!(summary.int("distinct_rows"), Some(0));
    assert_eq!(run.count("android.contacts2.column"), 7);
    assert_eq!(run.count("android.contacts2.number"), 0);
}

#[test]
fn missing_columns_shorten_the_dedup_key() {
    let dir = TempDir::new("partial");
    dir.exec(
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, number TEXT, date INTEGER);\
         INSERT INTO calls VALUES (1, '13800138000', 1700000000000),\
                                  (2, '13800138000', 1700000000000),\
                                  (3, '13900139000', 1700000060000);",
    );
    let run = run_db(&dir.db());

    let summary = only(&run, "android.contacts2.summary");
    assert_eq!(summary.text("date_unit"), Some("milliseconds"));
    assert_eq!(summary.int("total_rows"), Some(3));
    assert_eq!(summary.int("distinct_rows"), Some(2));
    assert_eq!(summary.int("duplicate_rows"), Some(1));
    let key: Vec<&str> = summary
        .get("distinct_key")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(key, vec!["number", "date"]);
    let missing: Vec<&str> = summary
        .get("columns_missing")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(missing.contains(&"duration"));
    assert!(missing.contains(&"type"));
}

#[test]
fn seconds_and_mixed_date_units_are_classified_without_guessing() {
    let dir = TempDir::new("units");
    dir.exec(
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, number TEXT, date INTEGER);\
         INSERT INTO calls VALUES (1, '1', 1700000000), (2, '2', 1700000000000);",
    );
    let run = run_db(&dir.db());
    let summary = only(&run, "android.contacts2.summary");
    assert_eq!(summary.text("date_unit"), Some("mixed"));
    // Mixed endpoints are not projected into absolute time.
    assert_eq!(summary.get("date_min_utc").and_then(|v| v.as_str()), None);
    assert_eq!(summary.get("date_max_utc").and_then(|v| v.as_str()), None);

    let dir = TempDir::new("seconds");
    dir.exec(
        "CREATE TABLE calls (_id INTEGER PRIMARY KEY, number TEXT, date INTEGER);\
         INSERT INTO calls VALUES (1, '1', 1700000000);",
    );
    let run = run_db(&dir.db());
    let summary = only(&run, "android.contacts2.summary");
    assert_eq!(summary.text("date_unit"), Some("seconds"));
    assert_eq!(summary.text("date_min_utc"), Some("2023-11-14T22:13:20Z"));
}

#[test]
fn non_sqlite_file_emits_an_error_record_and_warning() {
    let dir = TempDir::new("bad");
    let path = dir.write(b"this is not a sqlite database at all");
    let run = run_db(&path);

    assert_eq!(run.count("android.contacts2.summary"), 0);
    let error = only(&run, "android.contacts2.error");
    assert_eq!(error.text("code"), Some("DB_ERROR"));
    assert!(error.text("message").unwrap_or("").contains("SQLite 3"));
    assert!(run.warnings.iter().any(|(code, _, _)| code == "DB_ERROR"));
    assert!(run.summary.as_deref().unwrap_or("").contains("没有可统计"));
}

#[test]
fn incomplete_database_copy_is_an_explicit_error() {
    let dir = TempDir::new("truncated");
    dir.exec(&format!(
        "{FULL_SCHEMA}; INSERT INTO calls VALUES (1, '1', 1700000000000, 1, 1, NULL, NULL);"
    ));
    let bytes = fs::read(dir.db()).unwrap();
    // Keep the SQLite header but drop most pages: header validation passes,
    // opening the database fails.
    let truncated = &bytes[..bytes.len().min(600)];
    let path = dir.write(truncated);
    let run = run_db(&path);

    let error = only(&run, "android.contacts2.error");
    assert_eq!(error.text("code"), Some("DB_ERROR"));
    assert!(run.warnings.iter().any(|(code, _, _)| code == "DB_ERROR"));
}

#[test]
fn manifest_declares_the_v2_identity() {
    let manifest = include_str!("../plugin.toml");
    assert!(manifest.contains("id = \"org.ljnljn.android-contacts2\""));
    assert!(manifest.contains("protocol = 2"));
    assert!(manifest.contains("library = \"ljnljn_android_contacts2\""));
    assert!(manifest.contains("id = \"contacts2_calls\""));
}
