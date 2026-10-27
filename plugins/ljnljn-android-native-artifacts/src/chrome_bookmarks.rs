//! Chromium bookmark JSON reader.
//!
//! The `Bookmarks` file is a JSON tree, not a database. Only the known
//! `roots` / `children` structure is walked; no JSON path from evidence is
//! interpreted as code.

use lovelycarve_plugin::{FileRef, Json, Result, Run, Timestamp, Value};
use mobile_core::{ArtifactFile, ArtifactRecord};
use serde_json::Value as JsonValue;

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const PATTERNS: &[&str] = &[
    "**/app_chrome/Default/Bookmarks",
    "**/app_sbrowser/Default/Bookmarks",
    "**/app_opera/Bookmarks",
    "**/app_webview/Default/Bookmarks",
];

pub fn run_evidence(run: &Run) -> Result<()> {
    let view = run.require_view()?;
    let mut patterns = Vec::new();
    for mount in view.mounts() {
        let prefix = mount.path.trim_matches('/');
        for pattern in PATTERNS {
            patterns.push(if prefix.is_empty() {
                (*pattern).to_owned()
            } else {
                format!("{prefix}/{pattern}")
            });
        }
    }
    patterns.sort();
    patterns.dedup();
    let refs: Vec<&str> = patterns.iter().map(String::as_str).collect();
    let paths = match view.find(&refs) {
        Ok(paths) => paths,
        Err(error) => {
            run.warn(&error.code, error.message)?;
            return Ok(());
        }
    };
    let mut files = 0usize;
    let mut rows = 0usize;
    for path in paths {
        run.check_cancelled()?;
        let Some(file) = view.open(&path)? else {
            continue;
        };
        if let Some(count) = analyze(run, &file)? {
            files += 1;
            rows += count;
        }
    }
    if files == 0 {
        run.warn("BOOKMARKS_NOT_FOUND", "未找到 Chromium Bookmarks JSON")?;
    }
    run.set_summary(format!(
        "android_chrome_bookmarks：{files} 个文件，{rows} 条书签"
    ));
    Ok(())
}

pub fn run_selected(run: &Run) -> Result<()> {
    let mut files = 0usize;
    let mut rows = 0usize;
    for file in run.inputs() {
        run.check_cancelled()?;
        if let Some(count) = analyze(run, file)? {
            files += 1;
            rows += count;
        }
    }
    if files == 0 {
        run.warn("BOOKMARKS_NOT_FOUND", "所选文件不是 Chromium Bookmarks JSON")?;
    }
    run.set_summary(format!(
        "所选 Bookmarks：{files} 个文件，{rows} 条书签"
    ));
    Ok(())
}

fn analyze(run: &Run, file: &FileRef) -> Result<Option<usize>> {
    let source = file
        .view_path()
        .map(str::to_owned)
        .unwrap_or_else(|| file.name().to_owned());
    let text = match file.read_text(MAX_BYTES) {
        Ok(text) => text,
        Err(error) => {
            run.warn_file(file, &error.code, error.message)?;
            return Ok(None);
        }
    };
    let value: JsonValue = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            run.warn_file(
                file,
                "BOOKMARKS_JSON",
                format!("{source}: JSON 无法解析：{error}"),
            )?;
            return Ok(None);
        }
    };
    let Some(roots) = value.get("roots").and_then(JsonValue::as_object) else {
        run.warn_file(
            file,
            "BOOKMARKS_JSON",
            format!("{source}: 缺少 roots 对象"),
        )?;
        return Ok(None);
    };

    let mut rows = Vec::new();
    for (root_name, root) in roots {
        let mut path = Vec::new();
        collect(root, root_name, &mut path, &mut rows);
    }
    for (index, row) in rows.iter().enumerate() {
        run.check_cancelled()?;
        let time = row
            .date_added
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .map(|value| Timestamp::from_unix_nanos(
                value.div_euclid(1_000_000) - mobile_core::time::WEBKIT_EPOCH,
                (value.rem_euclid(1_000_000) as u32) * 1_000,
            ));
        let title = if row.name.is_empty() {
            row.url.clone()
        } else {
            row.name.clone()
        };
        let mut data = vec![
            (Value::from("url"), Value::from(row.url.as_str())),
            (Value::from("name"), Value::from(row.name.as_str())),
            (Value::from("parent"), Value::from(row.parent.as_str())),
            (Value::from("path"), Value::from(row.path.as_str())),
            (Value::from("id"), Value::from(row.id.as_str())),
            (Value::from("root"), Value::from(row.root.as_str())),
            (
                Value::from("date_added_raw"),
                Value::from(row.date_added.as_deref().unwrap_or("")),
            ),
        ];
        if let Some(kind) = &row.kind {
            data.push((Value::from("type"), Value::from(kind.as_str())));
        }
        run.record(&ArtifactRecord {
            platform: "android".to_owned(),
            artifact: "android_chrome_bookmarks".to_owned(),
            source: source.clone(),
            record_key: if row.id.is_empty() {
                format!("{}:{}", row.root, index + 1)
            } else {
                format!("{}:{}", row.root, row.id)
            },
            time,
            title,
            data: Json(Value::Map(data)),
        })
        .from(file)
        .note(format!("bookmark {}", index + 1))
        .emit()?;
    }

    run.record(&ArtifactFile {
        platform: "android".to_owned(),
        artifact: "android_chrome_bookmarks".to_owned(),
        source,
        status: if rows.is_empty() { "empty" } else { "ok" }.to_owned(),
        rows: rows.len() as i64,
        detail: "json".to_owned(),
    })
    .from(file)
    .emit()?;
    Ok(Some(rows.len()))
}

#[derive(Default)]
struct Bookmark {
    url: String,
    name: String,
    parent: String,
    path: String,
    id: String,
    root: String,
    date_added: Option<String>,
    kind: Option<String>,
}

fn collect(
    node: &JsonValue,
    root: &str,
    path: &mut Vec<String>,
    out: &mut Vec<Bookmark>,
) {
    let name = node
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    let kind = node.get("type").and_then(JsonValue::as_str);
    if kind == Some("url") {
        out.push(Bookmark {
            url: node
                .get("url")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_owned(),
            name: name.to_owned(),
            parent: path.last().cloned().unwrap_or_else(|| root.to_owned()),
            path: path.join(" / "),
            id: node
                .get("id")
                .map(display_scalar)
                .unwrap_or_default(),
            root: root.to_owned(),
            date_added: node
                .get("date_added")
                .map(display_scalar)
                .filter(|value| !value.is_empty()),
            kind: kind.map(str::to_owned),
        });
    }
    if let Some(children) = node.get("children").and_then(JsonValue::as_array) {
        if kind != Some("url") {
            path.push(name.to_owned());
        }
        for child in children {
            collect(child, root, path, out);
        }
        if kind != Some("url") {
            path.pop();
        }
    }
}

fn display_scalar(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        JsonValue::Number(number) => number.to_string(),
        JsonValue::Bool(boolean) => boolean.to_string(),
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}
