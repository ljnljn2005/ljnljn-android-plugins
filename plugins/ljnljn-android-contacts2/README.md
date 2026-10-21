# ljnljn-android-contacts2

LovelyCarve **LPP v2** 插件：只读统计 Android `contacts2.db` 的 `calls` 表通话记录。

- 插件 ID：`org.ljnljn.android-contacts2`
- 版本：`0.1.0`（SDK `lovelycarve-plugin 0.2.0`）
- 分析器：`contacts2_calls`（`scope = "files"`，扩展名 `db`、`sqlite`）
- 记录类型：`android.contacts2.summary`、`.column`、`.number`、`.type`、`.note`、
  `.limitation`、`.error`
- 权限：`file_materialize`（rusqlite 需要路径；副本只读，运行结束自动释放）
- 结构哈希：`fnv1a64:6077eac13890a950`

## 能力

SQL 统计层（`stats`）从 v1 原样复用，只执行写死在代码里的固定查询：

- 定位 `calls` 表（SQLite 对象名大小写不敏感；优先表，其次视图）；
- 总行数、`_id` 的 NULL/非整型/重复取值检查；
- 去重行数：`SELECT DISTINCT` 去重键 `(number, date, type, duration)`，缺列时
  自动缩短键并写入限制说明；
- 每号码通话次数与 `duration` 整型合计（原始单位）；
- `type` 分布与 AOSP `CallLog.Calls` 常量映射（未知值原样保留）；
- `date` 最早/最晚原始值，并按数量级判定秒/毫秒（`mixed`、`unknown` 不猜测）；
- `name` / `cached_name` 非空行数；
- 缺表、视图、空表、缺列、非 SQLite/损坏数据库都有明确状态或错误记录，不 panic。

**去重语义保持不变**：`duplicate_rows = total_rows - distinct_rows`，去重只在统计
结果中发生，从不修改检材；`incomplete_rows` 的概念不适用于本插件。

## 时间处理

`date` 仍以原始数值输出并保留数量级判定依据。**只有**判定为 `seconds` 或
`milliseconds` 时，`summary` 额外给出 `date_min_utc` / `date_max_utc` RFC 3339
投影（epoch 本身是 UTC，不做时区推断）；`mixed` / `unknown` 不产生时间戳。
本插件不生成全局时间线事件，也不把 `date` 当作本机时间。

## 记录

| 类型 | 说明 |
| --- | --- |
| `android.contacts2.summary` | 状态、表、行数、去重键/行数、号码/类型计数、date 范围/单位/UTC 投影、duration 与列检查 |
| `android.contacts2.column` | 7 个预期列的存在性、声明类型、序号 |
| `android.contacts2.number` | 号码（原文分组）、通话次数、duration 合计 |
| `android.contacts2.type` | type 原值、数值、含义、次数 |
| `android.contacts2.note` | 重复记录、缺列、_id 重复等解释性说明 |
| `android.contacts2.limitation` | 限制与覆盖说明（序号 + 文本，最多 64 条 + 溢出说明） |
| `android.contacts2.error` | 无法物化/打开/查询时的错误代码与说明 |

## 构建与测试

```bash
cargo test --locked
cargo build --release --locked
lc-plugin-host --describe --library target/release/ljnljn_android_contacts2.dll > schema.json
```

独立 workspace（`[workspace]`），SDK 路径 `../../_plugin-gen/_sdk_v2-official/sdk`；
`rusqlite` 使用 `bundled`，目标机无需安装 SQLite。`testing` feature 只在
dev-dependencies 启用。

## 安装包布局

```text
ljnljn-android-contacts2/
├── plugin.toml
├── ljnljn_android_contacts2.dll
├── schema.json
├── README.md
├── LICENSE
└── locales/{en,zh-CN}.toml
```

## 边界

- 只读打开宿主物化的单文件副本；不写回检材、不创建永久索引。
- 不合并 `-wal` / `-shm` / `-journal` 边车文件；若只有主文件，未 checkpoint 的
  通话可能缺失（`journal_hint = wal` 时会说明）。
- `date` 单位只按数量级判断，不换算绝对时间、不推断时区或夏令时；UTC 投影仅
  在单位明确时提供，原始值始终保留。
- `duration` 只累加整型值，按原始单位输出（Android 通常为秒，但不校验语义）。
- 号码按原始值分组，不做归一化（空格、+86、区号）或归属地判断，不关联
  `contacts` 联系人表。
- 物化文件上限 2 GiB；宿主单次运行最多 16 个租约、合计 8 GiB，本插件每个输入
  只使用一个租约。
- 不执行检材内 SQL 文本，不联网。

## 许可证

AGPL-3.0-only，见 [LICENSE](LICENSE)。
