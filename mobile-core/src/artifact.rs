//! Generic artifact records and fixed-SQL execution.

use std::collections::HashSet;

use lovelycarve_plugin::{Json, Record, Result, Run, Timestamp, Value};

use crate::sqlite::{self, Db, DbValue, QueryResult};
use crate::time::{self, TimeKind};

/// One artifact type, resolved from one or more database files.
pub struct SqlSpec {
    pub artifact: &'static str,
    pub platform: &'static str,
    /// Glob patterns relative to a mount root. `**/` searches the whole view.
    pub paths: &'static [&'static str],
    pub units: &'static [SqlUnit],
}

/// One fixed query inside an artifact. Several units can share an artifact,
/// for example SMS and MMS rows in the same telephony database.
pub struct SqlUnit {
    /// Short discriminator written into `data.query`.
    pub label: &'static str,
    /// Query variants tried in order; the first one whose tables all exist
    /// and whose fixed SQL succeeds is used.
    pub variants: &'static [SqlVariant],
    pub time_column: Option<&'static str>,
    pub time_kind: TimeKind,
    pub key_columns: &'static [&'static str],
    pub title_columns: &'static [&'static str],
}

pub struct SqlVariant {
    /// The query only runs when every table exists (ASCII case-insensitive).
    pub required_tables: &'static [&'static str],
    pub sql: &'static str,
}

#[derive(Record)]
#[record(
    id = "mobile.artifact",
    label = "移动终端工件",
    title = "{artifact} · {title}"
)]
pub struct ArtifactRecord {
    #[field(label = "平台", key)]
    pub platform: String,
    #[field(label = "工件", key)]
    pub artifact: String,
    #[field(label = "来源", key)]
    pub source: String,
    #[field(label = "记录键", key)]
    pub record_key: String,
    #[field(label = "时间", timeline = "mobile")]
    pub time: Option<Timestamp>,
    #[field(label = "标题")]
    pub title: String,
    #[field(label = "数据", kind = "json")]
    pub data: Json,
}

#[derive(Record)]
#[record(
    id = "mobile.artifact.file",
    label = "移动终端工件文件",
    title = "{artifact} · {status}"
)]
pub struct ArtifactFile {
    #[field(label = "平台", key)]
    pub platform: String,
    #[field(label = "工件", key)]
    pub artifact: String,
    #[field(label = "来源", key)]
    pub source: String,
    #[field(label = "状态")]
    pub status: String,
    #[field(label = "数据行数")]
    pub rows: i64,
    #[field(label = "说明")]
    pub detail: String,
}

pub fn run_evidence(run: &Run, spec: &SqlSpec) -> Result<()> {
    let view = run.require_view()?;
    let mut patterns = Vec::new();
    for mount in view.mounts() {
        let prefix = mount.path.trim_matches('/');
        for pattern in spec.paths {
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
        if let Some(count) = analyze_file(run, spec, &file, Some(&view))? {
            files += 1;
            rows += count;
        }
    }
    if files == 0 {
        run.warn(
            "ARTIFACT_NOT_FOUND",
            format!("未找到 {} 可识别的数据库", spec.artifact),
        )?;
    }
    run.set_summary(format!(
        "{}：{files} 个数据库，{rows} 条记录",
        spec.artifact
    ));
    Ok(())
}

/// Runs every spec against explicitly selected files. A selected database that
/// matches several specs (for example contacts2.db) is reported by each one.
pub fn run_files(run: &Run, specs: &'static [&'static SqlSpec]) -> Result<()> {
    let mut rows = 0usize;
    let mut matched = 0usize;
    for file in run.inputs() {
        run.check_cancelled()?;
        for spec in specs {
            if let Some(count) = analyze_file(run, spec, file, None)? {
                matched += 1;
                rows += count;
            }
        }
    }
    if matched == 0 {
        run.warn(
            "NO_MATCHING_DB",
            "所选文件没有被任何原生移动端工件识别",
        )?;
    }
    run.set_summary(format!("所选文件：{matched} 个工件匹配，{rows} 条记录"));
    Ok(())
}

fn analyze_file(
    run: &Run,
    spec: &SqlSpec,
    file: &lovelycarve_plugin::FileRef,
    view: Option<&lovelycarve_plugin::View>,
) -> Result<Option<usize>> {
    let source = file
        .view_path()
        .map(str::to_owned)
        .unwrap_or_else(|| file.name().to_owned());

    let db = match Db::open(file, view) {
        Ok(db) => db,
        Err(error) => {
            // A selected non-database is worth telling the examiner about; a
            // glob match that is not SQLite is not.
            run.warn_file(file, "DB_OPEN", format!("{source}: {error}"))?;
            return Ok(None);
        }
    };
    let tables = match db.tables() {
        Ok(tables) => tables,
        Err(error) => {
            run.warn_file(file, "DB_OBJECTS", format!("{source}: {error}"))?;
            return Ok(None);
        }
    };

    let mut total = 0usize;
    let mut applicable = 0usize;
    let mut failed = 0usize;
    let mut truncated = false;
    let mut details = Vec::new();

    for unit in spec.units {
        let mut tried = false;
        let mut unit_rows = 0usize;
        let mut last_error = None;
        for variant in unit.variants {
            if !has_tables(&tables, variant.required_tables) {
                continue;
            }
            tried = true;
            match db.query(variant.sql) {
                Ok(result) => {
                    if result.truncated {
                        truncated = true;
                    }
                    unit_rows = emit_rows(run, spec, unit, file, &source, &result)?;
                    last_error = None;
                    break;
                }
                Err(error) => last_error = Some(error),
            }
        }
        if !tried {
            continue;
        }
        applicable += 1;
        if let Some(error) = last_error {
            failed += 1;
            details.push(format!("{}=error", unit.label));
            run.warn_file(
                file,
                "QUERY_ERROR",
                format!("{source} [{}]: {error}", unit.label),
            )?;
        } else {
            total += unit_rows;
            details.push(format!("{}={unit_rows}", unit.label));
        }
    }

    if applicable == 0 {
        return Ok(None);
    }

    let status = if failed == applicable {
        "error"
    } else if failed > 0 {
        "partial"
    } else if truncated {
        "truncated"
    } else if total == 0 {
        "empty"
    } else {
        "ok"
    };
    run.record(&ArtifactFile {
        platform: spec.platform.to_owned(),
        artifact: spec.artifact.to_owned(),
        source,
        status: status.to_owned(),
        rows: total as i64,
        detail: details.join(", "),
    })
    .from(file)
    .emit()?;

    Ok(Some(total))
}

fn has_tables(tables: &HashSet<String>, required: &[&str]) -> bool {
    required
        .iter()
        .all(|name| tables.contains(&name.to_ascii_lowercase()))
}

fn emit_rows(
    run: &Run,
    spec: &SqlSpec,
    unit: &SqlUnit,
    file: &lovelycarve_plugin::FileRef,
    source: &str,
    result: &QueryResult,
) -> Result<usize> {
    let mut emitted = 0usize;
    for (index, row) in result.rows.iter().enumerate() {
        if index % 128 == 0 {
            run.check_cancelled()?;
        }
        let mut data = Vec::with_capacity(result.columns.len() + 1);
        for (position, column) in result.columns.iter().enumerate() {
            let value = row.get(position).cloned().unwrap_or(DbValue::Null);
            data.push((Value::from(column.as_str()), sqlite::to_json(&value)));
        }
        data.push((
            Value::from("query"),
            Value::from(unit.label),
        ));

        let key = join_columns(result, row, unit.key_columns);
        let record_key = if key.is_empty() {
            format!("{}:{}", unit.label, index + 1)
        } else {
            format!("{}:{key}", unit.label)
        };
        let title = title_for(spec, unit, result, row);
        let time = unit
            .time_column
            .and_then(|column| result.value(row, column))
            .and_then(DbValue::as_i64)
            .and_then(|value| time::convert(value, unit.time_kind));

        run.record(&ArtifactRecord {
            platform: spec.platform.to_owned(),
            artifact: spec.artifact.to_owned(),
            source: source.to_owned(),
            record_key,
            time,
            title,
            data: Json(Value::Map(data)),
        })
        .from(file)
        .note(format!("{} {} #{}", spec.artifact, unit.label, index + 1))
        .emit()?;
        emitted += 1;
    }
    Ok(emitted)
}

fn join_columns(result: &QueryResult, row: &[DbValue], columns: &[&str]) -> String {
    let mut parts = Vec::new();
    for column in columns {
        if let Some(value) = result.value(row, column) {
            let text = sqlite::display(value);
            let text = text.trim();
            if !text.is_empty() {
                parts.push(text.to_owned());
            }
        }
    }
    truncate(&parts.join("|"), 180)
}

fn title_for(
    spec: &SqlSpec,
    unit: &SqlUnit,
    result: &QueryResult,
    row: &[DbValue],
) -> String {
    let mut parts = Vec::new();
    for column in unit.title_columns {
        if let Some(value) = result.value(row, column) {
            let text = sqlite::display(value);
            let text = text.trim();
            if !text.is_empty() {
                parts.push(text.to_owned());
            }
        }
    }
    if parts.is_empty() {
        format!("{} {}", spec.artifact, unit.label)
    } else {
        truncate(&parts.join(" · "), 180)
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut result: String = text.chars().take(limit).collect();
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_variant_requires_all_tables() {
        let mut tables = HashSet::new();
        tables.insert("message".to_owned());
        assert!(has_tables(&tables, &["message"]));
        assert!(!has_tables(&tables, &["message", "chat"]));
    }

    #[test]
    fn truncation_keeps_char_boundaries() {
        assert_eq!(truncate("abcdef", 3), "abc…");
        assert_eq!(truncate("中文测试", 2), "中文…");
    }
}
