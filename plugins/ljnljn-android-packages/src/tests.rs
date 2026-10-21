//! 合成样本测试。所有 XML 都是手写/拼接的假数据，不读取真实案件或设备文件。

use lovelycarve_plugin::{Artifact, Value};

use crate::model::*;
use crate::parser::{parse_bytes, parse_str, static_limitations, too_large_report};
use crate::render::{
    build_result, result_nodes, summary_line, KEY_CODE_PATH, KEY_CONFLICTING_FIELDS, KEY_DUPLICATE,
    KEY_ERROR_CODE, KEY_EXTERNAL, KEY_FINGERPRINT, KEY_INSTALLER, KEY_INTERNAL, KEY_LIMITATIONS,
    KEY_NAME, KEY_OCCURRENCES, KEY_SOURCE_FILE, KEY_SOURCE_FILE_ID, KEY_UPDATED_OCCURRENCES,
    KEY_VERSION, NODE_ERROR, NODE_PACKAGE, NODE_PLATFORM, NODE_SUMMARY,
};
use crate::{identity_matches, ANALYZER_FILE, PLUGIN_ID};

const NORMAL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\" ?>\n\
<!-- synthetic packages.xml for tests -->\n\
<packages>\n\
  <last-platform-version internal=\"33\" external=\"33\" fingerprint=\"synthetic/fingerprint\" />\n\
  <package name=\"com.example.alpha\" codePath=\"/data/app/alpha/base.apk\" version=\"42\" installer=\"com.android.vending\" />\n\
  <package name=\"com.example.beta\" codePath=\"/system/app/Beta/Beta.apk\" />\n\
  <updated-package name=\"com.example.gamma\" codePath=\"/data/app/gamma/base.apk\" version=\"7\" />\n\
</packages>\n";

fn find_package<'a>(report: &'a ParseReport, name: &str) -> &'a PackageEntry {
    report
        .packages
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("package {name} not found"))
}

fn line_of(xml: &str, needle: &str) -> u64 {
    let offset = xml.find(needle).expect("needle");
    1 + xml[..offset].matches('\n').count() as u64
}

fn attr_text(artifact: &Artifact, key: &str) -> Option<String> {
    match artifact.attributes.get(key)? {
        Value::String(text) => text.as_str().map(str::to_owned),
        Value::Boolean(flag) => Some(flag.to_string()),
        Value::Integer(number) => number
            .as_u64()
            .map(|value| value.to_string())
            .or_else(|| number.as_i64().map(|value| value.to_string())),
        _ => None,
    }
}

fn artifacts_of<'a>(
    result: &'a lovelycarve_plugin::AnalysisResult,
    artifact_type: &str,
) -> Vec<&'a Artifact> {
    result
        .artifacts
        .iter()
        .filter(|artifact| artifact.artifact_type == artifact_type)
        .collect()
}

// ---------------------------------------------------------------------------
// 正常路径
// ---------------------------------------------------------------------------

#[test]
fn normal_document_outputs_header_and_packages() {
    let report = parse_str(NORMAL);
    assert_eq!(report.status, ParseStatus::Ok);
    assert!(report.error.is_none());

    let header = report.header.as_ref().expect("header");
    assert_eq!(header.internal.as_deref(), Some("33"));
    assert_eq!(header.external.as_deref(), Some("33"));
    assert_eq!(header.fingerprint.as_deref(), Some("synthetic/fingerprint"));
    assert_eq!(
        header.position.offset,
        NORMAL.find("<last-platform-version").unwrap() as u64
    );
    assert_eq!(
        header.position.line,
        line_of(NORMAL, "<last-platform-version")
    );

    assert_eq!(report.packages.len(), 3);
    assert_eq!(report.package_elements, 2);
    assert_eq!(report.updated_package_elements, 1);

    let alpha = find_package(&report, "com.example.alpha");
    assert_eq!(alpha.version.as_deref(), Some("42"));
    assert_eq!(alpha.code_path.as_deref(), Some("/data/app/alpha/base.apk"));
    assert_eq!(alpha.installer.as_deref(), Some("com.android.vending"));
    assert_eq!(alpha.first_kind, ElementKind::Package);
    assert_eq!(alpha.occurrences, 1);
    assert!(!alpha.is_duplicate());
    assert_eq!(
        alpha.first_position.offset,
        NORMAL.find("<package name=\"com.example.alpha\"").unwrap() as u64
    );
    assert_eq!(
        alpha.first_position.line,
        line_of(NORMAL, "<package name=\"com.example.alpha\"")
    );

    let beta = find_package(&report, "com.example.beta");
    assert!(beta.version.is_none());
    assert!(beta.installer.is_none());

    let gamma = find_package(&report, "com.example.gamma");
    assert_eq!(gamma.first_kind, ElementKind::UpdatedPackage);
    assert_eq!(gamma.updated_occurrences, 1);
    assert!(report.has_limitation("missing_version_attribute"));
}

#[test]
fn missing_last_platform_version_is_not_an_error() {
    let xml = "<packages>\n  <package name=\"com.example.only\" version=\"1\"/>\n</packages>\n";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert!(report.header.is_none());
    assert_eq!(report.packages.len(), 1);
    assert!(report.has_limitation("missing_last_platform_version"));
    assert_eq!(
        find_package(&report, "com.example.only").version.as_deref(),
        Some("1")
    );
}

#[test]
fn attribute_order_does_not_change_values() {
    let first = "<packages><package name=\"com.order\" codePath=\"/p\" version=\"7\" installer=\"com.store\"/></packages>";
    let second = "<packages><package installer=\"com.store\" version=\"7\" name=\"com.order\" codePath=\"/p\"/></packages>";

    let a = parse_str(first);
    let b = parse_str(second);
    assert_eq!(a.status, ParseStatus::Ok);
    assert_eq!(b.status, ParseStatus::Ok);

    let pa = find_package(&a, "com.order");
    let pb = find_package(&b, "com.order");
    assert_eq!(pa.version, pb.version);
    assert_eq!(pa.code_path, pb.code_path);
    assert_eq!(pa.installer, pb.installer);
    assert_eq!(pa.first_kind, pb.first_kind);
}

#[test]
fn attribute_names_are_case_insensitive_and_element_names_tolerate_case() {
    let xml = "<PACKAGES>\n\
  <LAST-PLATFORM-VERSION INTERNAL=\"22\" External=\"23\" FINGERPRINT=\"fp\"/>\n\
  <PACKAGE NAME=\"com.CaseSensitive\" CODEPATH=\"/p\" VERSION=\"1\" INSTALLER=\"i\"/>\n\
</PACKAGES>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    let header = report.header.as_ref().expect("header");
    assert_eq!(header.internal.as_deref(), Some("22"));
    assert_eq!(header.external.as_deref(), Some("23"));
    assert_eq!(header.fingerprint.as_deref(), Some("fp"));

    let package = find_package(&report, "com.CaseSensitive");
    assert_eq!(package.code_path.as_deref(), Some("/p"));
    assert_eq!(package.version.as_deref(), Some("1"));
    assert_eq!(package.installer.as_deref(), Some("i"));
}

#[test]
fn xml_declaration_comments_namespaces_self_closing_and_escapes_are_handled() {
    let xml = "<?xml version=\"1.0\"?>\n\
<!-- outer comment -->\n\
<p:packages xmlns:p=\"urn:p\" xmlns:android=\"urn:android\">\n\
  <p:last-platform-version internal=\"22\" external=\"22\" fingerprint=\"f&amp;g\"/>\n\
  <!-- inner comment -->\n\
  <p:package name=\"com.example.a&amp;b\" codePath=\"/data/a&#47;b\" version='1.0'/>\n\
  <p:updated-package name='com.single' codePath='/system/single'/>\n\
</p:packages>\n";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert!(report.xml_decl.as_deref().unwrap_or("").starts_with("xml "));
    assert_eq!(report.comments, 2);
    assert_eq!(report.namespaces.len(), 2);
    assert_eq!(report.namespaces[0].prefix.as_deref(), Some("p"));
    assert_eq!(report.namespaces[0].uri, "urn:p");
    assert_eq!(report.namespaces[1].prefix.as_deref(), Some("android"));
    assert!(report.has_limitation("namespace_prefix_ignored"));

    let header = report.header.as_ref().expect("header");
    assert_eq!(header.fingerprint.as_deref(), Some("f&g"));

    let escaped = find_package(&report, "com.example.a&b");
    assert_eq!(escaped.code_path.as_deref(), Some("/data/a/b"));
    assert_eq!(escaped.version.as_deref(), Some("1.0"));

    let single_quoted = find_package(&report, "com.single");
    assert_eq!(single_quoted.code_path.as_deref(), Some("/system/single"));
    assert_eq!(single_quoted.first_kind, ElementKind::UpdatedPackage);
}

#[test]
fn crlf_and_lf_line_numbers_are_correct() {
    let xml = "<packages>\r\n  <last-platform-version internal=\"22\"/>\r\n  <package name=\"com.crlf\"/>\r\n</packages>\r\n";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    let header = report.header.as_ref().expect("header");
    assert_eq!(header.position.line, 2);
    assert_eq!(
        header.position.offset,
        xml.find("<last-platform-version").unwrap() as u64
    );
    let package = find_package(&report, "com.crlf");
    assert_eq!(package.first_position.line, 3);
    assert_eq!(
        package.first_position.offset,
        xml.find("<package name=\"com.crlf\"").unwrap() as u64
    );
}

#[test]
fn cr_only_line_endings_are_counted() {
    let xml = "<packages>\r  <package name=\"com.cr\"/>\r</packages>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(find_package(&report, "com.cr").first_position.line, 2);
}

#[test]
fn utf8_bom_is_accepted_and_offsets_stay_source_relative() {
    let xml = "\u{FEFF}<packages>\n  <package name=\"com.bom\"/>\n</packages>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.root_position.expect("root").offset, 3);
    assert_eq!(
        find_package(&report, "com.bom").first_position.offset,
        xml.find("<package name=\"com.bom\"").unwrap() as u64
    );
}

#[test]
fn empty_packages_root_is_ok_with_zero_packages() {
    let report = parse_str("<packages/>");
    assert_eq!(report.status, ParseStatus::Ok);
    assert!(report.packages.is_empty());
    assert!(report.root_position.is_some());
    assert!(report.has_limitation("missing_last_platform_version"));
}

#[test]
fn doctype_is_recognized_but_entities_are_not_expanded() {
    let with_doctype = "<!DOCTYPE packages [<!ELEMENT packages ANY>]>\n\
<packages><package name=\"com.doctype\"/></packages>";
    let report = parse_str(with_doctype);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.doctype_count, 1);
    assert!(report.has_limitation("doctype_ignored"));
    assert_eq!(find_package(&report, "com.doctype").name, "com.doctype");

    let custom_entity = "<!DOCTYPE packages [<!ENTITY foo \"bar\">]>\n\
<packages><package name=\"&foo;\"/></packages>";
    let rejected = parse_str(custom_entity);
    assert_eq!(rejected.status, ParseStatus::BadXml);
    assert!(rejected.packages.is_empty());
}

#[test]
fn nested_sigs_and_perms_do_not_break_package_inventory() {
    let xml = "<packages>\n\
  <last-platform-version internal=\"34\" external=\"34\" fingerprint=\"synthetic/build\" />\n\
  <permission-trees>\n\
    <permission-tree name=\"com.example.tree\" package=\"com.example.tree\"/>\n\
  </permission-trees>\n\
  <package name=\"com.example.settings\" codePath=\"/system/priv-app/Settings\" version=\"123\" sharedUserId=\"android.uid.system\">\n\
    <sigs count=\"1\"><cert key=\"synthetic-cert\"/></sigs>\n\
    <perms><item name=\"android.permission.INTERNET\" granted=\"true\" flags=\"0\"/></perms>\n\
  </package>\n\
  <updated-package name=\"com.example.settings\" codePath=\"/system/priv-app/Settings\" />\n\
</packages>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.packages.len(), 1);
    assert_eq!(report.duplicate_package_names(), 1);
    assert_eq!(report.ignored_root_children, 1);
    let entry = find_package(&report, "com.example.settings");
    assert_eq!(entry.version.as_deref(), Some("123"));
    assert_eq!(entry.occurrences, 2);
    assert_eq!(entry.updated_occurrences, 1);
}

#[test]
fn header_after_packages_is_still_found() {
    let xml = "<packages>\n\
  <package name=\"com.before.header\"/>\n\
  <last-platform-version internal=\"35\" external=\"35\" fingerprint=\"f\"/>\n\
</packages>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(
        find_package(&report, "com.before.header")
            .first_position
            .line,
        2
    );
    let header = report.header.as_ref().expect("header");
    assert_eq!(header.internal.as_deref(), Some("35"));
    assert_eq!(header.position.line, 3);
}

#[test]
fn empty_version_is_treated_as_missing() {
    let report =
        parse_str("<packages><package name=\"com.empty.version\" version=\"\"/></packages>");
    assert_eq!(report.status, ParseStatus::Ok);
    assert!(find_package(&report, "com.empty.version").version.is_none());
    assert!(report.has_limitation("missing_version_attribute"));
}

#[test]
fn case_variant_duplicate_attributes_use_document_order_first() {
    let report =
        parse_str("<packages><package name=\"com.first\" NAME=\"com.second\"/></packages>");
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.packages.len(), 1);
    assert_eq!(report.packages[0].name, "com.first");
    assert_eq!(report.ambiguous_attributes, 1);
    assert!(report.has_limitation("ambiguous_attributes"));
}

// ---------------------------------------------------------------------------
// 去重 / updated-package
// ---------------------------------------------------------------------------

#[test]
fn duplicate_package_names_are_deduplicated_with_first_position() {
    let xml = "<packages>\n\
  <package name=\"com.dup\" codePath=\"/first\" version=\"1\"/>\n\
  <updated-package name=\"com.dup\" codePath=\"/first\" installer=\"com.store\"/>\n\
  <package name=\"com.dup\" codePath=\"/second\" version=\"2\"/>\n\
  <package name=\"com.other\"/>\n\
</packages>\n";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.packages.len(), 2);
    assert_eq!(report.duplicate_package_names(), 1);

    let duplicate = find_package(&report, "com.dup");
    assert_eq!(duplicate.occurrences, 3);
    assert_eq!(duplicate.updated_occurrences, 1);
    assert!(duplicate.is_duplicate());
    assert_eq!(duplicate.first_kind, ElementKind::Package);
    assert_eq!(
        duplicate.first_position.offset,
        xml.find("<package name=\"com.dup\" codePath=\"/first\"")
            .unwrap() as u64
    );
    assert_eq!(
        duplicate.first_position.line,
        line_of(xml, "<package name=\"com.dup\" codePath=\"/first\"")
    );
    // 首见值保留；后续只填补 installer。
    assert_eq!(duplicate.code_path.as_deref(), Some("/first"));
    assert_eq!(duplicate.version.as_deref(), Some("1"));
    assert_eq!(duplicate.installer.as_deref(), Some("com.store"));
    assert_eq!(duplicate.conflicting_fields, vec!["version", "codePath"]);
    assert!(report.has_limitation("duplicate_package_names"));
}

#[test]
fn updated_package_only_is_included() {
    let xml = "<packages><updated-package name=\"com.updated\" codePath=\"/u\" version=\"9\"/></packages>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    let entry = find_package(&report, "com.updated");
    assert_eq!(entry.first_kind, ElementKind::UpdatedPackage);
    assert_eq!(entry.updated_occurrences, 1);
    assert_eq!(entry.version.as_deref(), Some("9"));
}

// ---------------------------------------------------------------------------
// 空 / 坏 XML / 非 packages 根
// ---------------------------------------------------------------------------

#[test]
fn empty_inputs_are_empty_status() {
    for input in [
        "",
        "   \n\t  ",
        "<?xml version=\"1.0\"?>",
        "<!-- only a comment -->",
    ] {
        let report = parse_str(input);
        assert_eq!(report.status, ParseStatus::Empty, "input: {input:?}");
        assert!(report.packages.is_empty());
        assert!(report.header.is_none());
        assert!(report.error.is_none());
        assert!(report.has_limitation("empty_document"));
    }
}

#[test]
fn bad_xml_returns_zero_results_with_error_position() {
    let mismatched = "<packages>\n  <package name=\"com.example.a\">\n</packages>\n";
    let report = parse_str(mismatched);
    assert_eq!(report.status, ParseStatus::BadXml);
    assert!(report.packages.is_empty());
    assert!(report.header.is_none());
    let error = report.error.as_ref().expect("error");
    assert_eq!(error.code, "BAD_XML");
    assert!(error.position.offset > 0);
    assert!(error.position.line >= 1);
    assert!(report.has_limitation("bad_xml_zero_results"));
}

#[test]
fn bad_entity_in_attribute_is_bad_xml() {
    let report = parse_str("<packages><package name=\"com.a&b\"/></packages>");
    assert_eq!(report.status, ParseStatus::BadXml);
    assert!(report.packages.is_empty());
    assert!(report.error.is_some());
}

#[test]
fn unclosed_tag_and_unclosed_comment_are_bad_xml() {
    for xml in [
        "<packages><package name=\"a\"",
        "<packages><!-- unclosed",
        "<packages><package name=\"a\"></packages>",
        "<packages><package name=\"a\"/></packages",
    ] {
        let report = parse_str(xml);
        assert_eq!(report.status, ParseStatus::BadXml, "input: {xml:?}");
        assert!(report.packages.is_empty());
        assert!(report.error.is_some());
    }
}

#[test]
fn not_packages_root_returns_zero_results() {
    let xml = "<manifest><package name=\"com.example.in.manifest\"/></manifest>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::NotPackages);
    assert!(report.packages.is_empty());
    assert_eq!(report.root_name.as_deref(), Some("manifest"));
    let error = report.error.as_ref().expect("error");
    assert_eq!(error.code, "NOT_PACKAGES");
    assert!(report.has_limitation("not_packages_root"));
}

#[test]
fn multiple_roots_and_trailing_text_are_bad_xml() {
    let multiple = parse_str("<packages></packages><packages></packages>");
    assert_eq!(multiple.status, ParseStatus::BadXml);
    let error = multiple.error.as_ref().expect("error");
    assert_eq!(error.code, "MULTIPLE_ROOTS");

    let trailing = parse_str("<packages></packages>junk");
    assert_eq!(trailing.status, ParseStatus::BadXml);
    let error = trailing.error.as_ref().expect("error");
    assert_eq!(error.code, "TEXT_OUTSIDE_ROOT");
}

#[test]
fn nested_packages_are_not_counted_as_root_packages() {
    let xml = "<packages>\n\
  <sigs><package name=\"com.nested\"/></sigs>\n\
  <package name=\"com.top\"/>\n\
</packages>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.packages.len(), 1);
    assert_eq!(report.packages[0].name, "com.top");
    assert_eq!(report.nested_package_elements, 1);
    assert_eq!(report.ignored_root_children, 1);
    assert!(report.has_limitation("nested_package_ignored"));
}

#[test]
fn unnamed_package_is_skipped() {
    let report = parse_str("<packages><package codePath=\"/x\"/></packages>");
    assert_eq!(report.status, ParseStatus::Ok);
    assert!(report.packages.is_empty());
    assert_eq!(report.unnamed_packages, 1);
    assert!(report.has_limitation("unnamed_package_skipped"));
}

#[test]
fn multiple_headers_keep_first_value_and_flag_conflict() {
    let xml = "<packages>\n\
  <last-platform-version internal=\"22\" external=\"22\"/>\n\
  <last-platform-version internal=\"23\" external=\"23\"/>\n\
</packages>";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.header_elements, 2);
    assert!(report.header_conflict);
    let header = report.header.as_ref().expect("header");
    assert_eq!(header.internal.as_deref(), Some("22"));
    assert!(report.has_limitation("multiple_last_platform_version"));
    assert!(report.has_limitation("last_platform_version_conflict"));
}

// ---------------------------------------------------------------------------
// 编码 / 上限
// ---------------------------------------------------------------------------

#[test]
fn utf16_and_android_binary_xml_are_rejected() {
    let utf16 = parse_bytes(&[0xFF, 0xFE, b'<', 0, b'p', 0]);
    assert_eq!(utf16.status, ParseStatus::UnsupportedEncoding);
    assert_eq!(utf16.error.as_ref().unwrap().code, "UTF16_NOT_SUPPORTED");
    assert!(utf16.has_limitation("utf16_not_supported"));

    let binary = parse_bytes(&[0x03, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(binary.status, ParseStatus::UnsupportedEncoding);
    assert_eq!(
        binary.error.as_ref().unwrap().code,
        "BINARY_XML_NOT_SUPPORTED"
    );
    assert!(binary.has_limitation("binary_xml_not_supported"));
}

#[test]
fn invalid_utf8_is_rejected_with_offset() {
    let bytes = [b'<', b'p', b'a', 0xFF, b'>'];
    let report = parse_bytes(&bytes);
    assert_eq!(report.status, ParseStatus::UnsupportedEncoding);
    let error = report.error.as_ref().expect("error");
    assert_eq!(error.code, "INVALID_UTF8");
    assert_eq!(error.position.offset, 3);
    assert_eq!(error.position.line, 1);
    assert!(report.has_limitation("invalid_utf8"));
}

#[test]
fn oversized_input_is_rejected_without_parsing() {
    let report = too_large_report(MAX_INPUT_BYTES + 1);
    assert_eq!(report.status, ParseStatus::InputTooLarge);
    assert!(report.packages.is_empty());
    assert_eq!(report.error.as_ref().unwrap().code, "INPUT_TOO_LARGE");
    assert!(report.has_limitation("input_too_large"));
}

// ---------------------------------------------------------------------------
// 渲染
// ---------------------------------------------------------------------------

#[test]
fn render_contains_platform_package_error_and_summary_nodes() {
    let report = parse_str(NORMAL);
    let result = build_result(&report, "packages.xml", "file-42", NORMAL.len() as u64);

    let node_types: Vec<&str> = result
        .nodes
        .iter()
        .map(|node| node.node_type.as_str())
        .collect();
    assert_eq!(
        node_types,
        vec![NODE_PLATFORM, NODE_PACKAGE, NODE_ERROR, NODE_SUMMARY]
    );
    assert_eq!(result.nodes.len(), result_nodes().len());

    assert_eq!(artifacts_of(&result, NODE_PLATFORM).len(), 1);
    assert_eq!(artifacts_of(&result, NODE_PACKAGE).len(), 3);
    assert_eq!(artifacts_of(&result, NODE_ERROR).len(), 0);
    assert_eq!(artifacts_of(&result, NODE_SUMMARY).len(), 1);

    let platform = artifacts_of(&result, NODE_PLATFORM)[0];
    assert_eq!(attr_text(platform, KEY_INTERNAL).as_deref(), Some("33"));
    assert_eq!(attr_text(platform, KEY_EXTERNAL).as_deref(), Some("33"));
    assert_eq!(
        attr_text(platform, KEY_FINGERPRINT).as_deref(),
        Some("synthetic/fingerprint")
    );

    let alpha = artifacts_of(&result, NODE_PACKAGE)
        .into_iter()
        .find(|artifact| attr_text(artifact, KEY_NAME).as_deref() == Some("com.example.alpha"))
        .expect("alpha artifact");
    assert_eq!(attr_text(alpha, KEY_VERSION).as_deref(), Some("42"));
    assert_eq!(
        attr_text(alpha, KEY_CODE_PATH).as_deref(),
        Some("/data/app/alpha/base.apk")
    );
    assert_eq!(
        attr_text(alpha, KEY_INSTALLER).as_deref(),
        Some("com.android.vending")
    );
    assert_eq!(attr_text(alpha, KEY_OCCURRENCES).as_deref(), Some("1"));
    assert_eq!(
        attr_text(alpha, KEY_UPDATED_OCCURRENCES).as_deref(),
        Some("0")
    );
    assert_eq!(attr_text(alpha, KEY_DUPLICATE).as_deref(), Some("false"));
    assert_eq!(
        attr_text(alpha, KEY_CONFLICTING_FIELDS).as_deref(),
        Some("-")
    );
    assert_eq!(
        attr_text(alpha, KEY_SOURCE_FILE).as_deref(),
        Some("packages.xml")
    );
    assert_eq!(
        attr_text(alpha, KEY_SOURCE_FILE_ID).as_deref(),
        Some("file-42")
    );

    let summary = artifacts_of(&result, NODE_SUMMARY)[0];
    let limitations = attr_text(summary, KEY_LIMITATIONS).expect("limitations");
    assert!(limitations.contains("text_xml_only"));
    assert!(limitations.contains("missing_version_attribute"));
    assert!(result
        .summary
        .as_deref()
        .unwrap_or("")
        .contains("唯一包 3 个"));
}

#[test]
fn render_duplicate_package_reports_conflicts() {
    let xml = "<packages>\n\
  <package name=\"com.dup\" codePath=\"/first\" version=\"1\"/>\n\
  <package name=\"com.dup\" codePath=\"/second\" version=\"2\"/>\n\
</packages>";
    let report = parse_str(xml);
    let result = build_result(&report, "dup.xml", "f", xml.len() as u64);
    let package = artifacts_of(&result, NODE_PACKAGE)[0];
    assert_eq!(attr_text(package, KEY_OCCURRENCES).as_deref(), Some("2"));
    assert_eq!(attr_text(package, KEY_DUPLICATE).as_deref(), Some("true"));
    assert_eq!(
        attr_text(package, KEY_CONFLICTING_FIELDS).as_deref(),
        Some("version,codePath")
    );
    assert_eq!(attr_text(package, KEY_CODE_PATH).as_deref(), Some("/first"));
}

#[test]
fn render_bad_xml_has_error_artifact_and_zero_packages() {
    let xml = "<packages>\n  <package name=\"com.a\">\n</packages>\n";
    let report = parse_str(xml);
    let result = build_result(&report, "bad.xml", "bad-1", xml.len() as u64);

    assert_eq!(artifacts_of(&result, NODE_PACKAGE).len(), 0);
    assert_eq!(artifacts_of(&result, NODE_PLATFORM).len(), 0);
    let errors = artifacts_of(&result, NODE_ERROR);
    assert_eq!(errors.len(), 1);
    assert_eq!(
        attr_text(errors[0], KEY_ERROR_CODE).as_deref(),
        Some("BAD_XML")
    );
    assert!(result
        .summary
        .as_deref()
        .unwrap_or("")
        .contains("返回 0 个包"));

    let summary = artifacts_of(&result, NODE_SUMMARY)[0];
    let limitations = attr_text(summary, KEY_LIMITATIONS).expect("limitations");
    assert!(limitations.contains("bad_xml_zero_results"));
    assert!(limitations.contains("text_xml_only"));
}

#[test]
fn render_empty_document_has_summary_only() {
    let report = parse_str("");
    let result = build_result(&report, "empty.xml", "e", 0);
    assert_eq!(result.artifacts.len(), 1);
    assert_eq!(result.artifacts[0].artifact_type, NODE_SUMMARY);
    assert_eq!(attr_text(&result.artifacts[0], KEY_ERROR_CODE), None);
    assert!(summary_line(&report).contains("0 个包"));
}

#[test]
fn static_limitations_are_always_listed() {
    let limitations = static_limitations();
    assert!(limitations
        .iter()
        .any(|item| item.code == "read_only_no_network"));
    assert!(limitations.iter().any(|item| item.code == "no_timeline"));
    assert!(limitations.iter().any(|item| item.code == "bounded_output"));
}

// ---------------------------------------------------------------------------
// 健壮性 / 清单一致性
// ---------------------------------------------------------------------------

#[test]
fn parser_does_not_panic_on_malformed_inputs() {
    let snippets = [
        "",
        "<",
        "<packages",
        "<packages>",
        "<packages><package",
        "<packages><package name='x'",
        "<packages><package name='x' ",
        "<packages><!--",
        "<packages><![CDATA[",
        "<packages></packages",
        "<?xml",
        "<!DOCTYPE",
        "<packages><package name=\"a\"/></packages",
        "<packages>\0</packages>",
        "<packages><package name=\"a\"/><package name=\"a\"/></packages>",
    ];
    for snippet in snippets {
        let report = parse_str(snippet);
        // 只要求返回一个状态；不 panic、不挂起。
        assert!(!report.status.as_str().is_empty());
    }

    let byte_samples: [&[u8]; 6] = [
        b"",
        b"<packages/>",
        &[0x00],
        &[0xFF, 0xFE],
        &[0x03, 0x00, 0x08, 0x00],
        &[0x3C, 0xFF, 0x3E],
    ];
    for bytes in byte_samples {
        let _ = parse_bytes(bytes);
    }
}

#[test]
fn manifest_matches_code_constants() {
    let manifest = include_str!("../plugin.toml");
    assert!(manifest.contains("id = \"org.ljnljn.android-packages\""));
    assert!(manifest.contains("name = \"Android packages.xml 软件包清单\""));
    assert!(manifest.contains("version = \"0.1.0\""));
    assert!(manifest.contains("protocol_min = 1"));
    assert!(manifest.contains("protocol_max = 1"));
    assert!(manifest.contains("permissions = [\"emit_artifacts\"]"));
    assert!(manifest.contains("isolation = \"process\""));
    assert!(manifest.contains("path = \"ljnljn_kb_android_packages_xml_2d5e9b71\""));
    assert!(manifest.contains("analyzer = \"ljnljn.android-packages.file\""));
    assert!(manifest.contains("extensions = [\"xml\"]"));
    assert!(manifest.contains("type = \"file_analyzer\""));
    assert_eq!(ANALYZER_FILE, "ljnljn.android-packages.file");
    assert!(identity_matches(PLUGIN_ID, "0.1.0"));
    assert!(!identity_matches("org.example.other", "0.1.0"));
    assert!(!identity_matches(PLUGIN_ID, "9.9.9"));
}
