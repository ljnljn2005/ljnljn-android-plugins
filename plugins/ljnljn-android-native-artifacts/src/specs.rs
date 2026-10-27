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

static DOWNLOADS: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["downloads"],
        sql: r#"
SELECT _id AS download_id, lastmod AS last_modified, title AS title,
       description AS description, uri AS uri, _data AS path,
       mimetype AS mime_type, notificationpackage AS package,
       current_bytes AS current_bytes, total_bytes AS total_bytes,
       status AS status, errorMsg AS error_message, etag AS etag,
       is_visible_in_downloads_ui AS visible, deleted AS deleted
FROM downloads ORDER BY lastmod ASC
"#,
    },
    SqlVariant {
        required_tables: &["downloads"],
        sql: r#"
SELECT _id AS download_id, lastmod AS last_modified, title AS title,
       description AS description, uri AS uri, _data AS path,
       mimetype AS mime_type, NULL AS package, current_bytes AS current_bytes,
       total_bytes AS total_bytes, status AS status, NULL AS error_message,
       NULL AS etag, NULL AS visible, NULL AS deleted
FROM downloads ORDER BY lastmod ASC
"#,
    },
];

static COOKIES: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["cookies"],
        sql: r#"
SELECT ROWID AS cookie_id, creation_utc AS creation, host_key AS host,
       name AS name, value AS value, path AS path, expires_utc AS expires,
       last_access_utc AS last_access, is_secure AS is_secure,
       is_httponly AS is_httponly
FROM cookies ORDER BY last_access_utc ASC
"#,
    },
    SqlVariant {
        required_tables: &["cookies"],
        sql: r#"
SELECT ROWID AS cookie_id, creation_utc AS creation, host_key AS host,
       name AS name, value AS value, path AS path, expires_utc AS expires,
       last_access_utc AS last_access, NULL AS is_secure, NULL AS is_httponly
FROM cookies ORDER BY last_access_utc ASC
"#,
    },
];

static AUTOFILL: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["autofill"],
        sql: r#"
SELECT ROWID AS autofill_id, date_created AS date_created, name AS name,
       value AS value, date_last_used AS date_last_used, count AS count
FROM autofill ORDER BY date_last_used ASC
"#,
    },
    SqlVariant {
        required_tables: &["autofill"],
        sql: r#"
SELECT ROWID AS autofill_id, date_created AS date_created, name AS name,
       value AS value, NULL AS date_last_used, NULL AS count
FROM autofill ORDER BY date_created ASC
"#,
    },
];

static CALENDAR: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["Events", "Calendars"],
        sql: r#"
SELECT e._id AS event_id, e.dtstart AS start, e.dtend AS end,
       e.eventTimezone AS timezone, e.title AS title, e.description AS description,
       e.eventLocation AS location, e.organizer AS organizer,
       c.calendar_displayName AS calendar
FROM Events e
LEFT JOIN Calendars c ON c._id = e.calendar_id
ORDER BY e.dtstart ASC
"#,
    },
    SqlVariant {
        required_tables: &["Events"],
        sql: r#"
SELECT _id AS event_id, dtstart AS start, dtend AS end,
       eventTimezone AS timezone, title AS title, description AS description,
       eventLocation AS location, organizer AS organizer, NULL AS calendar
FROM Events ORDER BY dtstart ASC
"#,
    },
];

static KEEP: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["text_search_note_content_content", "tree_entity"],
        sql: r#"
SELECT t._id AS note_id, t.time_created AS created,
       t.time_last_updated AS updated, t.user_edited_timestamp AS edited,
       t.title AS title, f.c0text AS text, t.last_modifier_email AS modifier
FROM text_search_note_content_content f
INNER JOIN tree_entity t ON f.docid = t._id
ORDER BY t.time_last_updated ASC
"#,
    },
    SqlVariant {
        required_tables: &["tree_entity", "list_item"],
        sql: r#"
SELECT t._id AS note_id, t.time_created AS created,
       t.time_last_updated AS updated, t.user_edited_timestamp AS edited,
       t.title AS title, l.text AS text, t.last_modifier_email AS modifier
FROM tree_entity t
LEFT JOIN list_item l ON t._id = l._id
WHERE t.title IS NOT NULL OR l.text IS NOT NULL
ORDER BY t.time_last_updated ASC
"#,
    },
];

static MAPS: &[SqlVariant] = &[SqlVariant {
    required_tables: &["sync_item"],
    sql: r#"
SELECT rowid AS place_id, key_string AS key,
       round(latitude * 0.000001, 6) AS latitude,
       round(longitude * 0.000001, 6) AS longitude,
       timestamp AS timestamp
FROM sync_item ORDER BY timestamp ASC
"#,
}];

static TOP_SITES: &[SqlVariant] = &[SqlVariant {
    required_tables: &["top_sites"],
    sql: r#"
SELECT ROWID AS site_id, url AS url, url_rank AS url_rank,
       title AS title, redirects AS redirects
FROM top_sites ORDER BY url_rank ASC
"#,
}];

pub static ANDROID_DOWNLOADS: SqlSpec = SqlSpec {
    artifact: "android_downloads",
    platform: "android",
    paths: &["**/com.android.providers.downloads/databases/downloads.db"],
    units: &[SqlUnit {
        label: "download",
        variants: DOWNLOADS,
        time_column: Some("last_modified"),
        time_kind: TimeKind::UnixMillis,
        key_columns: &["download_id"],
        title_columns: &["title", "path"],
    }],
};

pub static ANDROID_COOKIES: SqlSpec = SqlSpec {
    artifact: "android_cookies",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Cookies",
        "**/app_sbrowser/Default/Cookies",
        "**/app_opera/Cookies",
        "**/app_webview/Default/Cookies",
    ],
    units: &[SqlUnit {
        label: "cookie",
        variants: COOKIES,
        time_column: Some("last_access"),
        time_kind: TimeKind::WebkitMicros,
        key_columns: &["cookie_id"],
        title_columns: &["host", "name"],
    }],
};

pub static ANDROID_AUTOFILL: SqlSpec = SqlSpec {
    artifact: "android_autofill",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Web Data",
        "**/app_sbrowser/Default/Web Data",
        "**/app_opera/Web Data",
        "**/app_webview/Default/Web Data",
    ],
    units: &[SqlUnit {
        label: "autofill",
        variants: AUTOFILL,
        time_column: Some("date_last_used"),
        time_kind: TimeKind::Auto,
        key_columns: &["autofill_id"],
        title_columns: &["name", "value"],
    }],
};

pub static ANDROID_CALENDAR: SqlSpec = SqlSpec {
    artifact: "android_calendar",
    platform: "android",
    paths: &["**/com.android.providers.calendar/databases/calendar.db"],
    units: &[SqlUnit {
        label: "event",
        variants: CALENDAR,
        time_column: Some("start"),
        time_kind: TimeKind::UnixMillis,
        key_columns: &["event_id"],
        title_columns: &["title", "location", "calendar"],
    }],
};

pub static ANDROID_KEEP: SqlSpec = SqlSpec {
    artifact: "android_keep_notes",
    platform: "android",
    paths: &["**/com.google.android.keep/databases/keep.db"],
    units: &[SqlUnit {
        label: "note",
        variants: KEEP,
        time_column: Some("updated"),
        time_kind: TimeKind::UnixMillis,
        key_columns: &["note_id"],
        title_columns: &["title", "text"],
    }],
};

pub static ANDROID_MAPS: SqlSpec = SqlSpec {
    artifact: "android_maps_places",
    platform: "android",
    paths: &["**/com.google.android.apps.maps/databases/gmm_myplaces.db"],
    units: &[SqlUnit {
        label: "place",
        variants: MAPS,
        time_column: Some("timestamp"),
        time_kind: TimeKind::UnixMillis,
        key_columns: &["place_id"],
        title_columns: &["key", "latitude", "longitude"],
    }],
};

pub static ANDROID_TOP_SITES: SqlSpec = SqlSpec {
    artifact: "android_top_sites",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Top Sites",
        "**/app_sbrowser/Default/Top Sites",
        "**/app_opera/Top Sites",
        "**/app_webview/Default/Top Sites",
    ],
    units: &[SqlUnit {
        label: "site",
        variants: TOP_SITES,
        time_column: None,
        time_kind: TimeKind::UnixMillis,
        key_columns: &["site_id"],
        title_columns: &["title", "url"],
    }],
};

static LOGIN_DATA: &[SqlVariant] = &[SqlVariant {
    required_tables: &["logins"],
    sql: r#"
SELECT ROWID AS login_id, origin_url AS origin_url,
       username_value AS username, password_value AS password_blob,
       date_created AS date_created, blacklisted_by_user AS blacklisted,
       signon_realm AS signon_realm
FROM logins ORDER BY date_created ASC
"#,
}];

static CREDIT_CARDS: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["masked_credit_cards", "server_card_metadata"],
        sql: r#"
SELECT c.id AS card_id, c.name_on_card AS name_on_card, c.network AS network,
       c.last_four AS last_four, c.exp_month AS exp_month, c.exp_year AS exp_year,
       c.bank_name AS bank_name, c.nickname AS nickname,
       m.use_date AS last_used, m.use_count AS use_count
FROM masked_credit_cards c
LEFT JOIN server_card_metadata m ON m.id = c.id
ORDER BY m.use_date ASC
"#,
    },
    SqlVariant {
        required_tables: &["masked_credit_cards"],
        sql: r#"
SELECT id AS card_id, name_on_card AS name_on_card, network AS network,
       last_four AS last_four, exp_month AS exp_month, exp_year AS exp_year,
       bank_name AS bank_name, nickname AS nickname,
       NULL AS last_used, NULL AS use_count
FROM masked_credit_cards ORDER BY id ASC
"#,
    },
];

static FIREFOX_COOKIES: &[SqlVariant] = &[
    SqlVariant {
        required_tables: &["moz_cookies"],
        sql: r#"
SELECT ROWID AS cookie_id, lastAccessed AS last_accessed,
       creationTime AS created, host AS host, name AS name, value AS value,
       expiry AS expiry, path AS path
FROM moz_cookies ORDER BY lastAccessed ASC
"#,
    },
    SqlVariant {
        required_tables: &["moz_cookies"],
        sql: r#"
SELECT ROWID AS cookie_id, lastAccessed AS last_accessed,
       NULL AS created, host AS host, name AS name, value AS value,
       expiry AS expiry, path AS path
FROM moz_cookies ORDER BY lastAccessed ASC
"#,
    },
];

pub static ANDROID_LOGIN_DATA: SqlSpec = SqlSpec {
    artifact: "android_login_data",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Login Data",
        "**/app_sbrowser/Default/Login Data",
        "**/app_opera/Login Data",
        "**/app_webview/Default/Login Data",
    ],
    units: &[SqlUnit {
        label: "login",
        variants: LOGIN_DATA,
        time_column: Some("date_created"),
        time_kind: TimeKind::WebkitMicros,
        key_columns: &["login_id"],
        title_columns: &["origin_url", "username"],
    }],
};

pub static ANDROID_CREDIT_CARDS: SqlSpec = SqlSpec {
    artifact: "android_credit_cards",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Web Data",
        "**/app_sbrowser/Default/Web Data",
        "**/app_opera/Web Data",
        "**/app_webview/Default/Web Data",
    ],
    units: &[SqlUnit {
        label: "card",
        variants: CREDIT_CARDS,
        time_column: Some("last_used"),
        time_kind: TimeKind::WebkitMicros,
        key_columns: &["card_id"],
        title_columns: &["name_on_card", "last_four"],
    }],
};

pub static ANDROID_FIREFOX_COOKIES: SqlSpec = SqlSpec {
    artifact: "android_firefox_cookies",
    platform: "android",
    paths: &["**/org.mozilla.firefox/files/mozilla/*.default/cookies.sqlite"],
    units: &[SqlUnit {
        label: "cookie",
        variants: FIREFOX_COOKIES,
        time_column: Some("last_accessed"),
        time_kind: TimeKind::UnixMicros,
        key_columns: &["cookie_id"],
        title_columns: &["host", "name"],
    }],
};

static MEDIA_HISTORY_SESSIONS: &[SqlVariant] = &[SqlVariant {
    required_tables: &["playbackSession"],
    sql: r#"
SELECT last_updated_time_s AS last_updated, origin_id AS origin_id, url AS url,
       position_ms AS position_ms, duration_ms AS duration_ms, title AS title,
       artist AS artist, album AS album, source_title AS source_title
FROM playbackSession ORDER BY last_updated_time_s ASC
"#,
}];

static MEDIA_HISTORY_PLAYBACKS: &[SqlVariant] = &[SqlVariant {
    required_tables: &["playback"],
    sql: r#"
SELECT last_updated_time_s AS last_updated, id AS playback_id, origin_id AS origin_id,
       url AS url, watch_time_s AS watch_time_s, has_audio AS has_audio,
       has_video AS has_video
FROM playback ORDER BY last_updated_time_s ASC
"#,
}];

static MEDIA_HISTORY_ORIGINS: &[SqlVariant] = &[SqlVariant {
    required_tables: &["origin"],
    sql: r#"
SELECT last_updated_time_s AS last_updated, id AS origin_id, origin AS origin,
       aggregate_watchtime_audio_video_s AS aggregate_watchtime
FROM origin ORDER BY last_updated_time_s ASC
"#,
}];

pub static ANDROID_MEDIA_HISTORY: SqlSpec = SqlSpec {
    artifact: "android_media_history",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Media History",
        "**/app_sbrowser/Default/Media History",
        "**/app_opera/Media History",
        "**/app_webview/Default/Media History",
    ],
    units: &[
        SqlUnit {
            label: "session",
            variants: MEDIA_HISTORY_SESSIONS,
            time_column: Some("last_updated"),
            time_kind: TimeKind::WebkitSeconds,
            key_columns: &["origin_id"],
            title_columns: &["title", "url"],
        },
        SqlUnit {
            label: "playback",
            variants: MEDIA_HISTORY_PLAYBACKS,
            time_column: Some("last_updated"),
            time_kind: TimeKind::WebkitSeconds,
            key_columns: &["playback_id"],
            title_columns: &["url"],
        },
        SqlUnit {
            label: "origin",
            variants: MEDIA_HISTORY_ORIGINS,
            time_column: Some("last_updated"),
            time_kind: TimeKind::WebkitSeconds,
            key_columns: &["origin_id"],
            title_columns: &["origin"],
        },
    ],
};

static SHORTCUTS: &[SqlVariant] = &[SqlVariant {
    required_tables: &["omni_box_shortcuts"],
    sql: r#"
SELECT rowid AS shortcut_id, last_access_time AS last_access, text AS text,
       type AS match_type, transition AS transition, url AS url,
       fill_into_edit AS fill_into_edit, contents AS contents,
       description AS description, keyword AS keyword, number_of_hits AS hits
FROM omni_box_shortcuts ORDER BY last_access_time ASC, rowid ASC
"#,
}];

static NETWORK_PREDICTOR: &[SqlVariant] = &[SqlVariant {
    required_tables: &["network_action_predictor"],
    sql: r#"
SELECT rowid AS predictor_id, user_text AS user_text, url AS url,
       number_of_hits AS hits, number_of_misses AS misses
FROM network_action_predictor ORDER BY rowid ASC
"#,
}];

static BROWSING_TOPICS: &[SqlVariant] = &[SqlVariant {
    required_tables: &["browsing_topics_api_usages"],
    sql: r#"
SELECT rowid AS topic_id, last_usage_time AS last_usage,
       context_domain AS context_domain,
       hashed_context_domain AS hashed_context_domain,
       hashed_main_frame_host AS hashed_main_frame_host
FROM browsing_topics_api_usages ORDER BY last_usage_time ASC
"#,
}];

static OFFLINE_PAGES: &[SqlVariant] = &[SqlVariant {
    required_tables: &["offlinepages_v1"],
    sql: r#"
SELECT rowid AS page_id, creation_time AS creation_time,
       last_access_time AS last_access, online_url AS online_url,
       file_path AS file_path, title AS title, access_count AS access_count,
       file_size AS file_size
FROM offlinepages_v1 ORDER BY creation_time ASC
"#,
}];

static PAYMENTS_CUSTOMER: &[SqlVariant] = &[SqlVariant {
    required_tables: &["payments_customer_data"],
    sql: r#"
SELECT rowid AS customer_id, customer_id AS customer
FROM payments_customer_data ORDER BY rowid ASC
"#,
}];

pub static ANDROID_SHORTCUTS: SqlSpec = SqlSpec {
    artifact: "android_browser_shortcuts",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Shortcuts",
        "**/app_sbrowser/Default/Shortcuts",
        "**/app_opera/Shortcuts",
        "**/app_webview/Default/Shortcuts",
    ],
    units: &[SqlUnit {
        label: "shortcut",
        variants: SHORTCUTS,
        time_column: Some("last_access"),
        time_kind: TimeKind::WebkitMicros,
        key_columns: &["shortcut_id"],
        title_columns: &["text", "url"],
    }],
};

pub static ANDROID_NETWORK_PREDICTOR: SqlSpec = SqlSpec {
    artifact: "android_network_action_predictor",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Network Action Predictor",
        "**/app_sbrowser/Default/Network Action Predictor",
        "**/app_opera/Network Action Predictor",
        "**/app_webview/Default/Network Action Predictor",
    ],
    units: &[SqlUnit {
        label: "predictor",
        variants: NETWORK_PREDICTOR,
        time_column: None,
        time_kind: TimeKind::UnixMillis,
        key_columns: &["predictor_id"],
        title_columns: &["user_text", "url"],
    }],
};

pub static ANDROID_BROWSING_TOPICS: SqlSpec = SqlSpec {
    artifact: "android_browsing_topics",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/BrowsingTopicsState",
        "**/app_sbrowser/Default/BrowsingTopicsState",
        "**/app_webview/Default/BrowsingTopicsState",
    ],
    units: &[SqlUnit {
        label: "usage",
        variants: BROWSING_TOPICS,
        time_column: Some("last_usage"),
        time_kind: TimeKind::WebkitMicros,
        key_columns: &["topic_id"],
        title_columns: &["context_domain", "hashed_context_domain"],
    }],
};

pub static ANDROID_OFFLINE_PAGES: SqlSpec = SqlSpec {
    artifact: "android_offline_pages",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Offline Pages/metadata/OfflinePages.db",
        "**/app_sbrowser/Default/Offline Pages/metadata/OfflinePages.db",
        "**/app_webview/Default/Offline Pages/metadata/OfflinePages.db",
    ],
    units: &[SqlUnit {
        label: "page",
        variants: OFFLINE_PAGES,
        time_column: Some("creation_time"),
        time_kind: TimeKind::WebkitMicros,
        key_columns: &["page_id"],
        title_columns: &["title", "online_url"],
    }],
};

pub static ANDROID_PAYMENTS_CUSTOMER: SqlSpec = SqlSpec {
    artifact: "android_payments_customer",
    platform: "android",
    paths: &[
        "**/app_chrome/Default/Web Data",
        "**/app_sbrowser/Default/Web Data",
        "**/app_opera/Web Data",
        "**/app_webview/Default/Web Data",
    ],
    units: &[SqlUnit {
        label: "customer",
        variants: PAYMENTS_CUSTOMER,
        time_column: None,
        time_kind: TimeKind::UnixMillis,
        key_columns: &["customer_id"],
        title_columns: &["customer"],
    }],
};

pub static ANDROID_SPECS: &[&SqlSpec] = &[
    &ANDROID_SMS,
    &ANDROID_CALLS,
    &ANDROID_CONTACTS,
    &ANDROID_CHROME,
    &ANDROID_WHATSAPP,
    &ANDROID_MEDIA,
    &ANDROID_DOWNLOADS,
    &ANDROID_COOKIES,
    &ANDROID_AUTOFILL,
    &ANDROID_CALENDAR,
    &ANDROID_KEEP,
    &ANDROID_MAPS,
    &ANDROID_TOP_SITES,
    &ANDROID_LOGIN_DATA,
    &ANDROID_CREDIT_CARDS,
    &ANDROID_FIREFOX_COOKIES,
    &ANDROID_MEDIA_HISTORY,
    &ANDROID_SHORTCUTS,
    &ANDROID_NETWORK_PREDICTOR,
    &ANDROID_BROWSING_TOPICS,
    &ANDROID_OFFLINE_PAGES,
    &ANDROID_PAYMENTS_CUSTOMER,
];
