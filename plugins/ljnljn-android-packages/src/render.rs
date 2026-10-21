//! LPP 结果渲染：平台版本、去重包清单、错误与汇总/限制。
//!
//! 数据项与结果树节点一一对应（`artifact_type` == 节点 `type`）。所有属性键
//! 使用小写命名空间前缀 `ljnljn.android.packages.*`；缺失的可选字段直接不写，
//! 由宿主表格留空，不用空字符串伪装"存在但为空"。

use lovelycarve_plugin::{AnalysisResult, Artifact, Attributes, ResultColumn, ResultNode, Value};

use crate::model::{Limitation, ParseReport, ParseStatus, PlatformVersion, MAX_OUTPUT_PACKAGES};
use crate::parser::static_limitations;

pub const NODE_PLATFORM: &str = "ljnljn.android.packages.platform";
pub const NODE_PACKAGE: &str = "ljnljn.android.packages.package";
pub const NODE_SUMMARY: &str = "ljnljn.android.packages.summary";
pub const NODE_ERROR: &str = "ljnljn.android.packages.error";

pub const KEY_SOURCE_FILE: &str = "ljnljn.android.packages.source_file";
pub const KEY_SOURCE_FILE_ID: &str = "ljnljn.android.packages.source_file_id";
pub const KEY_SOURCE_FILE_SIZE: &str = "ljnljn.android.packages.source_file_size";
pub const KEY_STATUS: &str = "ljnljn.android.packages.status";

pub const KEY_INTERNAL: &str = "ljnljn.android.packages.internal";
pub const KEY_EXTERNAL: &str = "ljnljn.android.packages.external";
pub const KEY_FINGERPRINT: &str = "ljnljn.android.packages.fingerprint";
pub const KEY_OFFSET: &str = "ljnljn.android.packages.offset";
pub const KEY_LINE: &str = "ljnljn.android.packages.line";
pub const KEY_HEADER_ELEMENTS: &str = "ljnljn.android.packages.header_elements";
pub const KEY_HEADER_CONFLICT: &str = "ljnljn.android.packages.header_conflict";

pub const KEY_RANK: &str = "ljnljn.android.packages.rank";
pub const KEY_NAME: &str = "ljnljn.android.packages.name";
pub const KEY_VERSION: &str = "ljnljn.android.packages.version";
pub const KEY_CODE_PATH: &str = "ljnljn.android.packages.code_path";
pub const KEY_INSTALLER: &str = "ljnljn.android.packages.installer";
pub const KEY_FIRST_OFFSET: &str = "ljnljn.android.packages.first_offset";
pub const KEY_FIRST_LINE: &str = "ljnljn.android.packages.first_line";
pub const KEY_FIRST_KIND: &str = "ljnljn.android.packages.first_kind";
pub const KEY_OCCURRENCES: &str = "ljnljn.android.packages.occurrences";
pub const KEY_UPDATED_OCCURRENCES: &str = "ljnljn.android.packages.updated_occurrences";
pub const KEY_DUPLICATE: &str = "ljnljn.android.packages.duplicate";
pub const KEY_CONFLICTING_FIELDS: &str = "ljnljn.android.packages.conflicting_fields";

pub const KEY_ERROR_CODE: &str = "ljnljn.android.packages.error_code";
pub const KEY_ERROR_MESSAGE: &str = "ljnljn.android.packages.error_message";
pub const KEY_ERROR_OFFSET: &str = "ljnljn.android.packages.error_offset";
pub const KEY_ERROR_LINE: &str = "ljnljn.android.packages.error_line";

pub const KEY_ROOT_NAME: &str = "ljnljn.android.packages.root_name";
pub const KEY_ROOT_QNAME: &str = "ljnljn.android.packages.root_qname";
pub const KEY_ROOT_OFFSET: &str = "ljnljn.android.packages.root_offset";
pub const KEY_ROOT_LINE: &str = "ljnljn.android.packages.root_line";
pub const KEY_XML_DECL: &str = "ljnljn.android.packages.xml_declaration";
pub const KEY_NAMESPACES: &str = "ljnljn.android.packages.namespaces";
pub const KEY_COMMENTS: &str = "ljnljn.android.packages.comments";
pub const KEY_PROCESSING_INSTRUCTIONS: &str = "ljnljn.android.packages.processing_instructions";
pub const KEY_CDATA: &str = "ljnljn.android.packages.cdata_sections";
pub const KEY_DOCTYPE: &str = "ljnljn.android.packages.doctype_count";

pub const KEY_PACKAGE_COUNT: &str = "ljnljn.android.packages.package_count";
pub const KEY_PACKAGE_ELEMENTS: &str = "ljnljn.android.packages.package_elements";
pub const KEY_UPDATED_PACKAGE_ELEMENTS: &str = "ljnljn.android.packages.updated_package_elements";
pub const KEY_DUPLICATE_PACKAGE_NAMES: &str = "ljnljn.android.packages.duplicate_package_names";
pub const KEY_OUTPUT_OMITTED: &str = "ljnljn.android.packages.output_omitted";
pub const KEY_UNNAMED_PACKAGES: &str = "ljnljn.android.packages.unnamed_packages";
pub const KEY_NESTED_PACKAGES: &str = "ljnljn.android.packages.nested_package_elements";
pub const KEY_IGNORED_ROOT_CHILDREN: &str = "ljnljn.android.packages.ignored_root_children";
pub const KEY_AMBIGUOUS_ATTRIBUTES: &str = "ljnljn.android.packages.ambiguous_attributes";
pub const KEY_CLIPPED_VALUES: &str = "ljnljn.android.packages.clipped_values";
pub const KEY_LIMITATIONS: &str = "ljnljn.android.packages.limitations";

/// 结果树节点声明（列只影响展示，不改变产物属性）。
pub fn result_nodes() -> Vec<ResultNode> {
    let columns = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(key, label)| ResultColumn {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
            })
            .collect()
    };

    vec![
        ResultNode {
            node_type: NODE_PLATFORM.to_owned(),
            name: "Android 平台版本（last-platform-version）".to_owned(),
            columns: columns(&[
                (KEY_INTERNAL, "internal API"),
                (KEY_EXTERNAL, "external API"),
                (KEY_FINGERPRINT, "fingerprint"),
                (KEY_OFFSET, "首次出现字节偏移"),
                (KEY_LINE, "首次出现行号"),
                (KEY_HEADER_ELEMENTS, "元素个数"),
                (KEY_HEADER_CONFLICT, "多值冲突"),
                (KEY_SOURCE_FILE, "源文件"),
            ]),
        },
        ResultNode {
            node_type: NODE_PACKAGE.to_owned(),
            name: "Android 软件包（按包名去重）".to_owned(),
            columns: columns(&[
                (KEY_RANK, "#"),
                (KEY_NAME, "包名"),
                (KEY_VERSION, "version"),
                (KEY_CODE_PATH, "codePath"),
                (KEY_INSTALLER, "installer"),
                (KEY_FIRST_OFFSET, "首见字节偏移"),
                (KEY_FIRST_LINE, "首见行号"),
                (KEY_FIRST_KIND, "首见元素"),
                (KEY_OCCURRENCES, "出现次数"),
                (KEY_UPDATED_OCCURRENCES, "updated-package 次数"),
                (KEY_DUPLICATE, "重复"),
                (KEY_CONFLICTING_FIELDS, "冲突字段"),
                (KEY_SOURCE_FILE, "源文件"),
                (KEY_SOURCE_FILE_ID, "源文件句柄"),
            ]),
        },
        ResultNode {
            node_type: NODE_ERROR.to_owned(),
            name: "解析错误".to_owned(),
            columns: columns(&[
                (KEY_ERROR_CODE, "错误码"),
                (KEY_ERROR_MESSAGE, "说明"),
                (KEY_ERROR_OFFSET, "字节偏移"),
                (KEY_ERROR_LINE, "行号"),
                (KEY_SOURCE_FILE, "源文件"),
            ]),
        },
        ResultNode {
            node_type: NODE_SUMMARY.to_owned(),
            name: "分析汇总与限制".to_owned(),
            columns: columns(&[
                (KEY_STATUS, "状态"),
                (KEY_SOURCE_FILE, "源文件"),
                (KEY_SOURCE_FILE_ID, "源文件句柄"),
                (KEY_SOURCE_FILE_SIZE, "文件大小"),
                (KEY_PACKAGE_COUNT, "唯一包数"),
                (KEY_PACKAGE_ELEMENTS, "<package> 元素数"),
                (KEY_UPDATED_PACKAGE_ELEMENTS, "<updated-package> 元素数"),
                (KEY_DUPLICATE_PACKAGE_NAMES, "重复包名数"),
                (KEY_OUTPUT_OMITTED, "结果省略包数"),
                (KEY_INTERNAL, "internal API"),
                (KEY_EXTERNAL, "external API"),
                (KEY_FINGERPRINT, "fingerprint"),
                (KEY_ROOT_NAME, "根元素 local-name"),
                (KEY_ROOT_OFFSET, "根元素偏移"),
                (KEY_ROOT_LINE, "根元素行号"),
                (KEY_XML_DECL, "XML 声明"),
                (KEY_NAMESPACES, "命名空间声明"),
                (KEY_COMMENTS, "注释数"),
                (KEY_LIMITATIONS, "限制与覆盖范围"),
                (KEY_ERROR_CODE, "错误码"),
                (KEY_ERROR_MESSAGE, "错误说明"),
            ]),
        },
    ]
}

/// 把解析报告映射为 LPP 结果。
pub fn build_result(
    report: &ParseReport,
    file_name: &str,
    file_id: &str,
    file_size: u64,
) -> AnalysisResult {
    let mut artifacts = Vec::new();

    if report.status == ParseStatus::Ok {
        if let Some(header) = &report.header {
            artifacts.push(platform_artifact(header, report, file_name, file_id));
        }
        for (index, entry) in report.packages.iter().take(MAX_OUTPUT_PACKAGES).enumerate() {
            let mut attributes = Attributes::new();
            attributes.insert(KEY_RANK, Value::from(index as u64 + 1));
            attributes.insert(KEY_NAME, Value::from(entry.name.as_str()));
            if let Some(version) = &entry.version {
                attributes.insert(KEY_VERSION, Value::from(version.as_str()));
            }
            if let Some(code_path) = &entry.code_path {
                attributes.insert(KEY_CODE_PATH, Value::from(code_path.as_str()));
            }
            if let Some(installer) = &entry.installer {
                attributes.insert(KEY_INSTALLER, Value::from(installer.as_str()));
            }
            attributes.insert(KEY_FIRST_OFFSET, Value::from(entry.first_position.offset));
            attributes.insert(KEY_FIRST_LINE, Value::from(entry.first_position.line));
            attributes.insert(KEY_FIRST_KIND, Value::from(entry.first_kind.as_str()));
            attributes.insert(KEY_OCCURRENCES, Value::from(entry.occurrences));
            attributes.insert(
                KEY_UPDATED_OCCURRENCES,
                Value::from(entry.updated_occurrences),
            );
            attributes.insert(KEY_DUPLICATE, Value::from(entry.is_duplicate()));
            attributes.insert(KEY_CONFLICTING_FIELDS, Value::from(conflicting_text(entry)));
            attributes.insert(KEY_SOURCE_FILE, Value::from(file_name));
            attributes.insert(KEY_SOURCE_FILE_ID, Value::from(file_id));

            let title = match &entry.version {
                Some(version) => format!("软件包：{}（version {}）", entry.name, version),
                None => format!("软件包：{}", entry.name),
            };
            let value = match &entry.version {
                Some(version) => format!("{} {}", entry.name, version),
                None => entry.name.clone(),
            };
            artifacts.push(Artifact {
                artifact_type: NODE_PACKAGE.to_owned(),
                title,
                value: Some(Value::from(value)),
                attributes,
            });
        }
    }

    if let Some(error) = &report.error {
        let mut attributes = Attributes::new();
        attributes.insert(KEY_ERROR_CODE, Value::from(error.code));
        attributes.insert(KEY_ERROR_MESSAGE, Value::from(error.message.as_str()));
        attributes.insert(KEY_ERROR_OFFSET, Value::from(error.position.offset));
        attributes.insert(KEY_ERROR_LINE, Value::from(error.position.line));
        attributes.insert(KEY_SOURCE_FILE, Value::from(file_name));
        attributes.insert(KEY_SOURCE_FILE_ID, Value::from(file_id));
        artifacts.push(Artifact {
            artifact_type: NODE_ERROR.to_owned(),
            title: format!("packages.xml 解析错误：{}", error.code),
            value: Some(Value::from(error.message.as_str())),
            attributes,
        });
    }

    let summary = summary_line(report);
    let mut summary_attributes = Attributes::new();
    summary_attributes.insert(KEY_STATUS, Value::from(report.status.as_str()));
    summary_attributes.insert(KEY_SOURCE_FILE, Value::from(file_name));
    summary_attributes.insert(KEY_SOURCE_FILE_ID, Value::from(file_id));
    summary_attributes.insert(KEY_SOURCE_FILE_SIZE, Value::from(file_size));
    summary_attributes.insert(KEY_PACKAGE_COUNT, Value::from(report.packages.len() as u64));
    summary_attributes.insert(KEY_PACKAGE_ELEMENTS, Value::from(report.package_elements));
    summary_attributes.insert(
        KEY_UPDATED_PACKAGE_ELEMENTS,
        Value::from(report.updated_package_elements),
    );
    summary_attributes.insert(
        KEY_DUPLICATE_PACKAGE_NAMES,
        Value::from(report.duplicate_package_names()),
    );
    summary_attributes.insert(KEY_OUTPUT_OMITTED, Value::from(report.output_omitted()));
    summary_attributes.insert(KEY_UNNAMED_PACKAGES, Value::from(report.unnamed_packages));
    summary_attributes.insert(
        KEY_NESTED_PACKAGES,
        Value::from(report.nested_package_elements),
    );
    summary_attributes.insert(
        KEY_IGNORED_ROOT_CHILDREN,
        Value::from(report.ignored_root_children),
    );
    summary_attributes.insert(
        KEY_AMBIGUOUS_ATTRIBUTES,
        Value::from(report.ambiguous_attributes),
    );
    summary_attributes.insert(KEY_CLIPPED_VALUES, Value::from(report.clipped_values));

    if let Some(header) = &report.header {
        insert_optional(&mut summary_attributes, KEY_INTERNAL, &header.internal);
        insert_optional(&mut summary_attributes, KEY_EXTERNAL, &header.external);
        insert_optional(
            &mut summary_attributes,
            KEY_FINGERPRINT,
            &header.fingerprint,
        );
        summary_attributes.insert(KEY_HEADER_ELEMENTS, Value::from(report.header_elements));
        summary_attributes.insert(KEY_HEADER_CONFLICT, Value::from(report.header_conflict));
    }

    if let Some(root_name) = &report.root_name {
        summary_attributes.insert(KEY_ROOT_NAME, Value::from(root_name.as_str()));
    }
    if let Some(root_qname) = &report.root_qname {
        summary_attributes.insert(KEY_ROOT_QNAME, Value::from(root_qname.as_str()));
    }
    if let Some(position) = report.root_position {
        summary_attributes.insert(KEY_ROOT_OFFSET, Value::from(position.offset));
        summary_attributes.insert(KEY_ROOT_LINE, Value::from(position.line));
    }
    if let Some(declaration) = &report.xml_decl {
        summary_attributes.insert(KEY_XML_DECL, Value::from(declaration.as_str()));
    }
    summary_attributes.insert(KEY_NAMESPACES, Value::from(namespace_text(report)));
    summary_attributes.insert(KEY_COMMENTS, Value::from(report.comments));
    summary_attributes.insert(
        KEY_PROCESSING_INSTRUCTIONS,
        Value::from(report.processing_instructions),
    );
    summary_attributes.insert(KEY_CDATA, Value::from(report.cdata_sections));
    summary_attributes.insert(KEY_DOCTYPE, Value::from(report.doctype_count));
    summary_attributes.insert(KEY_LIMITATIONS, Value::from(limitation_text(report)));
    if let Some(error) = &report.error {
        summary_attributes.insert(KEY_ERROR_CODE, Value::from(error.code));
        summary_attributes.insert(KEY_ERROR_MESSAGE, Value::from(error.message.as_str()));
        summary_attributes.insert(KEY_ERROR_OFFSET, Value::from(error.position.offset));
        summary_attributes.insert(KEY_ERROR_LINE, Value::from(error.position.line));
    }

    artifacts.push(Artifact {
        artifact_type: NODE_SUMMARY.to_owned(),
        title: "packages.xml 分析汇总".to_owned(),
        value: Some(Value::from(summary.as_str())),
        attributes: summary_attributes,
    });

    AnalysisResult {
        summary: Some(summary),
        nodes: result_nodes(),
        artifacts,
        timeline: Vec::new(),
        findings: Vec::new(),
    }
}

fn platform_artifact(
    header: &PlatformVersion,
    report: &ParseReport,
    file_name: &str,
    file_id: &str,
) -> Artifact {
    let mut attributes = Attributes::new();
    insert_optional(&mut attributes, KEY_INTERNAL, &header.internal);
    insert_optional(&mut attributes, KEY_EXTERNAL, &header.external);
    insert_optional(&mut attributes, KEY_FINGERPRINT, &header.fingerprint);
    attributes.insert(KEY_OFFSET, Value::from(header.position.offset));
    attributes.insert(KEY_LINE, Value::from(header.position.line));
    attributes.insert(KEY_HEADER_ELEMENTS, Value::from(report.header_elements));
    attributes.insert(KEY_HEADER_CONFLICT, Value::from(report.header_conflict));
    attributes.insert(KEY_SOURCE_FILE, Value::from(file_name));
    attributes.insert(KEY_SOURCE_FILE_ID, Value::from(file_id));

    let value = format!(
        "internal={} external={} fingerprint={}",
        header.internal.as_deref().unwrap_or("-"),
        header.external.as_deref().unwrap_or("-"),
        header.fingerprint.as_deref().unwrap_or("-")
    );
    Artifact {
        artifact_type: NODE_PLATFORM.to_owned(),
        title: "last-platform-version".to_owned(),
        value: Some(Value::from(value)),
        attributes,
    }
}

fn insert_optional(attributes: &mut Attributes, key: &str, value: &Option<String>) {
    if let Some(value) = value {
        attributes.insert(key, Value::from(value.as_str()));
    }
}

fn conflicting_text(entry: &crate::model::PackageEntry) -> String {
    if entry.conflicting_fields.is_empty() {
        "-".to_owned()
    } else {
        entry.conflicting_fields.join(",")
    }
}

/// 动态限制 + 静态覆盖范围说明。
pub fn limitation_text(report: &ParseReport) -> String {
    let mut all: Vec<Limitation> = report.limitations.clone();
    all.extend(static_limitations());
    all.iter()
        .map(|item| format!("{}: {}", item.code, item.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn namespace_text(report: &ParseReport) -> String {
    if report.namespaces.is_empty() {
        return "-".to_owned();
    }
    report
        .namespaces
        .iter()
        .map(|namespace| match &namespace.prefix {
            Some(prefix) => format!("{prefix}={}", namespace.uri),
            None => format!("(default)={}", namespace.uri),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// 一行中文汇总（同时写入 `AnalysisResult.summary` 与汇总数据项 value）。
pub fn summary_line(report: &ParseReport) -> String {
    if let Some(error) = &report.error {
        return format!(
            "packages.xml 解析状态 {}（{}）：{}；位置：字节偏移 {}，行 {}；返回 0 个包。",
            report.status.as_str(),
            error.code,
            error.message,
            error.position.offset,
            error.position.line
        );
    }

    match report.status {
        ParseStatus::Empty => {
            "packages.xml：文件为空或没有根元素（只有空白/注释/XML 声明）；0 个包。".to_owned()
        }
        ParseStatus::InputTooLarge => format!(
            "packages.xml：文件有 {} 字节，超过 64 MiB 解析上限；0 个包。",
            report.input_bytes
        ),
        ParseStatus::UnsupportedEncoding => format!(
            "packages.xml：不是受支持的 UTF-8 文本 XML（{} 字节）；0 个包。",
            report.input_bytes
        ),
        ParseStatus::BadXml => "packages.xml：XML 不是良构文档；0 个包。".to_owned(),
        ParseStatus::NotPackages => "packages.xml：根元素不是 <packages>；0 个包。".to_owned(),
        ParseStatus::Ok => {
            let header = match &report.header {
                Some(header) => format!(
                    "internal={} external={} fingerprint={}（偏移 {}，行 {}）",
                    header.internal.as_deref().unwrap_or("-"),
                    header.external.as_deref().unwrap_or("-"),
                    header.fingerprint.as_deref().unwrap_or("-"),
                    header.position.offset,
                    header.position.line
                ),
                None => "缺失".to_owned(),
            };
            let duplicates = report.duplicate_package_names();
            let mut line = format!(
                "packages.xml：根 <{}>；唯一包 {} 个（<package> {} 个、<updated-package> {} 个，重复包名 {} 个）；last-platform-version：{}。",
                report.root_name.as_deref().unwrap_or("?"),
                report.packages.len(),
                report.package_elements,
                report.updated_package_elements,
                duplicates,
                header
            );
            let omitted = report.output_omitted();
            if omitted > 0 {
                line.push_str(&format!(
                    "\n注意：结果只输出前 {MAX_OUTPUT_PACKAGES} 个包，另有 {omitted} 个唯一包未输出（见限制）。"
                ));
            }
            if !report.limitations.is_empty() {
                let codes = report
                    .limitations
                    .iter()
                    .map(|item| item.code)
                    .collect::<Vec<_>>()
                    .join(", ");
                line.push_str(&format!("\n限制/说明：{codes}。"));
            }
            line
        }
    }
}
