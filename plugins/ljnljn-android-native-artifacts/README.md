# ljnljn-android-native-artifacts

LovelyCarve **LPP v2** 插件：直接读取 Android 提取物中的核心应用数据，
不运行 ALEAPP，也不依赖它的 TSV 报告。

- 插件 ID：`org.ljnljn.android-native-artifacts`
- 记录：`mobile.artifact`、`mobile.artifact.file`
- 权限：`file_materialize`（只读物化数据库副本）

## 覆盖范围

| 分析器 | 数据源 | 核心字段 |
| --- | --- | --- |
| `android_sms` | `mmssms.db` 的 `sms`、`spam_sms`、`pdu`/`addr` | 号码、正文、类型、时间、MMS 主题与收发地址 |
| `android_calls` | `calllog.db` | 号码、类型、时长、归属地、时间 |
| `android_contacts` | `contacts2.db` | 显示名、电话、邮箱、mimetype |
| `android_chrome_history` | Chrome/Opera/WebView 的 `History` | URL、标题、访问次数、末次访问 |
| `android_whatsapp` | `msgstore.db` | 聊天 JID、方向、正文、类型、媒体路径、时间 |
| `android_media` | `External.db`、三星 `media.db` | 文件名、路径、MIME、大小、拍摄/添加时间、尺寸、经纬度 |
| `android_accounts` | `system/sync/accounts.xml` | 账号、类型、authority、enabled、syncable |
| `android_wifi` | `WifiConfigStore.xml` | SSID、安全模式、预共享密钥、上次连接时间 |

另有 `android_selected_databases`（手动选数据库）和 `android_selected_xml`
（手动选账号/WiFi XML）。

## 读取方式

- 每个数据库先复制到私有临时目录，连同存在的 `-wal` / `-shm` / `-journal`
  边车一起打开；检材本身不被写入。
- 只执行插件内固定的只读 SQL；SQLite 文件头、表与列先校验，不同 Android
  版本或厂商通过查询变体回退。
- 时间按存储格式归一化：短信/通话/WhatsApp 用毫秒，MMS 用秒，Chrome 用
  1601 纪元的微秒，媒体库按数量级自动判定；原始值保留在 `data` 中。
- XML 只按已知元素和属性名遍历，不执行 XPath 或检材文本。
- 单个数据库上限 1 GiB、单个工件最多 100,000 行、单个文本单元最多 64 KiB；
  超出会标记 `truncated`，不静默丢弃。

## 已知边界

- 只覆盖已列出的 AOSP/三星常见路径；厂商深度定制或非标准目录请用单文件
  分析器显式选择。
- MMS 只输出 PDU 元数据和收/发地址，不展开 SMIL 部件、不导出附件。
- 通讯录按 mimetype 行输出电话/邮箱，不做跨账号合并、不还原头像。
- WhatsApp 只读 `msgstore.db` 的消息表；不解析 `wa.db` 通讯录名、不导出媒体。
- 媒体库只读 `files` 表；若厂商只提供 `images`/`video`/`audio` 分表，当前不会合并。
- 不解析 Telegram、微信等自有加密数据库；不做删除记录恢复。
- 其它 ALEAPP 模块仍可用 `ljnljn-ileapp-aleapp-report` 导入报告，后续按批次
  迁移为原生解析。

## 构建

```bash
cargo test --locked
cargo build --release --locked
```

需要 LovelyCarve v2 宿主（protocol 2）。Release 中的 `.lcpkg` 可直接安装。

## 许可证

AGPL-3.0-only。解析规则参考 ALEAPP（MIT）的公开实现，未复制其 Python 代码。
