# ljnljn-android-contacts2

LovelyCarve 原生插件（LPP v1 / C ABI v1），插件 ID `org.ljnljn.android-contacts2`。

解析 contacts2.db 通话记录并按联系人与号码去重统计。

## 分析器

| 类型 | analyzer | 扩展名 |
| --- | --- | --- |
| 单文件 | `ljnljn.android-contacts2.file` | db, sqlite |

## 覆盖范围

- 只读查询 calls 表：_id、number、date、duration、type、name、cached_name（按存在列查询）
- 总行数与按 (number,date,type,duration) 去重后的行数
- 每个号码的通话次数与 duration 合计（原始单位）
- date 最早/最晚原始值与秒/毫秒数量级判定依据（不换算绝对时间、不推断时区）
- type 分布：1 来电 / 2 去电 / 3 未接 等，未知类型原样记录
- 缺表、缺列、空表、坏数据库时给出明确错误或零结果 + limitations，不 panic

## 构建

仓库内置 LovelyCarve 插件 SDK（`vendor/lovelycarve-sdk`，AGPL-3.0），无需外部路径：

```bash
cargo test --locked
cargo build --release --locked
```

Windows 产物为 `target/release/ljnljn_kb_android_contacts2_calls_6c1a4f83.dll`。

## 安装

把 `plugin.toml` 与构建出的 `ljnljn_kb_android_contacts2_calls_6c1a4f83.dll` 放进 LovelyCarve 插件目录下的同名子目录，重启引擎后生效。插件使用 `isolation = "process"`。

## 取证边界

数据库结构随 Android 版本变化，缺少字段时会报告而不猜测。

插件只读检材，不执行检材内容，不访问网络。已通过仓库内单元测试与 release 构建；尚未在真实 LovelyCarve GUI 与真实案件检材上完成端到端验证。

## 许可证

AGPL-3.0-only，见 [LICENSE](LICENSE)。内置 SDK 保留其原始许可证与协议文档。
