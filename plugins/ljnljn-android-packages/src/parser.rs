//! `packages.xml` 的严格事件解析（quick-xml）。
//!
//! 处理范围：
//! * XML 声明、注释、处理指令、DOCTYPE、CDATA：识别并跳过（不执行、不联网）。
//! * 根元素按 local-name 匹配（命名空间前缀忽略），必须（大小写不敏感地）是
//!   `packages`；根上的 `xmlns` / `xmlns:*` 声明记录后忽略。
//! * `<last-platform-version>` 与 `<package>` / `<updated-package>` 只认
//!   `<packages>` 的直接子元素；属性名按 ASCII 大小写不敏感匹配。
//! * 属性值由 quick-xml 做 XML 属性值规范化 + 预定义实体/数字字符引用解码
//!   （`&amp;`、`&#47;` 等）；未定义实体、坏引号、标签不配对等是坏 XML。
//! * 自闭合与成对标签等价；属性顺序不影响结果。
//! * 首见位置用 XML 源字节偏移（0 基）与行号（1 基）表示。
//!
//! 坏 XML 的行为：状态 `bad_xml`，包与平台结果清零，错误带 code/偏移/行号，
//! 并写入限制说明。解析器不 panic、不执行 XML 内容、不访问网络。

use std::collections::HashMap;

use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesDecl, BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::model::{
    clip_chars, ElementKind, Limitation, NamespaceDecl, PackageEntry, ParseError, ParseReport,
    ParseStatus, PlatformVersion, SourcePos, MAX_DECL_CHARS, MAX_FINGERPRINT_CHARS,
    MAX_INPUT_BYTES, MAX_INSTALLER_CHARS, MAX_NAMESPACE_DECLS, MAX_NAME_CHARS, MAX_PATH_CHARS,
    MAX_SHORT_VALUE_CHARS, MAX_UNIQUE_PACKAGES, MAX_VERSION_CHARS,
};

/// Android 二进制 XML（ABX/AXML）的 RES_XML_TYPE 头（小端 `0x00080003`）。
/// 这不是文本 XML；明确报不支持，而不是让 quick-xml 抛一个含混的语法错误。
const ANDROID_BINARY_XML_MAGIC: [u8; 4] = [0x03, 0x00, 0x08, 0x00];

/// 从内存字节解析。UTF-8 文本 XML 之外的输入会被明确拒绝（零结果 + 限制）。
pub fn parse_bytes(bytes: &[u8]) -> ParseReport {
    let length = bytes.len() as u64;
    if length > MAX_INPUT_BYTES {
        return too_large_report(length);
    }

    if looks_like_utf16(bytes) {
        let mut report = ParseReport::failure(
            ParseStatus::UnsupportedEncoding,
            "UTF16_NOT_SUPPORTED",
            "文件看起来是 UTF-16 编码的 XML；本插件只解析 UTF-8 文本 XML",
            length,
        );
        report.push_limitation(
            "utf16_not_supported",
            "UTF-16/带 NUL 字节的 XML 未解析；请提供 UTF-8 文本 packages.xml",
        );
        return report;
    }

    if looks_like_android_binary_xml(bytes) {
        let mut report = ParseReport::failure(
            ParseStatus::UnsupportedEncoding,
            "BINARY_XML_NOT_SUPPORTED",
            "文件是 Android 二进制 XML（ABX/AXML）头，不是文本 packages.xml",
            length,
        );
        report.push_limitation(
            "binary_xml_not_supported",
            "二进制 XML（ABX/AXML）未解析；请提供反编译/导出的文本 packages.xml",
        );
        return report;
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => parse_str(text),
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            let position = SourcePos {
                offset: valid_up_to as u64,
                line: line_for_prefix(&bytes[..valid_up_to]),
            };
            let mut report = ParseReport::failure(
                ParseStatus::UnsupportedEncoding,
                "INVALID_UTF8",
                format!("文件不是合法 UTF-8：{error}"),
                length,
            );
            if let Some(inner) = report.error.as_mut() {
                inner.position = position;
            }
            report.push_limitation("invalid_utf8", "非 UTF-8 字节无法解析；返回零结果");
            report
        }
    }
}

/// 从 UTF-8 文本解析。
pub fn parse_str(xml: &str) -> ParseReport {
    Parser::new(xml).run()
}

/// 文件超过上限时使用：不读取、不解析，零结果 + 明确原因。
pub fn too_large_report(length: u64) -> ParseReport {
    let mut report = ParseReport::failure(
        ParseStatus::InputTooLarge,
        "INPUT_TOO_LARGE",
        format!("文件有 {length} 字节，超过单次解析上限 {MAX_INPUT_BYTES} 字节；未读取/未解析"),
        length,
    );
    report.push_limitation(
        "input_too_large",
        "文件超过 64 MiB 解析上限；返回零结果，未做任何截断解析",
    );
    report
}

/// 预扫描：UTF-16 BOM 或明显的 UTF-16 `<` 字节模式。
fn looks_like_utf16(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    bytes.len() >= 2
        && ((bytes[0] == b'<' && bytes[1] == 0x00) || (bytes[0] == 0x00 && bytes[1] == b'<'))
}

fn looks_like_android_binary_xml(bytes: &[u8]) -> bool {
    bytes.starts_with(&ANDROID_BINARY_XML_MAGIC)
}

fn line_for_prefix(bytes: &[u8]) -> u64 {
    1 + count_line_breaks(bytes)
}

/// 按 XML 1.0 的行尾规则计数：`\r\n`、单独的 `\r`、单独的 `\n` 各算一行。
fn count_line_breaks(bytes: &[u8]) -> u64 {
    let mut breaks = 0_u64;
    let mut last_was_cr = false;
    for &byte in bytes {
        match byte {
            b'\r' => {
                breaks += 1;
                last_was_cr = true;
            }
            b'\n' => {
                if !last_was_cr {
                    breaks += 1;
                }
                last_was_cr = false;
            }
            _ => last_was_cr = false,
        }
    }
    breaks
}

/// 顺序位置换算：源偏移 → 行号。偏移单调递增（根、首见包、首见平台元素都按
/// 文档顺序出现），因此只需从上次扫描位置继续数换行，整体 O(n)。
struct LineTracker<'a> {
    xml: &'a str,
    scanned: usize,
    line: u64,
    /// 上一字节是否为 `\r`，用于把跨扫描片段的 `\r\n` 只算一行。
    last_was_cr: bool,
    /// quick-xml 在 from_str 路径会先消费 UTF-8 BOM，buffer_position 因此比源
    /// 文件少 3 字节；这里加回去，保证输出的是源文件绝对偏移。
    offset_base: u64,
}

impl<'a> LineTracker<'a> {
    fn new(xml: &'a str) -> Self {
        let offset_base = if xml.starts_with('\u{FEFF}') { 3 } else { 0 };
        Self {
            xml,
            scanned: 0,
            line: 1,
            last_was_cr: false,
            offset_base,
        }
    }

    fn position(&mut self, offset: u64) -> SourcePos {
        let offset = offset
            .saturating_add(self.offset_base)
            .min(self.xml.len() as u64) as usize;
        if offset < self.scanned {
            // 防御性回退：正常事件流不会发生，但错误定位可能重复请求更早的位置。
            self.scanned = 0;
            self.line = 1;
            self.last_was_cr = false;
        }
        for &byte in &self.xml.as_bytes()[self.scanned..offset] {
            match byte {
                b'\r' => {
                    self.line += 1;
                    self.last_was_cr = true;
                }
                b'\n' => {
                    if !self.last_was_cr {
                        self.line += 1;
                    }
                    self.last_was_cr = false;
                }
                _ => self.last_was_cr = false,
            }
        }
        self.scanned = offset;
        SourcePos {
            offset: offset as u64,
            line: self.line,
        }
    }
}

struct Parser<'a> {
    reader: Reader<&'a [u8]>,
    tracker: LineTracker<'a>,
    stack: Vec<String>,
    report: ParseReport,
    index: HashMap<String, usize>,
    packages: Vec<PackageEntry>,
    header: Option<PlatformVersion>,
    header_count: u64,
    header_conflict: bool,
    root_seen: bool,
    root_is_packages: bool,
    root_closed: bool,
}

impl<'a> Parser<'a> {
    fn new(xml: &'a str) -> Self {
        let mut reader = Reader::from_str(xml);
        // 显式写死严格选项：结束标签必须配对、不能有多余的结束标签。
        reader.config_mut().check_end_names = true;
        reader.config_mut().allow_unmatched_ends = false;
        Self {
            reader,
            tracker: LineTracker::new(xml),
            stack: Vec::new(),
            report: ParseReport::new(ParseStatus::Ok, xml.len() as u64),
            index: HashMap::new(),
            packages: Vec::new(),
            header: None,
            header_count: 0,
            header_conflict: false,
            root_seen: false,
            root_is_packages: false,
            root_closed: false,
        }
    }

    fn run(mut self) -> ParseReport {
        loop {
            let offset = self.reader.buffer_position();
            let event = match self.reader.read_event() {
                Ok(event) => event,
                Err(error) => {
                    self.fail("BAD_XML", format!("XML 解析失败：{error}"), offset);
                    break;
                }
            };

            match event {
                Event::Start(element) => {
                    let name = local_name(&element);
                    if !self.on_element(&element, &name, offset, false) {
                        break;
                    }
                }
                Event::Empty(element) => {
                    let name = local_name(&element);
                    if !self.on_element(&element, &name, offset, true) {
                        break;
                    }
                }
                Event::End(_) => self.on_end(),
                Event::Decl(declaration) => {
                    if !self.on_declaration(&declaration, offset) {
                        break;
                    }
                }
                Event::Comment(_) => self.report.comments += 1,
                Event::PI(_) => self.report.processing_instructions += 1,
                Event::CData(_) => {
                    if self.stack.is_empty() {
                        self.fail("TEXT_OUTSIDE_ROOT", "根元素之外存在 CDATA 段", offset);
                        break;
                    }
                    self.report.cdata_sections += 1;
                }
                Event::DocType(_) => {
                    if self.root_closed || !self.stack.is_empty() {
                        self.fail(
                            "DOCTYPE_NOT_IN_PROLOG",
                            "DOCTYPE 只能出现在根元素之前",
                            offset,
                        );
                        break;
                    }
                    if self.report.doctype_count > 0 {
                        self.fail("MULTIPLE_DOCTYPE", "存在多个 DOCTYPE", offset);
                        break;
                    }
                    self.report.doctype_count += 1;
                }
                Event::Text(text) => {
                    // XML 只允许根元素之外出现空白；非空白文本先于/晚于根元素都算坏 XML。
                    if self.stack.is_empty()
                        && text.as_ref().iter().any(|byte| !byte.is_ascii_whitespace())
                    {
                        self.fail("TEXT_OUTSIDE_ROOT", "根元素之外存在非空白文本", offset);
                        break;
                    }
                }
                Event::GeneralRef(_) => {
                    // 文本节点里的实体引用不展开；但根元素之外的引用属于非法内容。
                    if self.stack.is_empty() {
                        self.fail("TEXT_OUTSIDE_ROOT", "根元素之外存在实体引用", offset);
                        break;
                    }
                }
                Event::Eof => break,
            }
        }
        self.finish()
    }

    fn on_element(&mut self, element: &BytesStart, name: &str, offset: u64, empty: bool) -> bool {
        if self.root_closed {
            self.fail(
                "MULTIPLE_ROOTS",
                "根元素结束之后又出现元素（XML 只能有一个根元素）",
                offset,
            );
            return false;
        }

        if self.stack.is_empty() {
            if self.root_seen {
                self.fail("MULTIPLE_ROOTS", "XML 有多个根元素", offset);
                return false;
            }
            self.root_seen = true;
            let position = self.tracker.position(offset);
            self.report.root_qname = Some(raw_name(element));
            self.report.root_name = Some(name.to_owned());
            self.report.root_position = Some(position);

            if !name.eq_ignore_ascii_case("packages") {
                self.report.status = ParseStatus::NotPackages;
                self.report.error = Some(ParseError {
                    code: "NOT_PACKAGES",
                    message: format!(
                        "根元素是 <{name}>，不是 <packages>；未把它当作 Android 包清单解析"
                    ),
                    position,
                });
                self.report.push_limitation(
                    "not_packages_root",
                    "根元素不是 packages；返回零结果。请确认这是 Android packages.xml",
                );
                return false;
            }

            if let Err(message) = validate_attributes(element, self.reader.decoder()) {
                self.fail("BAD_XML", message, offset);
                return false;
            }
            if let Err(message) = self.collect_namespaces(element) {
                self.fail("BAD_XML", message, offset);
                return false;
            }

            self.root_is_packages = true;
            if empty {
                self.root_closed = true;
            } else {
                self.stack.push(name.to_owned());
            }
            return true;
        }

        // 已经在 <packages> 内部（根不是 packages 时已在上面终止）。
        if self.stack.len() == 1 {
            if name.eq_ignore_ascii_case("last-platform-version") {
                self.header_count += 1;
                let position = self.tracker.position(offset);
                match element_attributes(element, self.reader.decoder()) {
                    Ok((attributes, duplicates)) => {
                        self.report.duplicate_attributes += duplicates;
                        let parsed = platform_version(&attributes, position, &mut self.report);
                        match &self.header {
                            Some(existing) if platform_differs(existing, &parsed) => {
                                self.header_conflict = true;
                            }
                            Some(_) => {}
                            None => self.header = Some(parsed),
                        }
                    }
                    Err(message) => {
                        self.fail("BAD_XML", message, offset);
                        return false;
                    }
                }
            } else if name.eq_ignore_ascii_case("package") {
                self.report.package_elements += 1;
                if !self.record_package(element, offset, ElementKind::Package) {
                    return false;
                }
            } else if name.eq_ignore_ascii_case("updated-package") {
                self.report.updated_package_elements += 1;
                if !self.record_package(element, offset, ElementKind::UpdatedPackage) {
                    return false;
                }
            } else {
                if let Err(message) = validate_attributes(element, self.reader.decoder()) {
                    self.fail("BAD_XML", message, offset);
                    return false;
                }
                self.report.ignored_root_children += 1;
            }
        } else {
            // 更深层：只做属性良构校验，不把嵌套的 package 当成根下的包。
            if let Err(message) = validate_attributes(element, self.reader.decoder()) {
                self.fail("BAD_XML", message, offset);
                return false;
            }
            if name.eq_ignore_ascii_case("package") || name.eq_ignore_ascii_case("updated-package")
            {
                self.report.nested_package_elements += 1;
            }
        }

        if !empty {
            self.stack.push(name.to_owned());
        }
        true
    }

    fn on_end(&mut self) {
        if self.stack.pop().is_some() && self.stack.is_empty() {
            self.root_closed = true;
        }
    }

    fn on_declaration(&mut self, declaration: &BytesDecl, offset: u64) -> bool {
        if self.root_seen {
            self.fail(
                "DECL_AFTER_ROOT",
                "XML 声明出现在根元素之后，不是合法 XML",
                offset,
            );
            return false;
        }
        self.report.xml_decl_count += 1;
        if self.report.xml_decl.is_some() {
            self.fail("MULTIPLE_DECLARATIONS", "存在多个 XML 声明", offset);
            return false;
        }
        let raw_bytes: &[u8] = declaration;
        let raw = String::from_utf8_lossy(raw_bytes).trim().to_owned();
        let (text, _) = clip_chars(&raw, MAX_DECL_CHARS);
        self.report.xml_decl = Some(text);
        true
    }

    fn collect_namespaces(&mut self, element: &BytesStart) -> Result<(), String> {
        let decoder = self.reader.decoder();
        let mut collected: Vec<NamespaceDecl> = Vec::new();
        for attribute in element.attributes() {
            let attribute = attribute.map_err(|error| format!("根元素属性语法错误：{error}"))?;
            let key = attribute.key.as_ref();
            let prefix: Option<String> = if key == b"xmlns" {
                None
            } else if let Some(rest) = key.strip_prefix(b"xmlns:") {
                Some(
                    std::str::from_utf8(rest)
                        .map_err(|_| "命名空间前缀不是 UTF-8".to_string())?
                        .to_owned(),
                )
            } else {
                continue;
            };
            if collected.len() >= MAX_NAMESPACE_DECLS {
                break;
            }
            let uri = attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                .map_err(|error| {
                    format!(
                        "命名空间声明 {} 取值失败：{error}",
                        String::from_utf8_lossy(key)
                    )
                })?
                .into_owned();
            if !collected
                .iter()
                .any(|existing| existing.prefix == prefix && existing.uri == uri)
            {
                collected.push(NamespaceDecl { prefix, uri });
            }
        }
        self.report.namespaces.extend(collected);
        Ok(())
    }

    fn record_package(&mut self, element: &BytesStart, offset: u64, kind: ElementKind) -> bool {
        let (attributes, duplicates) = match element_attributes(element, self.reader.decoder()) {
            Ok(parsed) => parsed,
            Err(message) => {
                self.fail("BAD_XML", message, offset);
                return false;
            }
        };
        self.report.duplicate_attributes += duplicates;

        let (raw_name, name_hits) = attr_ci(&attributes, "name");
        if name_hits > 1 {
            self.report.ambiguous_attributes += 1;
        }
        let Some(raw_name) = raw_name else {
            self.report.unnamed_packages += 1;
            return true;
        };
        let name = raw_name.trim();
        if name.is_empty() {
            self.report.unnamed_packages += 1;
            return true;
        }
        if name.chars().count() > MAX_NAME_CHARS {
            // 截断会改变包身份，因此不登记；只计数并写入限制。
            self.report.oversized_name_packages += 1;
            return true;
        }
        let name = name.to_owned();

        let version = optional_field(&attributes, "version", MAX_VERSION_CHARS, &mut self.report);
        let code_path = optional_field(&attributes, "codePath", MAX_PATH_CHARS, &mut self.report);
        let installer = optional_field(
            &attributes,
            "installer",
            MAX_INSTALLER_CHARS,
            &mut self.report,
        );

        if let Some(&index) = self.index.get(&name) {
            let entry = &mut self.packages[index];
            entry.occurrences += 1;
            if kind == ElementKind::UpdatedPackage {
                entry.updated_occurrences += 1;
            }
            merge_field(
                &mut entry.version,
                version,
                "version",
                &mut entry.conflicting_fields,
            );
            merge_field(
                &mut entry.code_path,
                code_path,
                "codePath",
                &mut entry.conflicting_fields,
            );
            merge_field(
                &mut entry.installer,
                installer,
                "installer",
                &mut entry.conflicting_fields,
            );
            return true;
        }

        if self.packages.len() >= MAX_UNIQUE_PACKAGES {
            self.report.packages_skipped_after_cap += 1;
            return true;
        }

        let position = self.tracker.position(offset);
        let entry = PackageEntry {
            name: name.clone(),
            version,
            code_path,
            installer,
            first_kind: kind,
            first_position: position,
            occurrences: 1,
            updated_occurrences: if kind == ElementKind::UpdatedPackage {
                1
            } else {
                0
            },
            conflicting_fields: Vec::new(),
        };
        self.index.insert(name, self.packages.len());
        self.packages.push(entry);
        true
    }

    fn fail(&mut self, code: &'static str, message: impl Into<String>, offset: u64) {
        let position = self.tracker.position(offset);
        self.report.status = ParseStatus::BadXml;
        self.report.error = Some(ParseError {
            code,
            message: message.into(),
            position,
        });
        self.report.packages.clear();
        self.report.header = None;
        self.report.push_limitation(
            "bad_xml_zero_results",
            "XML 不是良构文档；返回零结果。错误位置见错误数据项",
        );
    }

    fn finish(mut self) -> ParseReport {
        match self.report.status {
            ParseStatus::NotPackages => return self.report,
            ParseStatus::BadXml => {
                // fail 已清空；这里再做一次防御性清零。
                self.report.packages.clear();
                self.report.header = None;
                return self.report;
            }
            ParseStatus::Empty | ParseStatus::UnsupportedEncoding | ParseStatus::InputTooLarge => {
                return self.report
            }
            ParseStatus::Ok => {}
        }

        if !self.root_seen {
            self.report.status = ParseStatus::Empty;
            self.report.push_limitation(
                "empty_document",
                "文件为空，或只有空白/注释/XML 声明，没有根元素",
            );
            return self.report;
        }

        if !self.root_is_packages {
            // 防御性分支：正常路径已在 on_element 里返回 NotPackages。
            self.report.status = ParseStatus::NotPackages;
            return self.report;
        }

        if !self.root_closed {
            let position = self.report.root_position.unwrap_or_default();
            self.report.status = ParseStatus::BadXml;
            self.report.error = Some(ParseError {
                code: "UNCLOSED_ROOT",
                message: "根元素 <packages> 未闭合".to_owned(),
                position,
            });
            self.report.packages.clear();
            self.report.header = None;
            self.report
                .push_limitation("bad_xml_zero_results", "根元素未闭合；返回零结果");
            return self.report;
        }

        self.report.header = self.header.take();
        self.report.header_elements = self.header_count;
        self.report.header_conflict = self.header_conflict;
        self.report.packages = std::mem::take(&mut self.packages);
        self.add_limitations();
        self.report
    }

    fn add_limitations(&mut self) {
        let header_incomplete = self.report.header.as_ref().is_some_and(|header| {
            header.internal.is_none() || header.external.is_none() || header.fingerprint.is_none()
        });
        if self.report.header.is_none() {
            self.report.push_limitation(
                "missing_last_platform_version",
                "没有找到 <last-platform-version>；internal/external/fingerprint 未输出",
            );
        } else if header_incomplete {
            self.report.push_limitation(
                "last_platform_version_incomplete",
                "last-platform-version 存在，但 internal/external/fingerprint 至少缺一个",
            );
        }
        if self.report.header_elements > 1 {
            self.report.push_limitation(
                "multiple_last_platform_version",
                "存在多个 <last-platform-version>；只使用第一个，其余仅计数/比较冲突",
            );
        }
        if self.report.header_conflict {
            self.report.push_limitation(
                "last_platform_version_conflict",
                "多个 last-platform-version 的已知属性值不一致；保留首见值",
            );
        }
        let packages_without_version = self
            .report
            .packages
            .iter()
            .filter(|entry| entry.version.is_none())
            .count() as u64;
        if packages_without_version > 0 {
            self.report.push_limitation(
                "missing_version_attribute",
                format!("{packages_without_version} 个唯一包没有 version 属性"),
            );
        }
        let duplicate_names = self.report.duplicate_package_names();
        if duplicate_names > 0 {
            self.report.push_limitation(
                "duplicate_package_names",
                format!(
                    "{duplicate_names} 个包名在文件中出现多次；按首次出现去重，后续元数据只填补空缺"
                ),
            );
        }
        if self.report.unnamed_packages > 0 {
            self.report.push_limitation(
                "unnamed_package_skipped",
                format!(
                    "{} 个 package/updated-package 元素缺少 name 或 name 为空，已跳过",
                    self.report.unnamed_packages
                ),
            );
        }
        if self.report.oversized_name_packages > 0 {
            self.report.push_limitation(
                "oversized_name_skipped",
                format!(
                    "{} 个包名超过 {MAX_NAME_CHARS} 字符，为避免改变包身份已跳过",
                    self.report.oversized_name_packages
                ),
            );
        }
        if self.report.packages_skipped_after_cap > 0 {
            self.report.push_limitation(
                "package_collection_capped",
                format!(
                    "唯一包名达到 {MAX_UNIQUE_PACKAGES} 上限后仍有 {} 个新包名未登记",
                    self.report.packages_skipped_after_cap
                ),
            );
        }
        if self.report.nested_package_elements > 0 {
            self.report.push_limitation(
                "nested_package_ignored",
                format!(
                    "{} 个 package 元素不在 <packages> 直接子级，未计入包清单",
                    self.report.nested_package_elements
                ),
            );
        }
        if self.report.ignored_root_children > 0 {
            self.report.push_limitation(
                "other_root_children",
                format!(
                    "{} 个 <packages> 直接子元素不是 package/updated-package/last-platform-version，已忽略",
                    self.report.ignored_root_children
                ),
            );
        }
        if self.report.doctype_count > 0 {
            self.report.push_limitation(
                "doctype_ignored",
                "存在 DOCTYPE；仅识别并忽略，不解析 DTD、不展开自定义实体、不读取外部实体",
            );
        }
        if !self.report.namespaces.is_empty() {
            self.report.push_limitation(
                "namespace_prefix_ignored",
                "根元素声明了命名空间；元素按 local-name 匹配、前缀忽略，命名空间 URI 不参与语义判断",
            );
        }
        if self.report.ambiguous_attributes > 0 {
            self.report.push_limitation(
                "ambiguous_attributes",
                format!(
                    "{} 组目标属性名在大小写不敏感比较下重复；使用文档顺序中的第一个值",
                    self.report.ambiguous_attributes
                ),
            );
        }
        if self.report.clipped_values > 0 {
            self.report.push_limitation(
                "attribute_value_truncated",
                format!(
                    "{} 个属性值超过字段长度上限，已截断并追加 '...'",
                    self.report.clipped_values
                ),
            );
        }
        if self.report.duplicate_attributes > 0 {
            self.report.push_limitation(
                "duplicate_attributes",
                format!(
                    "{} 个重复属性被检测到（quick-xml 通常已在解析时报错）",
                    self.report.duplicate_attributes
                ),
            );
        }
    }
}

fn local_name(element: &BytesStart) -> String {
    String::from_utf8_lossy(element.local_name().as_ref()).into_owned()
}

fn raw_name(element: &BytesStart) -> String {
    String::from_utf8_lossy(element.name().as_ref()).into_owned()
}

/// 解码一个元素的所有属性。quick-xml 的 Attributes 迭代器会先拒绝重复属性与
/// 坏引号；这里再对每个值做 XML 属性值规范化与实体解码，因此坏实体也会报错。
fn element_attributes(
    element: &BytesStart,
    decoder: Decoder,
) -> Result<(Vec<(String, String)>, u64), String> {
    let mut attributes = Vec::new();
    let mut duplicates = 0_u64;
    let mut seen: Vec<String> = Vec::new();

    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| format!("属性语法错误：{error}"))?;
        let key = std::str::from_utf8(attribute.key.as_ref())
            .map_err(|_| "属性名不是 UTF-8".to_owned())?
            .to_owned();
        if seen.iter().any(|existing| existing == &key) {
            duplicates += 1;
        } else {
            seen.push(key.clone());
        }
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
            .map_err(|error| format!("属性 {key} 取值失败：{error}"))?;
        attributes.push((key, value.into_owned()));
    }
    Ok((attributes, duplicates))
}

/// 对非目标元素只做属性良构校验（不保存值）。
fn validate_attributes(element: &BytesStart, decoder: Decoder) -> Result<(), String> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| format!("属性语法错误：{error}"))?;
        attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
            .map_err(|error| {
                format!(
                    "属性 {} 取值失败：{error}",
                    String::from_utf8_lossy(attribute.key.as_ref())
                )
            })?;
    }
    Ok(())
}

/// 大小写不敏感（ASCII）地查找属性；返回首个值与命中次数。
fn attr_ci<'a>(attributes: &'a [(String, String)], wanted: &str) -> (Option<&'a str>, u64) {
    let mut found = None;
    let mut hits = 0_u64;
    for (key, value) in attributes {
        if key.eq_ignore_ascii_case(wanted) {
            hits += 1;
            if found.is_none() {
                found = Some(value.as_str());
            }
        }
    }
    (found, hits)
}

fn optional_field(
    attributes: &[(String, String)],
    wanted: &str,
    max_chars: usize,
    report: &mut ParseReport,
) -> Option<String> {
    let (value, hits) = attr_ci(attributes, wanted);
    if hits > 1 {
        report.ambiguous_attributes += 1;
    }
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let (clipped, was_clipped) = clip_chars(value, max_chars);
    if was_clipped {
        report.clipped_values += 1;
    }
    Some(clipped)
}

fn platform_version(
    attributes: &[(String, String)],
    position: SourcePos,
    report: &mut ParseReport,
) -> PlatformVersion {
    PlatformVersion {
        internal: optional_field(attributes, "internal", MAX_SHORT_VALUE_CHARS, report),
        external: optional_field(attributes, "external", MAX_SHORT_VALUE_CHARS, report),
        fingerprint: optional_field(attributes, "fingerprint", MAX_FINGERPRINT_CHARS, report),
        position,
    }
}

fn platform_differs(left: &PlatformVersion, right: &PlatformVersion) -> bool {
    optional_differs(&left.internal, &right.internal)
        || optional_differs(&left.external, &right.external)
        || optional_differs(&left.fingerprint, &right.fingerprint)
}

fn optional_differs(left: &Option<String>, right: &Option<String>) -> bool {
    matches!((left, right), (Some(a), Some(b)) if a != b)
}

fn merge_field(
    target: &mut Option<String>,
    incoming: Option<String>,
    label: &'static str,
    conflicts: &mut Vec<&'static str>,
) {
    match (target.as_ref(), incoming) {
        (None, Some(value)) => *target = Some(value),
        (Some(existing), Some(value)) if existing != &value && !conflicts.contains(&label) => {
            conflicts.push(label);
        }
        _ => {}
    }
}

/// 供渲染层复用的静态限制说明（总是适用，不是"已支持"）。
pub const STATIC_LIMITATIONS: &[(&str, &str)] = &[
    (
        "text_xml_only",
        "只解析 UTF-8 文本 packages.xml；不按 XML 声明切换字节编码，Android 二进制 XML（ABX/AXML）与 UTF-16 明确拒绝",
    ),
    (
        "no_binary_or_manifest",
        "不解析压缩/二进制 XML，不读取 AndroidManifest.xml、APK、签名或权限；只输出 packages.xml 中可见字段",
    ),
    (
        "no_timeline",
        "packages.xml 不提供每包安装/更新时间；本插件不生成时间线事件，也不猜测时间",
    ),
    (
        "no_verdict",
        "不判断恶意、不校验完整性、不做信誉查询；version/codePath/installer 原样输出",
    ),
    (
        "read_only_no_network",
        "只读逻辑文件句柄；不执行 XML 内容、不展开外部实体、不联网",
    ),
    (
        "namespace_local_name",
        "元素按 local-name 匹配（前缀忽略）；带命名空间前缀的属性不作为目标属性；URI 不参与语义判断",
    ),
    (
        "case_insensitive_attributes",
        "目标属性名按 ASCII 大小写不敏感匹配；大小写变体重复时使用文档顺序中的第一个",
    ),
    (
        "dedup_first_seen",
        "包名按原样大小写去重；首见位置保留，后续同名项只填补空缺字段，冲突字段单独列出",
    ),
    (
        "bounded_output",
        "输入上限 64 MiB，最多登记 100,000 个唯一包名、输出 3,000 个包数据项；超限会显式说明",
    ),
];

/// 把静态限制转成报告条目（渲染层与测试共用）。
pub fn static_limitations() -> Vec<Limitation> {
    STATIC_LIMITATIONS
        .iter()
        .map(|(code, text)| Limitation {
            code,
            text: (*text).to_owned(),
        })
        .collect()
}
