//! Fixed SQL definitions for the Android core artifacts.

use mobile_core::{SqlSpec, SqlUnit, SqlVariant, TimeKind};

static SMS: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["sms"],
        sql: r#"
SELECT _id AS message_id, thread_id AS thread_id, address AS address,
       date AS date, date_sent AS date_sent, read AS read, type AS type,
       body AS body, service_center AS service_center, subject AS subject
FROM sms ORDER BY date ASC
"#,
    },
    SqlVariant {
        required_tables: &["sms"],
        sql: r#"
SELECT _id AS message_id, thread_id AS thread_id, address AS address,
       date AS date, date_sent AS date_sent, read AS read, type AS type,
       body AS body, NULL AS service_center, NULL AS subject
FROM sms ORDER BY date ASC
"#,
    },
    SqlVariant {
        required_tables: &["sms"],
        sql: r#"
SELECT _id AS message_id, thread_id AS thread_id, address AS address,
       date AS date, NULL AS date_sent, read AS read, type AS type,
       body AS body, NULL AS service_center, NULL AS subject
FROM sms ORDER BY date ASC
"#,
    },
];

static SPAM_SMS: &[SqlVariant] = &[SqlVariant {
    required_tables: &["spam_sms"],
    sql: r#"
SELECT _id AS message_id, thread_id AS thread_id, address AS address,
       date AS date, date_sent AS date_sent, read AS read, type AS type,
       body AS body, NULL AS service_center, NULL AS subject
FROM spam_sms ORDER BY date ASC
"#,
}];

static MMS: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["pdu", "addr"],
        sql: r#"
SELECT pdu._id AS message_id, pdu.thread_id AS thread_id, pdu.date AS date,
       pdu.date_sent AS date_sent, pdu.msg_box AS msg_box, pdu.subject AS subject,
       pdu.ct_t AS content_type,
       (SELECT address FROM addr WHERE addr.msg_id = pdu._id AND addr.type = 0x89 LIMIT 1) AS from_address,
       (SELECT address FROM addr WHERE addr.msg_id = pdu._id AND addr.type = 0x97 LIMIT 1) AS to_address
FROM pdu ORDER BY pdu.date ASC
"#,
    },
    SqlVariant {
        required_tables: &["pdu"],
        sql: r#"
SELECT _id AS message_id, thread_id AS thread_id, date AS date,
       date_sent AS date_sent, msg_box AS msg_box, subject AS subject,
       ct_t AS content_type, NULL AS from_address, NULL AS to_address
FROM pdu ORDER BY date ASC
"#,
    },
];

static CALLS: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["calls"],
        sql: r#"
SELECT _id AS call_id, date AS date, number AS number, duration AS duration,
       type AS type, geocoded_location AS geocoded_location,
       countryiso AS countryiso, deleted AS deleted,
       phone_account_address AS phone_account_address,
       transcription AS transcription
FROM calls ORDER BY date ASC
"#,
    },
    SqlVariant {
        required_tables: &["calls"],
        sql: r#"
SELECT _id AS call_id, date AS date, number AS number, duration AS duration,
       type AS type, NULL AS geocoded_location, NULL AS countryiso,
       NULL AS deleted, NULL AS phone_account_address, NULL AS transcription
FROM calls ORDER BY date ASC
"#,
    },
];

static CONTACTS: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["raw_contacts", "contacts", "data", "mimetypes"],
        sql: r#"
SELECT rc._id AS raw_contact_id, rc.display_name AS display_name,
       d.data1 AS data1, m.mimetype AS mimetype
FROM raw_contacts rc
JOIN contacts c ON rc.contact_id = c._id
LEFT JOIN data d ON d.raw_contact_id = rc._id
LEFT JOIN mimetypes m ON d.mimetype_id = m._id
WHERE m.mimetype IN ('vnd.android.cursor.item/phone_v2',
                     'vnd.android.cursor.item/email_v2')
ORDER BY rc.display_name ASC, rc._id ASC
"#,
    },
    SqlVariant {
        required_tables: &["raw_contacts", "data", "mimetypes"],
        sql: r#"
SELECT rc._id AS raw_contact_id, rc.display_name AS display_name,
       d.data1 AS data1, m.mimetype AS mimetype
FROM raw_contacts rc
LEFT JOIN data d ON d.raw_contact_id = rc._id
LEFT JOIN mimetypes m ON d.mimetype_id = m._id
WHERE m.mimetype IN ('vnd.android.cursor.item/phone_v2',
                     'vnd.android.cursor.item/email_v2')
ORDER BY rc.display_name ASC, rc._id ASC
"#,
    },
];

static CHROME: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["urls"],
        sql: r#"
SELECT id AS url_id, url AS url, title AS title, visit_count AS visit_count,
       typed_count AS typed_count, last_visit_time AS last_visit_time,
       hidden AS hidden
FROM urls ORDER BY last_visit_time ASC
"#,
    },
    SqlVariant {
        required_tables: &["urls"],
        sql: r#"
SELECT id AS url_id, url AS url, title AS title, visit_count AS visit_count,
       NULL AS typed_count, last_visit_time AS last_visit_time, NULL AS hidden
FROM urls ORDER BY last_visit_time ASC
"#,
    },
];

static WHATSAPP: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["message", "chat", "jid", "message_media"],
        sql: r#"
SELECT m._id AS message_id, m.timestamp AS timestamp,
       m.received_timestamp AS received_timestamp, m.from_me AS from_me,
       m.text_data AS text, m.message_type AS message_type,
       m.chat_row_id AS chat_row_id, m.sender_jid_row_id AS sender_jid_row_id,
       c.subject AS chat_subject, j.raw_string AS chat_jid,
       mm.file_path AS media_path, mm.file_size AS media_size
FROM message m
LEFT JOIN chat c ON c._id = m.chat_row_id
LEFT JOIN jid j ON j._id = c.jid_row_id
LEFT JOIN message_media mm ON mm.message_row_id = m._id
ORDER BY m.timestamp ASC
"#,
    },
    SqlVariant {
        required_tables: &["message", "chat", "jid"],
        sql: r#"
SELECT m._id AS message_id, m.timestamp AS timestamp,
       m.received_timestamp AS received_timestamp, m.from_me AS from_me,
       m.text_data AS text, m.message_type AS message_type,
       m.chat_row_id AS chat_row_id, m.sender_jid_row_id AS sender_jid_row_id,
       c.subject AS chat_subject, j.raw_string AS chat_jid,
       NULL AS media_path, NULL AS media_size
FROM message m
LEFT JOIN chat c ON c._id = m.chat_row_id
LEFT JOIN jid j ON j._id = c.jid_row_id
ORDER BY m.timestamp ASC
"#,
    },
    SqlVariant {
        required_tables: &["message"],
        sql: r#"
SELECT _id AS message_id, timestamp AS timestamp,
       received_timestamp AS received_timestamp, from_me AS from_me,
       text_data AS text, message_type AS message_type,
       chat_row_id AS chat_row_id, sender_jid_row_id AS sender_jid_row_id,
       NULL AS chat_subject, NULL AS chat_jid,
       NULL AS media_path, NULL AS media_size
FROM message ORDER BY timestamp ASC
"#,
    },
];

static MEDIA: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["files"],
        sql: r#"
SELECT _id AS media_id, _data AS path, _display_name AS display_name,
       mime_type AS mime_type, _size AS size, date_added AS date_added,
       date_modified AS date_modified, datetaken AS date_taken,
       width AS width, height AS height, latitude AS latitude,
       longitude AS longitude
FROM files ORDER BY date_added ASC
"#,
    },
    SqlVariant {
        required_tables: &["files"],
        sql: r#"
SELECT _id AS media_id, _data AS path, _display_name AS display_name,
       mime_type AS mime_type, _size AS size, date_added AS date_added,
       date_modified AS date_modified, datetaken AS date_taken,
       NULL AS width, NULL AS height, NULL AS latitude, NULL AS longitude
FROM files ORDER BY date_added ASC
"#,
    },
    SqlVariant {
        required_tables: &["files"],
        sql: r#"
SELECT _id AS media_id, _data AS path, _display_name AS display_name,
       mime_type AS mime_type, _size AS size, date_added AS date_added,
       NULL AS date_modified, NULL AS date_taken, NULL AS width,
       NULL AS height, NULL AS latitude, NULL AS longitude
FROM files ORDER BY date_added ASC
"#,
    },
];

pub static ANDROID_SMS: SqlSpec = SqlSpec {
    artifact: "android_sms",
    platform: "android",
    paths: &[
        "**/com.android.providers.telephony/databases/mmssms.db",
        "**/mmssms.db",
    ],
    units: &[
        SqlUnit {
            label: "sms",
            variants: SMS,
            time_column: Some("date"),
            time_kind: TimeKind::UnixMillis,
            key_columns: &["message_id"],
            title_columns: &["address", "body"],
        },
        SqlUnit {
            label: "spam_sms",
            variants: SPAM_SMS,
            time_column: Some("date"),
            time_kind: TimeKind::UnixMillis,
            key_columns: &["message_id"],
            title_columns: &["address", "body"],
        },
        SqlUnit {
            label: "mms",
            variants: MMS,
            time_column: Some("date"),
            time_kind: TimeKind::UnixSeconds,
            key_columns: &["message_id"],
            title_columns: &["subject", "from_address", "to_address"],
        },
    ],
};

pub static ANDROID_CALLS: SqlSpec = SqlSpec {
    artifact: "android_calls",
    platform: "android",
    paths: &[
        "**/com.android.providers.contacts/databases/calllog.db",
        "**/calllog.db",
    ],
    units: &[SqlUnit {
        label: "call",
        variants: CALLS,
        time_column: Some("date"),
        time_kind: TimeKind::UnixMillis,
        key_columns: &["call_id"],
        title_columns: &["number", "geocoded_location"],
    }],
};

pub static ANDROID_CONTACTS: SqlSpec = SqlSpec {
    artifact: "android_contacts",
    platform: "android",
    paths: &[
        "**/com.android.providers.contacts/databases/contacts2.db",
        "**/contacts2.db",
    ],
    units: &[SqlUnit {
        label: "value",
        variants: CONTACTS,
        time_column: None,
        time_kind: TimeKind::UnixMillis,
        key_columns: &["raw_contact_id", "mimetype", "data1"],
        title_columns: &["display_name", "data1"],
    }],
};

pub static ANDROID_CHROME: SqlSpec = SqlSpec {
    artifact: "android_chrome_history",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/History",
        "**/app_sbrowser/Default/History",
        "**/app_opera/History",
        "**/app_webview/Default/History",
    ],
    units: &[SqlUnit {
        label: "url",
        variants: CHROME,
        time_column: Some("last_visit_time"),
        time_kind: TimeKind::WebkitMicros,
        key_columns: &["url_id"],
        title_columns: &["title", "url"],
    }],
};

pub static ANDROID_WHATSAPP: SqlSpec = SqlSpec {
    artifact: "android_whatsapp",
    platform: "android",
    paths: &[
        "**/com.whatsapp/databases/msgstore.db",
        "**/msgstore.db",
    ],
    units: &[SqlUnit {
        label: "message",
        variants: WHATSAPP,
        time_column: Some("timestamp"),
        time_kind: TimeKind::UnixMillis,
        key_columns: &["message_id"],
        title_columns: &["chat_jid", "text"],
    }],
};

pub static ANDROID_MEDIA: SqlSpec = SqlSpec {
    artifact: "android_media",
    platform: "android",
    paths: &[
        "**/media/External.db",
        "**/databases/external.db",
        "**/com.samsung.android.providers.media/databases/media.db",
    ],
    units: &[SqlUnit {
        label: "file",
        variants: MEDIA,
        time_column: Some("date_added"),
        time_kind: TimeKind::Auto,
        key_columns: &["media_id"],
        title_columns: &["display_name", "path"],
    }],
};

pub static ANDROID_SPECS: &[&SqlSpec] = &[
    &ANDROID_SMS,
    &ANDROID_CALLS,
    &ANDROID_CONTACTS,
    &ANDROID_CHROME,
    &ANDROID_WHATSAPP,
    &ANDROID_MEDIA,
];
