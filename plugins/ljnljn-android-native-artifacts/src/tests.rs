use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lovelycarve_plugin::testing::{Fixture, Harness, TestRecord, TestRun};
use lovelycarve_plugin::Value;
use rusqlite::Connection;

const UNIX_MS_2024: i64 = 1_704_067_200_000;
const UNIX_S_2024: i64 = 1_704_067_200;
const WEBKIT_2024: i64 = 13_348_540_800_000_000;

struct Root {
    path: PathBuf,
}

impl Root {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ljnljn-android-native-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    fn database(&self, relative: &str, sql: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(sql).unwrap();
        path
    }

    fn evidence(&self, analyzer: &str) -> TestRun {
        Harness::new(super::plugin)
            .run_with(
                analyzer,
                Fixture::evidence(&[("part:1", &self.path)]),
                BTreeMap::new(),
                BTreeMap::new(),
            )
            .expect("run")
    }

    fn selected(&self, analyzer: &str, path: &Path) -> TestRun {
        Harness::new(super::plugin)
            .run_with(
                analyzer,
                Fixture::files(&[path]),
                BTreeMap::new(),
                BTreeMap::new(),
            )
            .expect("run")
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn data<'a>(record: &'a TestRecord, key: &str) -> Option<&'a Value> {
    let Value::Map(map) = record.get("data")? else {
        return None;
    };
    map.iter()
        .find(|(name, _)| name.as_str() == Some(key))
        .map(|(_, value)| value)
}

fn sms_fixture(root: &Root) -> PathBuf {
    root.database(
        "data/data/com.android.providers.telephony/databases/mmssms.db",
        &format!(
            "CREATE TABLE sms(_id INTEGER PRIMARY KEY, thread_id INTEGER, address TEXT, \
             date INTEGER, date_sent INTEGER, read INTEGER, type INTEGER, body TEXT, \
             service_center TEXT, subject TEXT);\
             INSERT INTO sms VALUES(1, 7, '+8613800000000', {UNIX_MS_2024}, 0, 1, 1, 'hello', NULL, NULL);"
        ),
    )
}

#[test]
fn sms_read_body_address_and_millisecond_time() {
    let root = Root::new("sms");
    sms_fixture(&root);
    let run = root.evidence("android_sms");

    assert_eq!(run.count("mobile.artifact"), 1);
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("artifact"), Some("android_sms"));
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "body").and_then(Value::as_str),
        Some("hello")
    );
    assert_eq!(
        data(record, "address").and_then(Value::as_str),
        Some("+8613800000000")
    );
    assert_eq!(run.of_type("mobile.artifact.file")[0].text("status"), Some("ok"));
}

#[test]
fn downloads_read_title_and_path() {
    let root = Root::new("downloads");
    root.database(
        "data/data/com.android.providers.downloads/databases/downloads.db",
        &format!(
            "CREATE TABLE downloads(_id INTEGER PRIMARY KEY, lastmod INTEGER, title TEXT, \
             description TEXT, uri TEXT, _data TEXT, mimetype TEXT, notificationpackage TEXT, \
             current_bytes INTEGER, total_bytes INTEGER, status INTEGER, errorMsg TEXT, \
             etag TEXT, is_visible_in_downloads_ui INTEGER, deleted INTEGER);\
             INSERT INTO downloads VALUES(1, {UNIX_MS_2024}, 'file.pdf', '', 'https://example.com/f', \
             '/sdcard/Download/file.pdf', 'application/pdf', 'com.android.chrome', 10, 10, 8, NULL, 'e', 1, 0);"
        ),
    );
    let run = root.evidence("android_downloads");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "path").and_then(Value::as_str),
        Some("/sdcard/Download/file.pdf")
    );
}

#[test]
fn cookies_read_host_and_name() {
    let root = Root::new("cookies");
    root.database(
        "data/data/com.android.chrome/app_chrome/Default/Cookies",
        &format!(
            "CREATE TABLE cookies(creation_utc INTEGER, host_key TEXT, name TEXT, value TEXT, \
             path TEXT, expires_utc INTEGER, last_access_utc INTEGER, is_secure INTEGER, is_httponly INTEGER);\
             INSERT INTO cookies VALUES({WEBKIT_2024}, '.example.com', 'sid', 'abc', '/', 0, {WEBKIT_2024}, 1, 1);"
        ),
    );
    let run = root.evidence("android_cookies");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "host").and_then(Value::as_str),
        Some(".example.com")
    );
}

#[test]
fn autofill_reads_name_value() {
    let root = Root::new("autofill");
    root.database(
        "data/data/com.android.chrome/app_chrome/Default/Web Data",
        &format!(
            "CREATE TABLE autofill(date_created INTEGER, name TEXT, value TEXT, \
             date_last_used INTEGER, count INTEGER);\
             INSERT INTO autofill VALUES({UNIX_MS_2024}, 'email', 'alice@example.com', {UNIX_MS_2024}, 3);"
        ),
    );
    let run = root.evidence("android_autofill");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(
        data(record, "value").and_then(Value::as_str),
        Some("alice@example.com")
    );
}

#[test]
fn calendar_events_read_title() {
    let root = Root::new("calendar");
    root.database(
        "data/data/com.android.providers.calendar/databases/calendar.db",
        &format!(
            "CREATE TABLE Calendars(_id INTEGER PRIMARY KEY, calendar_displayName TEXT);\
             CREATE TABLE Events(_id INTEGER PRIMARY KEY, calendar_id INTEGER, dtstart INTEGER, \
             dtend INTEGER, eventTimezone TEXT, title TEXT, description TEXT, eventLocation TEXT, organizer TEXT);\
             INSERT INTO Calendars VALUES(1, 'Work');\
             INSERT INTO Events VALUES(1, 1, {UNIX_MS_2024}, {UNIX_MS_2024}, 'UTC', 'Meeting', '', 'Room 1', 'alice');"
        ),
    );
    let run = root.evidence("android_calendar");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(data(record, "calendar").and_then(Value::as_str), Some("Work"));
}

#[test]
fn keep_notes_read_fts_text() {
    let root = Root::new("keep");
    root.database(
        "data/data/com.google.android.keep/databases/keep.db",
        &format!(
            "CREATE TABLE tree_entity(_id INTEGER PRIMARY KEY, time_created INTEGER, \
             time_last_updated INTEGER, user_edited_timestamp INTEGER, title TEXT, last_modifier_email TEXT);\
             CREATE TABLE text_search_note_content_content(docid INTEGER, c0text TEXT);\
             INSERT INTO tree_entity VALUES(1, {UNIX_MS_2024}, {UNIX_MS_2024}, 0, 'Todo', 'a@example.com');\
             INSERT INTO text_search_note_content_content VALUES(1, 'buy milk');"
        ),
    );
    let run = root.evidence("android_keep_notes");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(data(record, "text").and_then(Value::as_str), Some("buy milk"));
}

#[test]
fn maps_places_read_coordinates() {
    let root = Root::new("maps");
    root.database(
        "data/data/com.google.android.apps.maps/databases/gmm_myplaces.db",
        &format!(
            "CREATE TABLE sync_item(rowid INTEGER PRIMARY KEY, key_string TEXT, latitude INTEGER, \
             longitude INTEGER, sync_item BLOB, timestamp INTEGER);\
             INSERT INTO sync_item VALUES(1, 'Home', 31230400, 121473700, NULL, {UNIX_MS_2024});"
        ),
    );
    let run = root.evidence("android_maps_places");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "latitude").and_then(Value::as_f64),
        Some(31.2304)
    );
}

#[test]
fn top_sites_read_url_and_rank() {
    let root = Root::new("top-sites");
    root.database(
        "data/data/com.android.chrome/app_chrome/Default/Top Sites",
        "CREATE TABLE top_sites(url TEXT, url_rank INTEGER, title TEXT, redirects INTEGER);\
         INSERT INTO top_sites VALUES('https://example.com', 1, 'Example', 0);",
    );
    let run = root.evidence("android_top_sites");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(
        data(record, "url").and_then(Value::as_str),
        Some("https://example.com")
    );
}

#[test]
fn bookmarks_json_walks_url_nodes() {
    let root = Root::new("bookmarks");
    let path = root.write(
        "data/data/com.android.chrome/app_chrome/Default/Bookmarks",
        &format!(
            r#"{{"roots":{{"bookmark_bar":{{"type":"folder","name":"Bookmarks bar","children":[{{"type":"url","name":"Example","url":"https://example.com","id":"1","date_added":"{WEBKIT_2024}"}}]}},"other":{{"type":"folder","name":"Other","children":[]}}}}}}"#
        ),
    );
    let run = root.evidence("android_chrome_bookmarks");
    assert_eq!(run.count("mobile.artifact"), 1);
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "url").and_then(Value::as_str),
        Some("https://example.com")
    );
    let selected = root.selected("android_selected_json", &path);
    assert_eq!(selected.count("mobile.artifact"), 1);
}

#[test]
fn login_data_reads_origin_and_username() {
    let root = Root::new("login-data");
    root.database(
        "data/data/com.android.chrome/app_chrome/Default/Login Data",
        &format!(
            "CREATE TABLE logins(origin_url TEXT, username_value TEXT, password_value BLOB, \
             date_created INTEGER, blacklisted_by_user INTEGER, signon_realm TEXT);\
             INSERT INTO logins VALUES('https://example.com', 'alice', X'763130', {WEBKIT_2024}, 0, 'https://example.com');"
        ),
    );
    let run = root.evidence("android_login_data");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "username").and_then(Value::as_str),
        Some("alice")
    );
    assert_eq!(
        data(record, "password_blob").and_then(Value::as_str),
        Some("<blob 3 bytes>")
    );
}

#[test]
fn credit_cards_join_metadata() {
    let root = Root::new("cards");
    root.database(
        "data/data/com.android.chrome/app_chrome/Default/Web Data",
        &format!(
            "CREATE TABLE masked_credit_cards(id TEXT PRIMARY KEY, name_on_card TEXT, network TEXT, \
             last_four TEXT, exp_month INTEGER, exp_year INTEGER, bank_name TEXT, nickname TEXT);\
             CREATE TABLE server_card_metadata(id TEXT, use_date INTEGER, use_count INTEGER);\
             INSERT INTO masked_credit_cards VALUES('c1', 'ALICE WANG', 'VISA', '4242', 12, 2030, 'Bank', 'Personal');\
             INSERT INTO server_card_metadata VALUES('c1', {WEBKIT_2024}, 4);"
        ),
    );
    let run = root.evidence("android_credit_cards");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(
        data(record, "last_four").and_then(Value::as_str),
        Some("4242")
    );
    assert_eq!(data(record, "use_count").and_then(Value::as_i64), Some(4));
}

#[test]
fn firefox_cookies_read_host_and_name() {
    let root = Root::new("firefox");
    root.database(
        "data/data/org.mozilla.firefox/files/mozilla/abc.default/cookies.sqlite",
        &format!(
            "CREATE TABLE moz_cookies(id INTEGER PRIMARY KEY, lastAccessed INTEGER, creationTime INTEGER, \
             host TEXT, name TEXT, value TEXT, expiry INTEGER, path TEXT);\
             INSERT INTO moz_cookies VALUES(1, {}, {}, '.example.com', 'sid', 'abc', 0, '/');",
            UNIX_MS_2024 * 1000,
            UNIX_MS_2024 * 1000
        ),
    );
    let run = root.evidence("android_firefox_cookies");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "host").and_then(Value::as_str),
        Some(".example.com")
    );
}

#[test]
fn wifi_hotspot_reads_softap_xml() {
    let root = Root::new("hotspot");
    root.write(
        "data/misc/apexdata/com.android.wifi/WifiConfigStoreSoftAp.xml",
        r#"<WifiConfigStoreData><SoftAp><string name="SSID">MyAP</string><string name="Passphrase">secret</string><int name="SecurityType" value="1" /></SoftAp></WifiConfigStoreData>"#,
    );
    let run = root.evidence("android_wifi_hotspot");
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("title"), Some("MyAP"));
    assert_eq!(
        data(record, "passphrase").and_then(Value::as_str),
        Some("secret")
    );
}

#[test]
fn mms_reads_seconds_timestamp_and_addresses() {
    let root = Root::new("mms");
    root.database(
        "data/data/com.android.providers.telephony/databases/mmssms.db",
        &format!(
            "CREATE TABLE pdu(_id INTEGER PRIMARY KEY, thread_id INTEGER, date INTEGER, \
             date_sent INTEGER, msg_box INTEGER, subject TEXT, ct_t TEXT);\
             CREATE TABLE addr(msg_id INTEGER, type INTEGER, address TEXT);\
             INSERT INTO pdu VALUES(1, 7, {UNIX_S_2024}, 0, 1, 'Photo', 'image/jpeg');\
             INSERT INTO addr VALUES(1, 0x89, '+8613700000000');\
             INSERT INTO addr VALUES(1, 0x97, '+8613800000000');"
        ),
    );
    let run = root.evidence("android_sms");

    let record = run
        .of_type("mobile.artifact")
        .into_iter()
        .find(|record| data(record, "subject").and_then(Value::as_str) == Some("Photo"))
        .expect("MMS row");
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "from_address").and_then(Value::as_str),
        Some("+8613700000000")
    );
    assert_eq!(
        data(record, "to_address").and_then(Value::as_str),
        Some("+8613800000000")
    );
}

#[test]
fn calls_read_type_and_duration() {
    let root = Root::new("calls");
    root.database(
        "data/data/com.android.providers.contacts/databases/calllog.db",
        &format!(
            "CREATE TABLE calls(_id INTEGER PRIMARY KEY, date INTEGER, number TEXT, \
             duration INTEGER, type INTEGER, geocoded_location TEXT, countryiso TEXT, \
             deleted INTEGER, phone_account_address TEXT, transcription TEXT);\
             INSERT INTO calls VALUES(1, {UNIX_MS_2024}, '+8613900000000', 65, 2, 'Shanghai', 'CN', 0, NULL, NULL);"
        ),
    );
    let run = root.evidence("android_calls");

    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "type").and_then(Value::as_i64),
        Some(2)
    );
    assert_eq!(
        data(record, "geocoded_location").and_then(Value::as_str),
        Some("Shanghai")
    );
}

#[test]
fn contacts_join_phone_and_email_rows() {
    let root = Root::new("contacts");
    root.database(
        "data/data/com.android.providers.contacts/databases/contacts2.db",
        "CREATE TABLE contacts(_id INTEGER PRIMARY KEY);\
         CREATE TABLE raw_contacts(_id INTEGER PRIMARY KEY, contact_id INTEGER, display_name TEXT);\
         CREATE TABLE data(_id INTEGER PRIMARY KEY, raw_contact_id INTEGER, mimetype_id INTEGER, data1 TEXT);\
         CREATE TABLE mimetypes(_id INTEGER PRIMARY KEY, mimetype TEXT);\
         INSERT INTO contacts VALUES(1);\
         INSERT INTO raw_contacts VALUES(1, 1, 'Alice');\
         INSERT INTO mimetypes VALUES(1, 'vnd.android.cursor.item/phone_v2');\
         INSERT INTO mimetypes VALUES(2, 'vnd.android.cursor.item/email_v2');\
         INSERT INTO data VALUES(1, 1, 1, '+8613800000000');\
         INSERT INTO data VALUES(2, 1, 2, 'alice@example.com');",
    );
    let run = root.evidence("android_contacts");

    assert_eq!(run.count("mobile.artifact"), 2);
    let values: Vec<&str> = run
        .of_type("mobile.artifact")
        .iter()
        .filter_map(|record| data(record, "data1").and_then(Value::as_str))
        .collect();
    assert!(values.contains(&"+8613800000000"));
    assert!(values.contains(&"alice@example.com"));
}

#[test]
fn chrome_history_uses_webkit_microseconds() {
    let root = Root::new("chrome");
    root.database(
        "data/data/com.android.chrome/app_chrome/Default/History",
        &format!(
            "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, \
             typed_count INTEGER, last_visit_time INTEGER, hidden INTEGER);\
             INSERT INTO urls VALUES(1, 'https://example.com', 'Example', 3, 1, {WEBKIT_2024}, 0);"
        ),
    );
    let run = root.evidence("android_chrome_history");

    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "visit_count").and_then(Value::as_i64),
        Some(3)
    );
}

#[test]
fn whatsapp_joins_chat_jid_and_media() {
    let root = Root::new("whatsapp");
    root.database(
        "data/data/com.whatsapp/databases/msgstore.db",
        &format!(
            "CREATE TABLE jid(_id INTEGER PRIMARY KEY, raw_string TEXT);\
             CREATE TABLE chat(_id INTEGER PRIMARY KEY, jid_row_id INTEGER, subject TEXT);\
             CREATE TABLE message(_id INTEGER PRIMARY KEY, timestamp INTEGER, received_timestamp INTEGER, \
             from_me INTEGER, text_data TEXT, message_type INTEGER, chat_row_id INTEGER, sender_jid_row_id INTEGER);\
             CREATE TABLE message_media(message_row_id INTEGER, file_path TEXT, file_size INTEGER);\
             INSERT INTO jid VALUES(1, '8613800000000@s.whatsapp.net');\
             INSERT INTO chat VALUES(1, 1, NULL);\
             INSERT INTO message VALUES(1, {UNIX_MS_2024}, {UNIX_MS_2024}, 0, 'hi', 0, 1, 2);\
             INSERT INTO message_media VALUES(1, '/sdcard/WhatsApp/Media/a.jpg', 1234);"
        ),
    );
    let run = root.evidence("android_whatsapp");

    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "chat_jid").and_then(Value::as_str),
        Some("8613800000000@s.whatsapp.net")
    );
    assert_eq!(
        data(record, "media_path").and_then(Value::as_str),
        Some("/sdcard/WhatsApp/Media/a.jpg")
    );
}

#[test]
fn media_store_reads_display_name_and_size() {
    let root = Root::new("media");
    root.database(
        "data/media/External.db",
        &format!(
            "CREATE TABLE files(_id INTEGER PRIMARY KEY, _data TEXT, _display_name TEXT, \
             mime_type TEXT, _size INTEGER, date_added INTEGER, date_modified INTEGER, \
             datetaken INTEGER, width INTEGER, height INTEGER, latitude REAL, longitude REAL);\
             INSERT INTO files VALUES(1, '/sdcard/DCIM/IMG_0001.JPG', 'IMG_0001.JPG', 'image/jpeg', \
             4096, {UNIX_MS_2024}, {UNIX_MS_2024}, {UNIX_MS_2024}, 4032, 3024, 31.2304, 121.4737);"
        ),
    );
    let run = root.evidence("android_media");

    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "display_name").and_then(Value::as_str),
        Some("IMG_0001.JPG")
    );
    assert_eq!(data(record, "size").and_then(Value::as_i64), Some(4096));
}

#[test]
fn accounts_xml_emits_one_authority_row() {
    let root = Root::new("accounts");
    root.write(
        "data/system/sync/accounts.xml",
        r#"<?xml version='1.0' encoding='utf-8' standalone='yes' ?>
<accounts version="1">
  <account account="alice@example.com" type="com.google">
    <authority authority="com.google.android.gms" account="alice@example.com"
               type="com.google" user="0" enabled="true" syncable="true" id="1" />
  </account>
</accounts>
"#,
    );
    let run = root.evidence("android_accounts");

    assert_eq!(run.count("mobile.artifact"), 1);
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("title"), Some("com.google.android.gms"));
    assert_eq!(
        data(record, "account").and_then(Value::as_str),
        Some("alice@example.com")
    );
    assert_eq!(
        data(record, "enabled").and_then(Value::as_str),
        Some("true")
    );
}

#[test]
fn wifi_xml_reads_ssid_key_and_last_connection() {
    let root = Root::new("wifi");
    root.write(
        "data/misc/wifi/WifiConfigStore.xml",
        &format!(
            r#"<?xml version='1.0' encoding='utf-8' standalone='yes' ?>
<WifiConfigStoreData>
  <NetworkList>
    <Network>
      <WiFiConfiguration>
        <string name="ConfigKey">&quot;Home&quot;WPA_PSK</string>
        <string name="SSID">&quot;Home&quot;</string>
        <string name="PreSharedKey">secret-key</string>
        <long name="LastConnectedTime" value="{UNIX_MS_2024}" />
      </WiFiConfiguration>
    </Network>
  </NetworkList>
</WifiConfigStoreData>
"#
        ),
    );
    let run = root.evidence("android_wifi");

    assert_eq!(run.count("mobile.artifact"), 1);
    let record = run.of_type("mobile.artifact")[0];
    assert_eq!(record.text("title"), Some("Home"));
    assert_eq!(record.text("time"), Some("2024-01-01T00:00:00Z"));
    assert_eq!(
        data(record, "PreSharedKey").and_then(Value::as_str),
        Some("secret-key")
    );
    assert_eq!(
        data(record, "security_mode").and_then(Value::as_str),
        Some("WPA_PSK")
    );
}

#[test]
fn selected_database_uses_the_files_scope() {
    let root = Root::new("selected");
    let path = sms_fixture(&root);
    let run = root.selected("android_selected_databases", &path);

    assert_eq!(run.count("mobile.artifact"), 1);
    assert_eq!(
        run.of_type("mobile.artifact")[0].text("artifact"),
        Some("android_sms")
    );
}

#[test]
fn selected_xml_uses_the_files_scope() {
    let root = Root::new("selected-xml");
    let path = root.write(
        "accounts.xml",
        r#"<accounts><account account="bob" type="com.example"><authority authority="mail" account="bob" type="com.example" id="2" /></account></accounts>"#,
    );
    let run = root.selected("android_selected_xml", &path);

    assert_eq!(run.count("mobile.artifact"), 1);
    assert_eq!(
        run.of_type("mobile.artifact")[0].text("artifact"),
        Some("android_accounts")
    );
}

#[test]
fn missing_sms_database_is_a_warning() {
    let root = Root::new("missing");
    let run = root.evidence("android_sms");

    assert_eq!(run.count("mobile.artifact"), 0);
    assert!(run
        .warnings
        .iter()
        .any(|(code, _, _)| code == "ARTIFACT_NOT_FOUND"));
}
