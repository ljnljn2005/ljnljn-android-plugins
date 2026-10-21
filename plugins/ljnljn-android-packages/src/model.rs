//! `packages.xml` 解析的数据模型与固定上限。
//!
//! 这里只放纯数据，不做 I/O；解析层（`parser`）与渲染层（`render`）共用。
//! 所有上限都是为了让结果在宿主 8 MiB 帧 / 10,000 数据项限制内可控，
//! 触发上限时会显式写入 `limitations`，不会静默丢弃。

/// 单次分析读入内存的 XML 上限。Android `packages.xml` 通常只有几 MiB；
/// 超过上限时返回零结果并说明原因，避免 512 MiB 进程配额被单个文件耗尽。
pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

/// 解析阶段最多登记多少个不同包名。超过后仍继续校验 XML 并计数，
/// 但不再登记新包名（避免恶意输入把去重表撑爆）。
pub const MAX_UNIQUE_PACKAGES: usize = 100_000;

/// 结果里最多输出多少个包数据项（宿主上限 10,000 项；留出汇总/平台/错误项余量）。
pub const MAX_OUTPUT_PACKAGES: usize = 3_000;

/// 包名字符上限；超长包名不登记（截断会改变包身份，因此宁可跳过并计数）。
pub const MAX_NAME_CHARS: usize = 256;
/// codePath 字符上限（超出截断并标记）。
pub const MAX_PATH_CHARS: usize = 1_024;
/// 短值（installer、internal、external 等）字符上限（超出截断并标记）。
pub const MAX_SHORT_VALUE_CHARS: usize = 256;
/// installer 字符上限（超出截断并标记）。
pub const MAX_INSTALLER_CHARS: usize = MAX_SHORT_VALUE_CHARS;
/// version 字符上限（超出截断并标记）。
pub const MAX_VERSION_CHARS: usize = 64;
/// fingerprint 字符上限（超出截断并标记）。
pub const MAX_FINGERPRINT_CHARS: usize = 512;
/// XML 声明文本保留上限。
pub const MAX_DECL_CHARS: usize = 512;
/// 记录命名空间声明的数量上限。
pub const MAX_NAMESPACE_DECLS: usize = 32;

/// 一次解析的总体状态（不是"有没有包"，而是文档层面发生了什么）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseStatus {
    /// 根元素是 `<packages>`，事件流完整结束（可以 0 个包）。
    Ok,
    /// 空文件 / 只有空白、注释、声明，没有根元素。
    Empty,
    /// 有根元素但 local-name 不是 `packages`。
    NotPackages,
    /// XML 语法/结构错误。按任务要求返回零结果并给出错误位置。
    BadXml,
    /// 不是 UTF-8 文本 XML（UTF-16 / Android 二进制 XML 等）。
    UnsupportedEncoding,
    /// 超过 [`MAX_INPUT_BYTES`]，未解析。
    InputTooLarge,
}

impl ParseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Empty => "empty",
            Self::NotPackages => "not_packages",
            Self::BadXml => "bad_xml",
            Self::UnsupportedEncoding => "unsupported_encoding",
            Self::InputTooLarge => "input_too_large",
        }
    }

    /// 是否为零结果状态（不输出任何包/平台数据项）。
    pub fn is_zero_result(self) -> bool {
        !matches!(self, Self::Ok)
    }
}

/// 文件内字节偏移（0 基）与行号（1 基）。偏移是 XML 源文件中的位置，
/// 不是宿主句柄的随机读序列位置。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourcePos {
    pub offset: u64,
    pub line: u64,
}

/// 一条明确的解析错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub code: &'static str,
    pub message: String,
    pub position: SourcePos,
}

/// `<last-platform-version>` 的原始属性值（不做数值转换，避免丢失原样）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlatformVersion {
    pub internal: Option<String>,
    pub external: Option<String>,
    pub fingerprint: Option<String>,
    pub position: SourcePos,
}

/// 包条目的首次出现来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElementKind {
    Package,
    UpdatedPackage,
}

impl ElementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::UpdatedPackage => "updated-package",
        }
    }
}

/// 去重后的一个包。字段来自首次出现的元素；后续同名项只填补空缺字段，
/// 冲突值不改写首见值，只记录在 `conflicting_fields`。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageEntry {
    pub name: String,
    pub version: Option<String>,
    pub code_path: Option<String>,
    pub installer: Option<String>,
    pub first_kind: ElementKind,
    pub first_position: SourcePos,
    pub occurrences: u64,
    pub updated_occurrences: u64,
    pub conflicting_fields: Vec<&'static str>,
}

impl PackageEntry {
    pub fn is_duplicate(&self) -> bool {
        self.occurrences > 1
    }

    pub fn seen_in_updated_package(&self) -> bool {
        self.updated_occurrences > 0
    }
}

/// 根元素上收集到的命名空间声明（只记录，不参与语义判断）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceDecl {
    /// `None` 表示默认命名空间（`xmlns="..."`）。
    pub prefix: Option<String>,
    pub uri: String,
}

/// 一条覆盖范围/限制说明，`code` 供程序筛选，`text` 给人看。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Limitation {
    pub code: &'static str,
    pub text: String,
}

/// 解析的完整结果。坏 XML 时 `packages`/`header` 已被清空（零结果），
/// 但元素计数仍保留，便于解释"看到了什么、为什么没输出"。
#[derive(Clone, Debug, PartialEq)]
pub struct ParseReport {
    pub status: ParseStatus,
    pub error: Option<ParseError>,
    pub input_bytes: u64,

    pub root_qname: Option<String>,
    pub root_name: Option<String>,
    pub root_position: Option<SourcePos>,
    pub xml_decl: Option<String>,
    pub xml_decl_count: u64,

    pub header: Option<PlatformVersion>,
    pub header_elements: u64,
    pub header_conflict: bool,

    pub packages: Vec<PackageEntry>,
    pub package_elements: u64,
    pub updated_package_elements: u64,
    pub unnamed_packages: u64,
    pub oversized_name_packages: u64,
    pub packages_skipped_after_cap: u64,

    pub nested_package_elements: u64,
    pub ignored_root_children: u64,

    pub comments: u64,
    pub processing_instructions: u64,
    pub cdata_sections: u64,
    pub doctype_count: u64,
    pub namespaces: Vec<NamespaceDecl>,

    pub duplicate_attributes: u64,
    pub ambiguous_attributes: u64,
    pub clipped_values: u64,

    pub limitations: Vec<Limitation>,
}

impl ParseReport {
    /// 最小报告骨架。
    pub fn new(status: ParseStatus, input_bytes: u64) -> Self {
        Self {
            status,
            error: None,
            input_bytes,
            root_qname: None,
            root_name: None,
            root_position: None,
            xml_decl: None,
            xml_decl_count: 0,
            header: None,
            header_elements: 0,
            header_conflict: false,
            packages: Vec::new(),
            package_elements: 0,
            updated_package_elements: 0,
            unnamed_packages: 0,
            oversized_name_packages: 0,
            packages_skipped_after_cap: 0,
            nested_package_elements: 0,
            ignored_root_children: 0,
            comments: 0,
            processing_instructions: 0,
            cdata_sections: 0,
            doctype_count: 0,
            namespaces: Vec::new(),
            duplicate_attributes: 0,
            ambiguous_attributes: 0,
            clipped_values: 0,
            limitations: Vec::new(),
        }
    }

    /// 直接失败的报告（编码不支持、文件过大等）。
    pub fn failure(
        status: ParseStatus,
        code: &'static str,
        message: impl Into<String>,
        input_bytes: u64,
    ) -> Self {
        let mut report = Self::new(status, input_bytes);
        report.error = Some(ParseError {
            code,
            message: message.into(),
            position: SourcePos::default(),
        });
        report
    }

    pub fn push_limitation(&mut self, code: &'static str, text: impl Into<String>) {
        if self.limitations.iter().any(|item| item.code == code) {
            return;
        }
        self.limitations.push(Limitation {
            code,
            text: text.into(),
        });
    }

    pub fn has_limitation(&self, code: &str) -> bool {
        self.limitations.iter().any(|item| item.code == code)
    }

    /// 结果输出阶段被省略的包数（首见顺序超过 [`MAX_OUTPUT_PACKAGES`] 的部分）。
    pub fn output_omitted(&self) -> u64 {
        self.packages.len().saturating_sub(MAX_OUTPUT_PACKAGES) as u64
    }

    /// 去重后包含多个元素的包数（`occurrences > 1`）。
    pub fn duplicate_package_names(&self) -> u64 {
        self.packages
            .iter()
            .filter(|entry| entry.is_duplicate())
            .count() as u64
    }
}

/// 按字符（不是字节）截断，返回是否发生了截断。
/// 截断标记用 ASCII `...`，避免把非法 UTF-8 边界写进结果。
pub fn clip_chars(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        return (value.to_owned(), false);
    }
    let mut clipped: String = value.chars().take(max_chars).collect();
    clipped.push_str("...");
    (clipped, true)
}
