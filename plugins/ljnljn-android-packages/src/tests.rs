//! Synthetic tests: hand-written XML only, no real case or device data.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lovelycarve_plugin::testing::{Fixture, Harness};

use crate::model::{ElementKind, ParseStatus};
use crate::parser::{parse_bytes, parse_str, static_limitations, too_large_report};
use crate::{MAX_OUTPUT_PACKAGES, MAX_XML_BYTES};

const NORMAL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\" ?>\n\
<!-- synthetic packages.xml for tests -->\n\
<packages>\n\
  <last-platform-version internal=\"33\" external=\"33\" fingerprint=\"synthetic/fingerprint\" />\n\
  <package name=\"com.example.alpha\" codePath=\"/data/app/alpha/base.apk\" version=\"42\" installer=\"com.android.vending\" />\n\
  <package name=\"com.example.beta\" codePath=\"/system/app/Beta/Beta.apk\" />\n\
  <updated-package name=\"com.example.gamma\" codePath=\"/data/app/gamma/base.apk\" version=\"7\" />\n\
</packages>\n";

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
            "ljnljn-android-packages-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, bytes).unwrap();
        path
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

fn run_files(paths: &[&Path]) -> lovelycarve_plugin::testing::TestRun {
    harness()
        .run_with(
            "packages",
            Fixture::files(paths),
            Default::default(),
            Default::default(),
        )
        .expect("run")
}

fn package<'a>(
    run: &'a lovelycarve_plugin::testing::TestRun,
    name: &str,
) -> &'a lovelycarve_plugin::testing::TestRecord {
    run.of_type("android.packages.package")
        .into_iter()
        .find(|record| record.text("name") == Some(name))
        .unwrap_or_else(|| panic!("package {name} not emitted"))
}

#[test]
fn harness_emits_report_platform_and_packages_with_sources() {
    let fixture = TempDir::new("normal");
    let path = fixture.file("packages.xml", NORMAL.as_bytes());
    let run = run_files(&[&path]);

    assert_eq!(run.count("android.packages.report"), 1);
    assert_eq!(run.count("android.packages.platform"), 1);
    assert_eq!(run.count("android.packages.package"), 3);

    let report = &run.of_type("android.packages.report")[0];
    assert_eq!(report.text("status"), Some("ok"));
    assert_eq!(report.text("root_name"), Some("packages"));
    assert_eq!(report.int("package_elements"), Some(2));
    assert_eq!(report.int("updated_package_elements"), Some(1));
    assert_eq!(report.int("unique_packages"), Some(3));
    assert_eq!(report.int("duplicate_packages"), Some(0));
    assert_eq!(report.sources.len(), 1);
    assert_eq!(report.sources[0].root.as_deref(), Some("files"));
    assert!(report.sources[0]
        .note
        .as_deref()
        .unwrap_or("")
        .contains("status=ok"));

    let platform = &run.of_type("android.packages.platform")[0];
    assert_eq!(platform.text("internal"), Some("33"));
    assert_eq!(platform.text("external"), Some("33"));
    assert_eq!(platform.text("fingerprint"), Some("synthetic/fingerprint"));
    assert!(platform.int("line").unwrap_or(0) >= 3);

    let alpha = package(&run, "com.example.alpha");
    assert_eq!(alpha.text("version"), Some("42"));
    assert_eq!(alpha.text("code_path"), Some("/data/app/alpha/base.apk"));
    assert_eq!(alpha.text("installer"), Some("com.android.vending"));
    assert_eq!(alpha.text("first_kind"), Some("package"));
    assert_eq!(alpha.int("occurrences"), Some(1));
    assert_eq!(alpha.text("duplicate"), None); // bool field, not text
    assert_eq!(
        alpha.get("duplicate").and_then(|value| value.as_bool()),
        Some(false)
    );

    let gamma = package(&run, "com.example.gamma");
    assert_eq!(gamma.text("first_kind"), Some("updated_package"));
    assert_eq!(gamma.int("updated_occurrences"), Some(1));

    assert!(run
        .summary
        .as_deref()
        .unwrap_or("")
        .contains("唯一包合计 3 个"));
}

#[test]
fn duplicate_names_keep_first_values_and_list_conflicts() {
    let fixture = TempDir::new("duplicate");
    let xml = "<packages>\n\
  <package name=\"com.dup\" codePath=\"/first\" version=\"1\"/>\n\
  <updated-package name=\"com.dup\" codePath=\"/first\" installer=\"com.store\"/>\n\
  <package name=\"com.dup\" codePath=\"/second\" version=\"2\"/>\n\
  <package name=\"com.other\"/>\n\
</packages>\n";
    let path = fixture.file("packages.xml", xml.as_bytes());
    let run = run_files(&[&path]);

    assert_eq!(run.count("android.packages.package"), 2);
    let record = package(&run, "com.dup");
    assert_eq!(record.int("occurrences"), Some(3));
    assert_eq!(record.int("updated_occurrences"), Some(1));
    assert_eq!(record.text("code_path"), Some("/first"));
    assert_eq!(record.text("version"), Some("1"));
    assert_eq!(record.text("installer"), Some("com.store"));
    assert_eq!(
        record.get("duplicate").and_then(|v| v.as_bool()),
        Some(true)
    );
    let conflicts = record
        .get("conflicting_fields")
        .and_then(|v| v.as_array())
        .unwrap();
    let conflicts: Vec<&str> = conflicts.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(conflicts, vec!["version", "codePath"]);
    assert!(record.sources[0]
        .note
        .as_deref()
        .unwrap_or("")
        .contains("第 2 行"));
}

#[test]
fn bad_xml_is_a_warning_and_a_zero_result_report() {
    let fixture = TempDir::new("bad");
    let path = fixture.file(
        "bad.xml",
        b"<packages>\n  <package name=\"com.a\">\n</packages>\n",
    );
    let run = run_files(&[&path]);

    assert_eq!(run.count("android.packages.package"), 0);
    assert_eq!(run.count("android.packages.platform"), 0);
    let report = &run.of_type("android.packages.report")[0];
    assert_eq!(report.text("status"), Some("bad_xml"));
    assert_eq!(report.text("error_code"), Some("BAD_XML"));
    assert!(report.int("error_line").unwrap_or(0) >= 1);
    assert!(run.warnings.iter().any(|(code, _, _)| code == "BAD_XML"));
}

#[test]
fn not_packages_root_is_reported_without_records() {
    let fixture = TempDir::new("manifest");
    let path = fixture.file(
        "AndroidManifest.xml",
        b"<manifest><package name=\"com.example\"/></manifest>",
    );
    let run = run_files(&[&path]);

    assert_eq!(run.count("android.packages.package"), 0);
    let report = &run.of_type("android.packages.report")[0];
    assert_eq!(report.text("status"), Some("not_packages"));
    assert_eq!(report.text("root_name"), Some("manifest"));
    assert!(run
        .warnings
        .iter()
        .any(|(code, _, _)| code == "NOT_PACKAGES"));
}

#[test]
fn utf16_and_android_binary_xml_are_reported_as_unsupported() {
    let fixture = TempDir::new("encoding");
    let utf16 = fixture.file("utf16.xml", &[0xFF, 0xFE, b'<', 0, b'p', 0]);
    let binary = fixture.file(
        "binary.xml",
        &[0x03, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00],
    );
    let run = run_files(&[&utf16, &binary]);

    let reports = run.of_type("android.packages.report");
    assert_eq!(reports.len(), 2);
    assert!(reports
        .iter()
        .all(|report| report.text("status") == Some("unsupported_encoding")));
    let codes: Vec<&str> = reports
        .iter()
        .filter_map(|report| report.text("error_code"))
        .collect();
    assert!(codes.contains(&"UTF16_NOT_SUPPORTED"));
    assert!(codes.contains(&"BINARY_XML_NOT_SUPPORTED"));
}

#[test]
fn empty_document_has_an_explicit_empty_status() {
    let fixture = TempDir::new("empty");
    let path = fixture.file("empty.xml", b"   \n<!-- only a comment -->\n");
    let run = run_files(&[&path]);

    let report = &run.of_type("android.packages.report")[0];
    assert_eq!(report.text("status"), Some("empty"));
    assert_eq!(report.int("unique_packages"), Some(0));
    assert!(run
        .warnings
        .iter()
        .any(|(code, _, _)| code == "EMPTY_DOCUMENT"));
}

#[test]
fn parser_deduplicates_and_tolerates_namespaces_and_escapes() {
    let xml = "<?xml version=\"1.0\"?>\n\
<!-- outer comment -->\n\
<p:packages xmlns:p=\"urn:p\" xmlns:android=\"urn:android\">\n\
  <p:last-platform-version internal=\"22\" external=\"22\" fingerprint=\"f&amp;g\"/>\n\
  <p:package name=\"com.example.a&amp;b\" codePath=\"/data/a&#47;b\" version='1.0'/>\n\
  <p:updated-package name='com.single' codePath='/system/single'/>\n\
</p:packages>\n";
    let report = parse_str(xml);
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.comments, 1);
    assert_eq!(report.namespaces.len(), 2);
    assert_eq!(
        report.header.as_ref().unwrap().fingerprint.as_deref(),
        Some("f&g")
    );
    let escaped = report
        .packages
        .iter()
        .find(|entry| entry.name == "com.example.a&b")
        .expect("escaped package");
    assert_eq!(escaped.code_path.as_deref(), Some("/data/a/b"));
    let single = report
        .packages
        .iter()
        .find(|entry| entry.name == "com.single")
        .expect("single-quoted package");
    assert_eq!(single.first_kind, ElementKind::UpdatedPackage);
}

#[test]
fn parser_rejects_custom_entities_and_handles_case_variants() {
    let rejected = parse_str(
        "<!DOCTYPE packages [<!ENTITY foo \"bar\">]><packages><package name=\"&foo;\"/></packages>",
    );
    assert_eq!(rejected.status, ParseStatus::BadXml);

    let report = parse_str(
        "<PACKAGES><LAST-PLATFORM-VERSION INTERNAL=\"22\"/><PACKAGE NAME=\"com.Case\"/></PACKAGES>",
    );
    assert_eq!(report.status, ParseStatus::Ok);
    assert_eq!(report.packages[0].name, "com.Case");
    assert_eq!(
        report.header.as_ref().unwrap().internal.as_deref(),
        Some("22")
    );
}

#[test]
fn parser_does_not_panic_on_malformed_inputs() {
    for snippet in [
        "",
        "<",
        "<packages",
        "<packages>",
        "<packages><package name='x'",
        "<packages><!--",
        "<packages><![CDATA[",
        "<packages></packages",
        "<?xml",
        "<!DOCTYPE",
        "<packages>\u{0}</packages>",
    ] {
        let report = parse_str(snippet);
        assert!(!report.status.as_str().is_empty());
    }
    for bytes in [
        b"".as_slice(),
        b"<packages/>",
        &[0x00],
        &[0xFF, 0xFE],
        &[0x03, 0x00, 0x08, 0x00],
        &[0x3C, 0xFF, 0x3E],
    ] {
        let _ = parse_bytes(bytes);
    }
}

#[test]
fn too_large_report_keeps_the_v1_limit() {
    let report = too_large_report(MAX_XML_BYTES + 1);
    assert_eq!(report.status, ParseStatus::InputTooLarge);
    assert!(report.packages.is_empty());
    assert!(report.has_limitation("input_too_large"));
}

#[test]
fn static_limitations_and_output_cap_are_documented() {
    let limitations = static_limitations();
    assert!(limitations
        .iter()
        .any(|item| item.code == "read_only_no_network"));
    assert!(limitations.iter().any(|item| item.code == "no_timeline"));
    assert!(limitations.iter().any(|item| item.code == "bounded_output"));
    assert_eq!(MAX_OUTPUT_PACKAGES, 3_000);
}

#[test]
fn manifest_declares_the_v2_identity() {
    let manifest = include_str!("../plugin.toml");
    assert!(manifest.contains("id = \"org.ljnljn.android-packages\""));
    assert!(manifest.contains("protocol = 2"));
    assert!(manifest.contains("library = \"ljnljn_android_packages\""));
    assert!(manifest.contains("id = \"packages\""));
}
