//! XML artifacts: account sync authorities and saved Wi-Fi profiles.
//!
//! Both files are parsed as XML trees. No XPath or user text is executed; the
//! readers walk known element and attribute names only.

use lovelycarve_plugin::{FileRef, Json, Result, Run, Timestamp, Value};
use mobile_core::{ArtifactFile, ArtifactRecord};
use roxmltree::Document;

const MAX_XML_BYTES: u64 = 32 * 1024 * 1024;
const ACCOUNT_PATTERNS: &[&str] = &["**/system/sync/accounts.xml"];
const WIFI_PATTERNS: &[&str] = &[
    "**/misc/wifi/WifiConfigStore.xml",
    "**/apexdata/com.android.wifi/WifiConfigStore.xml",
];

pub fn accounts_evidence(run: &Run) -> Result<()> {
    run_evidence(run, "android_accounts", ACCOUNT_PATTERNS, emit_accounts)
}

pub fn wifi_evidence(run: &Run) -> Result<()> {
    run_evidence(run, "android_wifi", WIFI_PATTERNS, emit_wifi)
}

pub fn selected(run: &Run) -> Result<()> {
    let mut files = 0usize;
    for file in run.inputs() {
        run.check_cancelled()?;
        let Some((text, source)) = load(run, file)? else {
            continue;
        };
        let cleaned = sanitize(&text);
        let Ok(doc) = Document::parse(&cleaned) else {
            run.warn_file(
                file,
                "XML_PARSE",
                format!("{source}: XML 无法解析，已跳过"),
            )?;
            continue;
        };
        let accounts = emit_accounts(run, file, &doc, &source)?;
        let wifi = emit_wifi(run, file, &doc, &source)?;
        if accounts + wifi == 0 {
            run.warn_file(
                file,
                "XML_NO_ARTIFACT",
                format!("{source}: 没有 authority 或 Network 元素"),
            )?;
            continue;
        }
        files += 1;
    }
    if files == 0 {
        run.warn("XML_NO_ARTIFACT", "所选 XML 没有可识别的账号或 WiFi 配置")?;
    }
    run.set_summary(format!("所选 XML：{files} 个文件"));
    Ok(())
}

type Emit = fn(&Run, &FileRef, &Document<'_>, &str) -> Result<usize>;

fn run_evidence(
    run: &Run,
    artifact: &str,
    patterns: &[&str],
    emit: Emit,
) -> Result<()> {
    let view = run.require_view()?;
    let mut expanded = Vec::new();
    for mount in view.mounts() {
        let prefix = mount.path.trim_matches('/');
        for pattern in patterns {
            expanded.push(if prefix.is_empty() {
                (*pattern).to_owned()
            } else {
                format!("{prefix}/{pattern}")
            });
        }
    }
    expanded.sort();
    expanded.dedup();
    let refs: Vec<&str> = expanded.iter().map(String::as_str).collect();
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
        let Some((text, source)) = load(run, &file)? else {
            continue;
        };
        if artifact == "android_accounts"
            && source.split('/').any(|part| part == "data_mirror")
        {
            continue;
        }
        let cleaned = sanitize(&text);
        let doc = match Document::parse(&cleaned) {
            Ok(doc) => doc,
            Err(error) => {
                run.warn_file(
                    &file,
                    "XML_PARSE",
                    format!("{source}: XML 无法解析：{error}"),
                )?;
                continue;
            }
        };
        let count = emit(run, &file, &doc, &source)?;
        rows += count;
        files += 1;
        run.record(&ArtifactFile {
            platform: "android".to_owned(),
            artifact: artifact.to_owned(),
            source,
            status: if count == 0 { "empty" } else { "ok" }.to_owned(),
            rows: count as i64,
            detail: "xml".to_owned(),
        })
        .from(&file)
        .emit()?;
    }
    if files == 0 {
        run.warn("XML_NOT_FOUND", format!("未找到 {artifact} 的 XML 配置"))?;
    }
    run.set_summary(format!("{artifact}：{files} 个文件，{rows} 条记录"));
    Ok(())
}

fn load(run: &Run, file: &FileRef) -> Result<Option<(String, String)>> {
    let source = source_path(file);
    match file.read_text(MAX_XML_BYTES) {
        Ok(text) => Ok(Some((text, source))),
        Err(error) => {
            run.warn_file(file, &error.code, error.message)?;
            Ok(None)
        }
    }
}

fn emit_accounts(
    run: &Run,
    file: &FileRef,
    doc: &Document<'_>,
    source: &str,
) -> Result<usize> {
    let mut emitted = 0usize;
    for node in doc.descendants().filter(|node| node.has_tag_name("authority")) {
        run.check_cancelled()?;
        let attribute = |name: &str| node.attribute(name);
        let account = attribute("account").unwrap_or("");
        let kind = attribute("type").unwrap_or("");
        let authority = attribute("authority").unwrap_or("");
        let id = attribute("id").unwrap_or("");
        let mut data = Vec::new();
        for (name, value) in [
            ("user", attribute("user")),
            ("account", attribute("account")),
            ("type", attribute("type")),
            ("authority", attribute("authority")),
            ("enabled", attribute("enabled")),
            ("syncable", attribute("syncable")),
            ("id", attribute("id")),
        ] {
            if let Some(value) = value {
                data.push((Value::from(name), Value::from(value)));
            }
        }
        let key = format!("{account}|{kind}|{authority}|{id}");
        let title = if authority.is_empty() {
            account.to_owned()
        } else {
            authority.to_owned()
        };
        run.record(&ArtifactRecord {
            platform: "android".to_owned(),
            artifact: "android_accounts".to_owned(),
            source: source.to_owned(),
            record_key: key,
            time: None,
            title,
            data: Json(Value::Map(data)),
        })
        .from(file)
        .note("authority")
        .emit()?;
        emitted += 1;
    }
    Ok(emitted)
}

fn emit_wifi(
    run: &Run,
    file: &FileRef,
    doc: &Document<'_>,
    source: &str,
) -> Result<usize> {
    let mut emitted = 0usize;
    for network in doc.descendants().filter(|node| node.has_tag_name("Network")) {
        run.check_cancelled()?;
        let mut data = Vec::new();
        let mut ssid = String::new();
        let mut security = String::new();
        let mut config_key = String::new();
        let mut last_connected = None;
        let mut updated = None;
        let mut created = None;

        for element in network.descendants().filter(|node| node.is_element()) {
            let Some(name) = element.attribute("name") else {
                continue;
            };
            match name {
                "ConfigKey" => {
                    config_key = element.text().unwrap_or("").to_owned();
                    let parts: Vec<&str> = config_key.split('"').collect();
                    security = parts.get(2).copied().unwrap_or("").to_owned();
                }
                "SSID" => {
                    ssid = element
                        .text()
                        .unwrap_or("")
                        .trim_matches('"')
                        .to_owned();
                }
                "CaptivePortal" => {
                    data.push((
                        Value::from(name),
                        Value::from(element.attribute("value").unwrap_or("")),
                    ));
                }
                "semCreationTime" | "semUpdateTime" | "LastConnectedTime" => {
                    let raw = element.attribute("value").unwrap_or("");
                    data.push((Value::from(name), Value::from(raw)));
                    let timestamp = raw
                        .parse::<i64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .map(Timestamp::from_unix_millis);
                    match name {
                        "LastConnectedTime" => last_connected = timestamp,
                        "semUpdateTime" => updated = timestamp,
                        _ => created = timestamp,
                    }
                }
                "PreSharedKey" | "WEPKeys" | "LoginUrl" | "IpAssignment"
                | "Identity" | "Password" | "DefaultGwMacAddress" => {
                    data.push((
                        Value::from(name),
                        Value::from(element.text().unwrap_or("")),
                    ));
                }
                _ => {}
            }
        }

        if data.is_empty() && ssid.is_empty() {
            continue;
        }
        data.push((Value::from("security_mode"), Value::from(security.as_str())));
        data.push((Value::from("ssid"), Value::from(ssid.as_str())));
        data.push((
            Value::from("config_key"),
            Value::from(config_key.as_str()),
        ));
        let title = if ssid.is_empty() {
            config_key.clone()
        } else {
            ssid.clone()
        };
        let key = format!("{ssid}|{security}|{config_key}");
        run.record(&ArtifactRecord {
            platform: "android".to_owned(),
            artifact: "android_wifi".to_owned(),
            source: source.to_owned(),
            record_key: key,
            time: last_connected.or(updated).or(created),
            title,
            data: Json(Value::Map(data)),
        })
        .from(file)
        .note("Network")
        .emit()?;
        emitted += 1;
    }
    Ok(emitted)
}

fn source_path(file: &FileRef) -> String {
    file.view_path()
        .map(str::to_owned)
        .unwrap_or_else(|| file.name().to_owned())
}

fn sanitize(text: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .chars()
        .map(|character| {
            if character.is_control()
                && !matches!(character, '\n' | '\r' | '\t')
            {
                ' '
            } else {
                character
            }
        })
        .collect()
}
