//! 把 [`Stats`] 映射成 LPP v1 结果：结果树节点（列声明）+ artifacts + findings。
//!
//! 约定：
//! * 每个 artifact 都带 `source.path` / `source.file_id` / `source.root`；
//! * 属性统一使用 `calls.` 前缀，值保持原始数值或明确说明；
//! * 每个结果树节点的 `type` 与对应 artifact 的 `type` 一致；
//! * 不生成 timeline：`date` 单位未换算成绝对时间，不能伪造 RFC 3339 时间戳。

use lovelycarve_plugin::{
    AnalysisResult, Artifact, Attributes, Finding, ResultColumn, ResultNode, Severity, Value,
};

use crate::stats::{NoteLevel, NumberStat, RawNum, Stats, TypeStat, DATE_UNIT_BASIS};

/// 结果命名空间（artifact type / 属性 key 前缀）。
pub const NS: &str = "ljnljn.android-contacts2";

/// 概览表格。
pub const ART_SUMMARY: &str = "ljnljn.android-contacts2.summary";
/// 列检查表格。
pub const ART_COLUMN: &str = "ljnljn.android-contacts2.column";
/// 号码分组表格。
pub const ART_NUMBER: &str = "ljnljn.android-contacts2.number";
/// 类型分布表格。
pub const ART_TYPE: &str = "ljnljn.android-contacts2.type";
/// 限制说明表格。
pub const ART_LIMITATION: &str = "ljnljn.android-contacts2.limitation";

/// limitations 展开上限（正常路径远小于此值）。
pub const MAX_LIMITATION_ITEMS: usize = 64;

/// 结果树声明（顺序即界面展示顺序）。
pub fn result_nodes() -> Vec<ResultNode> {
    fn node(node_type: &str, name: &str, columns: &[(&str, &str)]) -> ResultNode {
        ResultNode {
            node_type: node_type.to_owned(),
            name: name.to_owned(),
            columns: columns
                .iter()
                .map(|(key, label)| ResultColumn {
                    key: (*key).to_owned(),
                    label: (*label).to_owned(),
                })
                .collect(),
        }
    }

    vec![
        node(
            ART_SUMMARY,
            "calls 表概览",
            &[
                ("calls.table", "表名"),
                ("calls.table_kind", "对象类型"),
                ("calls.table_queried", "是否查询行数据"),
                ("calls.total_rows", "总行数"),
                ("calls.id_null", "_id 为 NULL 行数"),
                ("calls.id_non_integer", "_id 非整型行数"),
                ("calls.id_duplicate_values", "_id 重复取值数"),
                ("calls.distinct_rows", "去重后行数"),
                ("calls.distinct_key", "去重键"),
                ("calls.duplicate_rows", "重复行数"),
                ("calls.number_count", "号码数"),
                ("calls.numbers_truncated", "号码明细被截断"),
                ("calls.type_count", "type 取值数"),
                ("calls.types_truncated", "type 明细被截断"),
                ("calls.date_min_raw", "最早 date（原始值）"),
                ("calls.date_max_raw", "最晚 date（原始值）"),
                ("calls.date_unit", "date 单位判定"),
                ("calls.date_unit_basis", "date 单位判定依据"),
                ("calls.date_null", "date 为 NULL 行数"),
                ("calls.date_non_numeric", "date 非数值行数"),
                ("calls.date_zero", "date = 0 行数"),
                ("calls.date_negative", "date < 0 行数"),
                ("calls.duration_total_raw", "duration 合计（原始单位）"),
                ("calls.duration_null", "duration 为 NULL 行数"),
                ("calls.duration_non_integer", "非整型 duration 行数"),
                ("calls.name_nonempty", "name 非空行数"),
                ("calls.cached_name_nonempty", "cached_name 非空行数"),
                ("calls.columns_present", "存在的列"),
                ("calls.columns_missing", "缺失的列"),
                ("calls.journal_hint", "日志模式提示"),
                ("calls.objects_seen", "库内表/视图（最多 20 个）"),
                ("source.path", "来源文件"),
            ],
        ),
        node(
            ART_COLUMN,
            "calls 列检查",
            &[
                ("calls.column_name", "列名"),
                ("calls.column_present", "是否存在"),
                ("calls.column_declared_type", "声明类型"),
                ("calls.column_position", "列序号"),
                ("source.path", "来源文件"),
            ],
        ),
        node(
            ART_NUMBER,
            "按号码统计",
            &[
                ("calls.number", "号码（原始值）"),
                ("calls.number_raw_type", "号码原始类型"),
                ("calls.number_truncated", "号码展示是否截断"),
                ("calls.call_count", "通话次数"),
                ("calls.duration_total_raw", "总时长（原始单位）"),
                ("calls.duration_non_integer", "非整型 duration 行数"),
                ("source.path", "来源文件"),
            ],
        ),
        node(
            ART_TYPE,
            "type 分布",
            &[
                ("calls.type_raw", "type 原始值"),
                ("calls.type_raw_type", "type 原始类型"),
                ("calls.type_code", "type 数值"),
                ("calls.type_label", "type 含义"),
                ("calls.call_count", "通话次数"),
                ("source.path", "来源文件"),
            ],
        ),
        node(
            ART_LIMITATION,
            "限制与覆盖说明",
            &[
                ("calls.limitation_index", "序号"),
                ("calls.limitation", "限制说明"),
                ("source.path", "来源文件"),
            ],
        ),
    ]
}

/// 构造完整 LPP 结果；`source` 是宿主给的显示名，不是真实路径。
pub fn build_result(stats: &Stats, source: &str, file_id: &str) -> AnalysisResult {
    let summary = summary_text(stats);
    let mut artifacts = Vec::new();
    artifacts.push(summary_artifact(stats, source, file_id, &summary));
    for column in &stats.columns {
        artifacts.push(column_artifact(column, source, file_id));
    }
    for number in &stats.numbers {
        artifacts.push(number_artifact(number, source, file_id));
    }
    for entry in &stats.types {
        artifacts.push(type_artifact(entry, source, file_id));
    }
    for (index, limitation) in stats
        .limitations
        .iter()
        .take(MAX_LIMITATION_ITEMS)
        .enumerate()
    {
        artifacts.push(limitation_artifact(limitation, index + 1, source, file_id));
    }
    if stats.limitations.len() > MAX_LIMITATION_ITEMS {
        let remaining = stats.limitations.len() - MAX_LIMITATION_ITEMS;
        artifacts.push(limitation_artifact(
            &format!("另有 {remaining} 条限制说明未展开。"),
            MAX_LIMITATION_ITEMS + 1,
            source,
            file_id,
        ));
    }

    let findings = stats
        .notes
        .iter()
        .map(|note| Finding {
            severity: match note.level {
                NoteLevel::Info => Severity::Info,
                NoteLevel::Low => Severity::Low,
                NoteLevel::Medium => Severity::Medium,
            },
            title: note.title.clone(),
            description: note.detail.clone(),
            attributes: base_attributes(source, file_id),
        })
        .collect();

    AnalysisResult {
        summary: Some(summary),
        nodes: result_nodes(),
        artifacts,
        timeline: Vec::new(),
        findings,
    }
}

fn summary_artifact(stats: &Stats, source: &str, file_id: &str, summary: &str) -> Artifact {
    let mut attributes = base_attributes(source, file_id);
    attributes.insert_optional("calls.table", stats.table_name.as_deref());
    attributes.insert_optional("calls.table_kind", stats.table_kind.as_deref());
    attributes.insert("calls.table_queried", stats.table_queried);
    attributes.insert("calls.total_rows", stats.total_rows);
    attributes.insert_optional("calls.id_null", stats.id_null);
    attributes.insert_optional("calls.id_non_integer", stats.id_non_integer);
    attributes.insert_optional("calls.id_duplicate_values", stats.id_duplicate_values);
    attributes.insert_optional("calls.distinct_rows", stats.distinct_rows);
    attributes.insert("calls.distinct_key", stats.distinct_key.join(","));
    attributes.insert_optional(
        "calls.duplicate_rows",
        stats
            .distinct_rows
            .map(|distinct| (stats.total_rows - distinct).max(0)),
    );
    attributes.insert_optional("calls.number_count", stats.number_count);
    attributes.insert("calls.numbers_truncated", stats.numbers_truncated);
    attributes.insert_optional("calls.type_count", stats.type_count);
    attributes.insert("calls.types_truncated", stats.types_truncated);
    attributes.insert_optional("calls.date_min_raw", stats.date_min.map(raw_num_value));
    attributes.insert_optional("calls.date_max_raw", stats.date_max.map(raw_num_value));
    attributes.insert("calls.date_unit", stats.date_unit.as_str());
    attributes.insert("calls.date_unit_basis", DATE_UNIT_BASIS);
    attributes.insert_optional("calls.date_null", stats.date_null);
    attributes.insert_optional("calls.date_non_numeric", stats.date_non_numeric);
    attributes.insert_optional("calls.date_zero", stats.date_zero);
    attributes.insert_optional("calls.date_negative", stats.date_negative);
    attributes.insert_optional("calls.duration_total_raw", stats.duration_total);
    attributes.insert_optional("calls.duration_null", stats.duration_null);
    attributes.insert_optional("calls.duration_non_integer", stats.duration_non_integer);
    attributes.insert_optional("calls.name_nonempty", stats.name_nonempty);
    attributes.insert_optional("calls.cached_name_nonempty", stats.cached_name_nonempty);

    let present = stats
        .columns
        .iter()
        .filter(|column| column.present)
        .map(|column| column.name)
        .collect::<Vec<_>>()
        .join(",");
    let missing = stats
        .columns
        .iter()
        .filter(|column| !column.present)
        .map(|column| column.name)
        .collect::<Vec<_>>()
        .join(",");
    attributes.insert("calls.columns_present", present);
    attributes.insert("calls.columns_missing", missing);
    attributes.insert_optional("calls.journal_hint", stats.journal_hint.as_deref());
    attributes.insert(
        "calls.objects_seen",
        format!(
            "{}{}",
            stats.objects.join(", "),
            if stats.objects_more { " …" } else { "" }
        ),
    );

    Artifact {
        artifact_type: ART_SUMMARY.to_owned(),
        title: "calls 表概览".to_owned(),
        value: Some(Value::from(summary)),
        attributes,
    }
}

fn column_artifact(column: &crate::stats::ColumnInfo, source: &str, file_id: &str) -> Artifact {
    let mut attributes = base_attributes(source, file_id);
    attributes.insert("calls.column_name", column.name);
    attributes.insert("calls.column_present", column.present);
    attributes.insert_optional(
        "calls.column_declared_type",
        column.declared_type.as_deref(),
    );
    attributes.insert_optional("calls.column_position", column.position);
    Artifact {
        artifact_type: ART_COLUMN.to_owned(),
        title: format!(
            "列 {}（{}）",
            column.name,
            if column.present { "存在" } else { "缺失" }
        ),
        value: Some(Value::from(column.name)),
        attributes,
    }
}

fn number_artifact(number: &NumberStat, source: &str, file_id: &str) -> Artifact {
    let mut attributes = base_attributes(source, file_id);
    attributes.insert("calls.number", number.display.as_str());
    attributes.insert("calls.number_raw_type", number.raw_type);
    attributes.insert("calls.number_truncated", number.truncated);
    attributes.insert("calls.call_count", number.call_count);
    attributes.insert_optional("calls.duration_total_raw", number.duration_total);
    attributes.insert_optional("calls.duration_non_integer", number.duration_non_integer);
    Artifact {
        artifact_type: ART_NUMBER.to_owned(),
        title: format!("号码 {}", number.display),
        value: Some(Value::from(number.display.as_str())),
        attributes,
    }
}

fn type_artifact(entry: &TypeStat, source: &str, file_id: &str) -> Artifact {
    let mut attributes = base_attributes(source, file_id);
    attributes.insert("calls.type_raw", entry.display.as_str());
    attributes.insert("calls.type_raw_type", entry.raw_type);
    attributes.insert_optional("calls.type_code", entry.code);
    attributes.insert_optional("calls.type_label", entry.label);
    attributes.insert("calls.call_count", entry.count);
    let title = match entry.label {
        Some(label) => format!("type {}（{label}）", entry.display),
        None => format!("type {}（未映射）", entry.display),
    };
    Artifact {
        artifact_type: ART_TYPE.to_owned(),
        title,
        value: Some(Value::from(entry.display.as_str())),
        attributes,
    }
}

fn limitation_artifact(limitation: &str, index: usize, source: &str, file_id: &str) -> Artifact {
    let mut attributes = base_attributes(source, file_id);
    attributes.insert("calls.limitation_index", index as u64);
    attributes.insert("calls.limitation", limitation);
    Artifact {
        artifact_type: ART_LIMITATION.to_owned(),
        title: format!("限制 {index}"),
        value: Some(Value::from(limitation)),
        attributes,
    }
}

fn base_attributes(source: &str, file_id: &str) -> Attributes {
    let mut attributes = Attributes::new();
    attributes.insert("source.path", source);
    attributes.insert("source.file_id", file_id);
    // 单文件句柄没有检材卷 id；与既有插件一致，用固定值表示“文件级来源”。
    attributes.insert("source.root", "file");
    attributes
}

fn raw_num_value(value: RawNum) -> Value {
    match value {
        RawNum::Int(value) => Value::from(value),
        RawNum::Real(value) if value.is_finite() => Value::from(value),
        // LPP 拒绝非有限浮点；退化成字符串，保证结果不被宿主整体拒收。
        RawNum::Real(value) => Value::from(value.to_string()),
    }
}

fn summary_text(stats: &Stats) -> String {
    match (stats.table_name.as_deref(), stats.table_kind.as_deref()) {
        (None, _) => "未找到 calls 表：未生成通话统计（详见 limitations 与 finding）。".to_owned(),
        (Some(_), Some("view")) => {
            "calls 是视图（view）：为避免执行检材内 SQL，本插件不查询，未生成通话统计。".to_owned()
        }
        (Some(_), _) if stats.total_rows == 0 => {
            "calls 表存在但为空（0 行）：没有可统计的通话记录。".to_owned()
        }
        (Some(_), _) => {
            let mut parts = vec![format!("calls 表共 {} 行", stats.total_rows)];
            if let Some(distinct) = stats.distinct_rows {
                let duplicates = (stats.total_rows - distinct).max(0);
                let key = if stats.distinct_key.is_empty() {
                    "无可去重列".to_owned()
                } else {
                    format!("({})", stats.distinct_key.join(","))
                };
                parts.push(format!(
                    "按 {key} 去重后 {distinct} 行（重复 {duplicates}）"
                ));
            }
            match stats.number_count {
                Some(count) => parts.push(format!("号码 {count} 个")),
                None => parts.push("number 列缺失".to_owned()),
            }
            match stats.type_count {
                Some(count) => parts.push(format!("type {count} 类")),
                None => parts.push("type 列缺失".to_owned()),
            }
            match (stats.date_min, stats.date_max) {
                (Some(min), Some(max)) => parts.push(format!(
                    "date 原始范围 {}..{}，判定单位为 {}",
                    min.display(),
                    max.display(),
                    stats.date_unit.label()
                )),
                _ => {
                    if stats
                        .columns
                        .iter()
                        .any(|column| column.name == "date" && column.present)
                    {
                        parts.push("date 无数值型取值".to_owned());
                    } else {
                        parts.push("date 列缺失".to_owned());
                    }
                }
            }
            if let Some(total) = stats.duration_total {
                parts.push(format!("duration 合计 {total}（原始单位）"));
            }
            format!(
                "{}；date 单位仅按数量级判定，未换算绝对时间、不推断时区。",
                parts.join("；")
            )
        }
    }
}
