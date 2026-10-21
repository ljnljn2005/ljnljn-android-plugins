//! Android `packages.xml` inventory (LPP v2).
//!
//! This is the LPP v2 port of the v1 `ljnljn.android-packages.file` file
//! analyzer. The strict quick-xml parser and its data model are carried over
//! unchanged (`model`, `parser`); only the result mapping is new: every input
//! produces one `android.packages.report` record, the
//! `<last-platform-version>` element becomes `android.packages.platform`, and
//! each de-duplicated package name becomes `android.packages.package`.
//!
//! De-duplication semantics are preserved: package names are compared
//! case-sensitively, the first occurrence keeps its position, later
//! occurrences only fill empty fields, and conflicting values are listed in
//! `conflicting_fields`.
//!
//! The parser never executes XML content, never expands external entities and
//! only accepts UTF-8 text XML. Bad XML, empty files, a non-`<packages>` root
//! and UTF-16 / Android binary XML produce an explicit status record plus a
//! warning instead of a silent empty success.

pub mod model;
pub mod parser;

#[cfg(test)]
mod tests;

use lovelycarve_plugin::{export_plugin, Plugin, Record, Result, Run};

use crate::model::{ElementKind, Limitation, ParseReport, ParseStatus, PlatformVersion};
use crate::parser::{parse_bytes, static_limitations, too_large_report};

/// Largest `packages.xml` read in one go (64 MiB). Larger inputs are reported
/// without being read.
pub const MAX_XML_BYTES: u64 = model::MAX_INPUT_BYTES;
/// Upper bound of emitted package records (the rest is counted, not dropped
/// silently).
pub const MAX_OUTPUT_PACKAGES: usize = model::MAX_OUTPUT_PACKAGES;

/// Document-level outcome of one input.
#[derive(Record)]
#[record(
    id = "android.packages.report",
    label = "packages.xml 解析报告",
    title = "{file}"
)]
struct PackagesReport {
    #[field(label = "文件", key)]
    file: String,
    #[field(
        label = "状态",
        kind = "enum",
        choices = "ok=解析完成,empty=空文档,not_packages=根元素不是 packages,bad_xml=XML 错误,unsupported_encoding=编码不支持,input_too_large=超过大小上限"
    )]
    status: String,
    #[field(label = "错误代码")]
    error_code: Option<String>,
    #[field(label = "错误说明")]
    error_message: Option<String>,
    #[field(label = "错误行号")]
    error_line: Option<i64>,
    #[field(label = "错误偏移")]
    error_offset: Option<i64>,
    #[field(label = "根元素")]
    root_name: Option<String>,
    #[field(label = "根元素限定名")]
    root_qname: Option<String>,
    #[field(label = "XML 声明")]
    xml_decl: Option<String>,
    #[field(label = "输入字节数", kind = "size")]
    input_bytes: i64,
    #[field(label = "package 元素数")]
    package_elements: i64,
    #[field(label = "updated-package 元素数")]
    updated_package_elements: i64,
    #[field(label = "唯一包名数")]
    unique_packages: i64,
    #[field(label = "重复包名数")]
    duplicate_packages: i64,
    #[field(label = "输出的包记录数")]
    output_packages: i64,
    #[field(label = "未输出的包记录数")]
    output_omitted: i64,
    #[field(label = "缺少 name 的包元素")]
    unnamed_packages: i64,
    #[field(label = "嵌套的 package 元素")]
    nested_package_elements: i64,
    #[field(label = "忽略的其他根子元素")]
    ignored_root_children: i64,
    #[field(label = "注释数")]
    comments: i64,
    #[field(label = "DOCTYPE 数")]
    doctype_count: i64,
    #[field(label = "命名空间声明数")]
    namespace_declarations: i64,
    #[field(label = "last-platform-version 元素数")]
    header_elements: i64,
    #[field(label = "last-platform-version 冲突")]
    header_conflict: bool,
    #[field(label = "限制与覆盖说明", kind = "list")]
    limitations: Vec<String>,
}

/// `<last-platform-version>` attributes exactly as stored.
#[derive(Record)]
#[record(
    id = "android.packages.platform",
    label = "Android 平台版本",
    title = "{file} · platform"
)]
struct PlatformVersionRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "internal")]
    internal: Option<String>,
    #[field(label = "external")]
    external: Option<String>,
    #[field(label = "fingerprint")]
    fingerprint: Option<String>,
    #[field(label = "行号")]
    line: i64,
    #[field(label = "字节偏移")]
    offset: i64,
}

/// One de-duplicated package name.
#[derive(Record)]
#[record(
    id = "android.packages.package",
    label = "Android 软件包",
    title = "{name}"
)]
struct PackageRecord {
    #[field(label = "文件", key)]
    file: String,
    #[field(label = "包名", key)]
    name: String,
    #[field(label = "版本")]
    version: Option<String>,
    #[field(label = "codePath", kind = "path")]
    code_path: Option<String>,
    #[field(label = "安装器")]
    installer: Option<String>,
    #[field(
        label = "首见元素",
        kind = "enum",
        choices = "package=package,updated_package=updated-package"
    )]
    first_kind: String,
    #[field(label = "首见行号")]
    first_line: i64,
    #[field(label = "首见字节偏移")]
    first_offset: i64,
    #[field(label = "出现次数")]
    occurrences: i64,
    #[field(label = "以 updated-package 出现次数")]
    updated_occurrences: i64,
    #[field(label = "重复")]
    duplicate: bool,
    #[field(label = "冲突字段", kind = "list")]
    conflicting_fields: Vec<String>,
}

fn packages(run: &Run) -> Result<()> {
    if run.inputs().is_empty() {
        run.warn(
            "INPUT_NOT_FOUND",
            "没有选中任何 XML 文件（分析器声明扩展名 xml）",
        )?;
        run.set_summary("未找到可分析的 packages.xml");
        return Ok(());
    }

    let mut files = 0usize;
    let mut unique = 0usize;
    let mut emitted_packages = 0usize;
    let mut failed = 0usize;

    for file in run.inputs() {
        run.check_cancelled()?;
        files += 1;
        let display = display_name(file);
        let report = if file.size() > MAX_XML_BYTES {
            too_large_report(file.size())
        } else {
            match file.read_all(MAX_XML_BYTES) {
                Ok(bytes) => parse_bytes(&bytes),
                Err(error) => {
                    failed += 1;
                    run.warn_file(
                        file,
                        &error.code,
                        format!("{display}: 读取失败：{}", error.message),
                    )?;
                    continue;
                }
            }
        };

        emit_report(run, file, &display, &report)?;
        if report.status != ParseStatus::Ok {
            failed += 1;
            let (code, message) = status_warning(&report);
            run.warn_file(file, code, format!("{display}: {message}"))?;
            continue;
        }

        if let Some(header) = &report.header {
            emit_platform(run, file, &display, header, report.header_conflict)?;
        }
        unique += report.packages.len();
        let start = run.emitted();
        let take = report.packages.len().min(MAX_OUTPUT_PACKAGES);
        for entry in report.packages.iter().take(take) {
            let record = PackageRecord {
                file: display.clone(),
                name: entry.name.clone(),
                version: entry.version.clone(),
                code_path: entry.code_path.clone(),
                installer: entry.installer.clone(),
                first_kind: match entry.first_kind {
                    ElementKind::Package => "package".to_owned(),
                    ElementKind::UpdatedPackage => "updated_package".to_owned(),
                },
                first_line: entry.first_position.line as i64,
                first_offset: entry.first_position.offset as i64,
                occurrences: entry.occurrences as i64,
                updated_occurrences: entry.updated_occurrences as i64,
                duplicate: entry.is_duplicate(),
                conflicting_fields: entry
                    .conflicting_fields
                    .iter()
                    .map(|field| (*field).to_owned())
                    .collect(),
            };
            run.record(&record)
                .from(file)
                .note(format!(
                    "packages.xml 第 {} 行，字节偏移 {}",
                    entry.first_position.line, entry.first_position.offset
                ))
                .emit()?;
        }
        emitted_packages += run.emitted().saturating_sub(start) as usize;
        if report.output_omitted() > 0 {
            run.warn_file(
                file,
                "OUTPUT_TRUNCATED",
                format!(
                    "{display}: 唯一包 {} 个，只输出前 {MAX_OUTPUT_PACKAGES} 个",
                    report.packages.len()
                ),
            )?;
        }
    }

    run.set_summary(format!(
        "解析 {files} 个 XML；各文件唯一包合计 {unique} 个，输出 {emitted_packages} 条包记录；失败 {failed} 个（详见告警与报告记录）"
    ));
    Ok(())
}

fn emit_report(
    run: &Run,
    file: &lovelycarve_plugin::FileRef,
    display: &str,
    report: &ParseReport,
) -> Result<()> {
    let mut limitations = report
        .limitations
        .iter()
        .map(format_limitation)
        .collect::<Vec<_>>();
    for limitation in static_limitations() {
        limitations.push(format_limitation(&limitation));
    }
    if report.output_omitted() > 0 {
        limitations.push(format!(
            "output_truncated: 仅输出前 {MAX_OUTPUT_PACKAGES} 个包，另有 {} 个未输出",
            report.output_omitted()
        ));
    }
    let error = report.error.as_ref();
    let record = PackagesReport {
        file: display.to_owned(),
        status: report.status.as_str().to_owned(),
        error_code: error.map(|error| error.code.to_owned()),
        error_message: error.map(|error| error.message.clone()),
        error_line: error.map(|error| error.position.line as i64),
        error_offset: error.map(|error| error.position.offset as i64),
        root_name: report.root_name.clone(),
        root_qname: report.root_qname.clone(),
        xml_decl: report.xml_decl.clone(),
        input_bytes: report.input_bytes as i64,
        package_elements: report.package_elements as i64,
        updated_package_elements: report.updated_package_elements as i64,
        unique_packages: report.packages.len() as i64,
        duplicate_packages: report.duplicate_package_names() as i64,
        output_packages: report.packages.len().min(MAX_OUTPUT_PACKAGES) as i64,
        output_omitted: report.output_omitted() as i64,
        unnamed_packages: report.unnamed_packages as i64,
        nested_package_elements: report.nested_package_elements as i64,
        ignored_root_children: report.ignored_root_children as i64,
        comments: report.comments as i64,
        doctype_count: report.doctype_count as i64,
        namespace_declarations: report.namespaces.len() as i64,
        header_elements: report.header_elements as i64,
        header_conflict: report.header_conflict,
        limitations,
    };
    run.record(&record)
        .from(file)
        .note(match &report.error {
            Some(error) => format!(
                "status={} error={} 第 {} 行 @{}",
                report.status.as_str(),
                error.code,
                error.position.line,
                error.position.offset
            ),
            None => format!("status={}", report.status.as_str()),
        })
        .emit()
}

fn emit_platform(
    run: &Run,
    file: &lovelycarve_plugin::FileRef,
    display: &str,
    header: &PlatformVersion,
    conflict: bool,
) -> Result<()> {
    let record = PlatformVersionRecord {
        file: display.to_owned(),
        internal: header.internal.clone(),
        external: header.external.clone(),
        fingerprint: header.fingerprint.clone(),
        line: header.position.line as i64,
        offset: header.position.offset as i64,
    };
    let note = if conflict {
        format!(
            "第 {} 行；存在多个 last-platform-version，保留首见值",
            header.position.line
        )
    } else {
        format!("第 {} 行", header.position.line)
    };
    run.record(&record).from(file).note(note).emit()
}

fn status_warning(report: &ParseReport) -> (&str, String) {
    match (&report.status, &report.error) {
        (_, Some(error)) => (
            error.code,
            format!("{}（第 {} 行）", error.message, error.position.line),
        ),
        (status, None) => (
            match status {
                ParseStatus::Empty => "EMPTY_DOCUMENT",
                ParseStatus::NotPackages => "NOT_PACKAGES",
                ParseStatus::BadXml => "BAD_XML",
                ParseStatus::UnsupportedEncoding => "UNSUPPORTED_ENCODING",
                ParseStatus::InputTooLarge => "INPUT_TOO_LARGE",
                ParseStatus::Ok => "OK",
            },
            status.as_str().to_owned(),
        ),
    }
}

fn format_limitation(limitation: &Limitation) -> String {
    format!("{}: {}", limitation.code, limitation.text)
}

fn display_name(file: &lovelycarve_plugin::FileRef) -> String {
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
    Plugin::new("org.ljnljn.android-packages")
        .record::<PackagesReport>()
        .record::<PlatformVersionRecord>()
        .record::<PackageRecord>()
        .analyzer("packages", packages)
}

export_plugin!(plugin);
