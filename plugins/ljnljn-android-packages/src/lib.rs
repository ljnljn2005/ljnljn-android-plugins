//! Android `packages.xml` 文件分析插件（LovelyCarve / LPP v1）。
//!
//! 输入是宿主传入的单个逻辑文件句柄（只读）。插件按宿主给的字节流做纯离线
//! XML 解析，输出：
//! * `<last-platform-version internal=... external=... fingerprint=...>`（属性名
//!   大小写不敏感）；
//! * `<packages>` 直接子级 `<package>` 与可选 `<updated-package>` 的去重包名、
//!   version、codePath、installer、首见字节偏移/行号；
//! * 解析状态、重复包名、坏 XML 错误位置与覆盖范围/限制。
//!
//! 不解析 E01/分区/文件系统，也不接触检材真实路径：这些由宿主完成。解析器不
//! 执行 XML 内容、不展开外部实体、不联网、不写入任何文件。坏 XML、空文件、
//! 非 `<packages>` 根、UTF-16/二进制 XML 都返回明确状态与零结果，不 panic。

pub mod model;
pub mod parser;
pub mod render;

#[cfg(test)]
mod tests;

use lovelycarve_plugin::{
    export_plugin, AnalysisResult, Error, File, FileAnalyzer, LogLevel, Plugin, Result,
};

use crate::model::{ParseReport, ParseStatus};

/// 插件 id（与 plugin.toml `id` 一致）。
pub const PLUGIN_ID: &str = "org.ljnljn.android-packages";
/// 插件显示名（与 plugin.toml `name` 一致）。
pub const PLUGIN_NAME: &str = "Android packages.xml 软件包清单";
/// 单文件分析器名（plugin.toml 文件能力；必须唯一）。
pub const ANALYZER_FILE: &str = "ljnljn.android-packages.file";
/// 单文件读取上限（与 parser 的输入上限一致）。
pub const MAX_XML_BYTES: u64 = model::MAX_INPUT_BYTES;

pub use crate::model::{
    clip_chars, ElementKind, Limitation, NamespaceDecl, PackageEntry, ParseError,
    ParseReport as Report, ParseStatus as Status, PlatformVersion, SourcePos, MAX_INPUT_BYTES,
    MAX_NAME_CHARS, MAX_OUTPUT_PACKAGES, MAX_PATH_CHARS, MAX_UNIQUE_PACKAGES,
};
pub use crate::parser::{parse_bytes, parse_str, static_limitations, too_large_report};
pub use crate::render::{
    build_result, limitation_text, result_nodes, summary_line, NODE_ERROR, NODE_PACKAGE,
    NODE_PLATFORM, NODE_SUMMARY,
};

/// `FileAnalyzer` 入口。
pub struct PackagesXmlAnalyzer;

impl FileAnalyzer for PackagesXmlAnalyzer {
    fn analyzer(&self) -> &str {
        ANALYZER_FILE
    }

    fn analyze(&self, file: &File) -> Result<AnalysisResult> {
        let size = file.size();
        let report: ParseReport = if size > MAX_XML_BYTES {
            // 先看大小再读，避免把一个超大文件整体拉进 512 MiB 的进程配额。
            parser::too_large_report(size)
        } else {
            let bytes = file.read(0, size as usize).map_err(|error| {
                Error::new(
                    "FILE_READ_FAILED",
                    format!(
                        "读取 {}（{size} 字节）失败：{}: {}",
                        file.name(),
                        error.code,
                        error.message
                    ),
                )
            })?;
            if bytes.len() as u64 != size {
                return Err(Error::new(
                    "FILE_READ_FAILED",
                    format!(
                        "读取 {} 发生短读：按句柄应有 {size} 字节，实际读到 {} 字节",
                        file.name(),
                        bytes.len()
                    ),
                ));
            }
            parser::parse_bytes(&bytes)
        };

        let result = render::build_result(&report, file.name(), file.id(), size);
        let status = report.status.as_str();
        file.context().log(
            LogLevel::Info,
            &format!(
                "{ANALYZER_FILE}: {} [{}] {}",
                file.name(),
                status,
                result.summary.as_deref().unwrap_or("(no summary)")
            ),
        );
        Ok(result)
    }
}

/// `plugin.toml` 的 id/version 与代码注册值是否一致（测试使用）。
pub fn identity_matches(plugin_id: &str, version: &str) -> bool {
    plugin_id == PLUGIN_ID && version == env!("CARGO_PKG_VERSION")
}

/// 插件构造函数；身份/版本必须与 plugin.toml 一致。
fn build_plugin() -> Plugin {
    Plugin::new(PLUGIN_ID, PLUGIN_NAME, env!("CARGO_PKG_VERSION"))
        .file_analyzer(PackagesXmlAnalyzer)
}

export_plugin!(build_plugin);

/// 状态辅助：零结果状态判断（测试与宿主日志使用）。
pub fn is_zero_result(status: ParseStatus) -> bool {
    status.is_zero_result()
}
