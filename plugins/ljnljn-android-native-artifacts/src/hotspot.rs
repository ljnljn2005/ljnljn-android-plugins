//! Wi-Fi hotspot (SoftAP) configuration.
//!
//! `softap.conf` stores the SSID length at byte 5 and the passphrase after the
//! last NUL; `WifiConfigStoreSoftAp.xml` stores the same values as named XML
//! fields. Only those two known layouts are read.

use lovelycarve_plugin::{FileRef, Json, Result, Run, Value};
use mobile_core::{ArtifactFile, ArtifactRecord};
use roxmltree::Document;

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const PATTERNS: &[&str] = &[
    "**/misc/wifi/softap.conf",
    "**/apexdata/com.android.wifi/WifiConfigStoreSoftAp.xml",
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
        let source = file
            .view_path()
            .map(str::to_owned)
            .unwrap_or_else(|| file.name().to_owned());
        let result = if source.ends_with(".conf") {
            parse_conf(run, &file, &source)
        } else {
            parse_xml(run, &file, &source)
        };
        let Some((ssid, passphrase, security)) = result? else {
            continue;
        };
        let data = vec![
            (Value::from("ssid"), Value::from(ssid.as_str())),
            (Value::from("passphrase"), Value::from(passphrase.as_str())),
            (Value::from("security_type"), Value::from(security.as_str())),
        ];
        run.record(&ArtifactRecord {
            platform: "android".to_owned(),
            artifact: "android_wifi_hotspot".to_owned(),
            source: source.clone(),
            record_key: ssid.clone(),
            time: None,
            title: ssid.clone(),
            data: Json(Value::Map(data)),
        })
        .from(&file)
        .note("SoftAP")
        .emit()?;
        run.record(&ArtifactFile {
            platform: "android".to_owned(),
            artifact: "android_wifi_hotspot".to_owned(),
            source,
            status: "ok".to_owned(),
            rows: 1,
            detail: "softap".to_owned(),
        })
        .from(&file)
        .emit()?;
        files += 1;
        rows += 1;
    }
    if files == 0 {
        run.warn("HOTSPOT_NOT_FOUND", "未找到 SoftAP 配置")?;
    }
    run.set_summary(format!("android_wifi_hotspot：{files} 个文件，{rows} 条配置"));
    Ok(())
}

fn parse_conf(
    run: &Run,
    file: &FileRef,
    source: &str,
) -> Result<Option<(String, String, String)>> {
    let bytes = match file.read_all(MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            run.warn_file(file, &error.code, error.message)?;
            return Ok(None);
        }
    };
    if bytes.len() < 6 {
        run.warn_file(
            file,
            "HOTSPOT_FORMAT",
            format!("{source}: 文件过短，无法读取 SSID 长度"),
        )?;
        return Ok(None);
    }
    let length = bytes[5] as usize;
    if 6 + length > bytes.len() {
        run.warn_file(
            file,
            "HOTSPOT_FORMAT",
            format!("{source}: SSID 长度超出文件范围"),
        )?;
        return Ok(None);
    }
    let ssid = String::from_utf8_lossy(&bytes[6..6 + length]).into_owned();
    if ssid.is_empty() {
        return Ok(None);
    }
    let pass_start = bytes
        .iter()
        .rposition(|byte| *byte == 0)
        .map(|index| index + 2)
        .unwrap_or(usize::MAX);
    let passphrase = if pass_start > 6 + length && pass_start < bytes.len() {
        String::from_utf8_lossy(&bytes[pass_start..]).into_owned()
    } else {
        String::new()
    };
    Ok(Some((ssid, passphrase, String::new())))
}

fn parse_xml(
    run: &Run,
    file: &FileRef,
    source: &str,
) -> Result<Option<(String, String, String)>> {
    let text = match file.read_text(MAX_BYTES) {
        Ok(text) => text,
        Err(error) => {
            run.warn_file(file, &error.code, error.message)?;
            return Ok(None);
        }
    };
    let cleaned: String = text
        .trim_start_matches('\u{feff}')
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect();
    let doc = match Document::parse(&cleaned) {
        Ok(doc) => doc,
        Err(error) => {
            run.warn_file(
                file,
                "HOTSPOT_XML",
                format!("{source}: XML 无法解析：{error}"),
            )?;
            return Ok(None);
        }
    };
    let mut ssid = String::new();
    let mut passphrase = String::new();
    let mut security = String::new();
    for node in doc.descendants().filter(|node| node.has_tag_name("SoftAp")) {
        for element in node.descendants().filter(|node| node.is_element()) {
            let Some(name) = element.attribute("name") else {
                continue;
            };
            match name {
                "SSID" | "WifiSsid" => {
                    ssid = element.text().unwrap_or("").to_owned();
                }
                "SecurityType" => {
                    security = element.attribute("value").unwrap_or("").to_owned();
                }
                "Passphrase" => {
                    passphrase = element.text().unwrap_or("").to_owned();
                }
                _ => {}
            }
        }
    }
    if ssid.is_empty() {
        return Ok(None);
    }
    Ok(Some((ssid, passphrase, security)))
}
