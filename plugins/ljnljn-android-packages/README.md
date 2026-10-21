# ljnljn-android-packages

LovelyCarve **LPP v2** 插件：解析 Android `packages.xml`，输出去重后的软件包清单与
`<last-platform-version>`。

- 插件 ID：`org.ljnljn.android-packages`
- 版本：`0.1.0`（SDK `lovelycarve-plugin 0.2.0`）
- 分析器：`packages`（`scope = "files"`，扩展名 `xml`）
- 记录类型：`android.packages.report`、`android.packages.platform`、
  `android.packages.package`
- 权限：无
- 结构哈希：`fnv1a64:9de0b62569721e2c`

## 能力

严格的事件式 XML 解析（继承 v1 的 `parser` / `model`，未重写）：

- XML 声明、注释、处理指令、DOCTYPE、CDATA 被识别并跳过，不执行、不联网；
- 根元素按 local-name 匹配且大小写不敏感，必须是 `<packages>`；
- `<last-platform-version>` 读取 `internal` / `external` / `fingerprint`（属性名
  大小写不敏感），多个元素保留首见值并标记冲突；
- 枚举 `<packages>` 直接子级 `<package>` 与 `<updated-package>`；嵌套 package
  只计数不登记；
- 属性值由 quick-xml 做规范化与实体解码；自闭合与成对标签等价，属性顺序无关；
- 首见位置输出源字节偏移与行号（CRLF/CR/LF、UTF-8 BOM 都已处理）。

**去重语义（与 v1 相同）**：包名按原样大小写比较，首见元素决定位置与首见字段；
后续同名元素只填补空缺字段，冲突值写入 `conflicting_fields`，不覆盖首见值。

## 记录

| 类型 | 说明 |
| --- | --- |
| `android.packages.report` | 每个输入一份：状态、错误代码/行号/偏移、根元素、XML 声明、元素与包计数、限制说明 |
| `android.packages.platform` | `<last-platform-version>` 的 internal/external/fingerprint、行号与偏移 |
| `android.packages.package` | 包名（键）、version、codePath、installer、首见元素/位置、出现次数、冲突字段 |

每条记录都带源文件引用（`source.root` / `source.path` / `source.node_id`，宿主
打开文件时自动填写）与解析位置说明。

## 状态与告警

不是“空成功”的输入会同时产生状态记录与 `run.warn`：

| 状态 | 含义 |
| --- | --- |
| `ok` | 良构 `<packages>`，可含 0 个包 |
| `empty` | 空文件或只有空白/注释/声明 |
| `not_packages` | 有根元素但不是 `<packages>` |
| `bad_xml` | XML 语法/结构错误（含错误 code、行号、偏移） |
| `unsupported_encoding` | UTF-16 或 Android 二进制 XML（ABX/AXML） |
| `input_too_large` | 超过 64 MiB，未读取 |

## 构建与测试

```bash
cargo test --locked
cargo build --release --locked
# schema.json（宿主位于 SDK 工作区 target/debug/）
lc-plugin-host --describe --library target/release/ljnljn_android_packages.dll > schema.json
```

本 crate 是独立 workspace（`[workspace]`），SDK 路径为
`../../_plugin-gen/_sdk_v2-official/sdk`；`testing` feature 只在 dev-dependencies
启用。Windows 产物为 `target/release/ljnljn_android_packages.dll`。

## 安装包布局

```text
ljnljn-android-packages/
├── plugin.toml
├── ljnljn_android_packages.dll
├── schema.json
├── README.md
├── LICENSE
└── locales/{en,zh-CN}.toml
```

## 边界

- 只解析 UTF-8 文本 `packages.xml`；不按 XML 声明切换字节编码，不解析 APK、
  AndroidManifest、签名或权限表。
- `packages.xml` 不提供每包安装/更新时间，本插件不生成时间线事件，也不猜测时间。
- 输入上限 64 MiB；最多登记 100,000 个唯一包名、输出 3,000 条包记录，超出会
  显式写入限制说明与告警。
- 只读逻辑文件句柄，不执行 XML 内容、不展开外部实体、不访问网络。

## 许可证

AGPL-3.0-only，见 [LICENSE](LICENSE)。
