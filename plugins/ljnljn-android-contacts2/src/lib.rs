//! LovelyCarve 文件分析器：Android `contacts2.db` 的 `calls` 表只读统计。
//!
//! 能力范围（唯一可离线复用、且不依赖案件特有数据）：
//!
//! 1. 对宿主给的单个逻辑文件调用 [`lovelycarve_plugin::File::materialize`]，
//!    取得临时副本后用 `rusqlite`（bundled SQLite）**只读**打开；
//! 2. 定位 `calls` 表，按实际存在的列查询；
//! 3. 输出总行数、按 `(number, date, type, duration)` 去重后的行数、每个号码的
//!    通话次数/总时长、`date` 最早/最晚原始值与数量级判定、`type` 分布；
//! 4. 缺表 / 视图 / 缺列 / 空表 / 坏数据库都给出明确错误或零结果 + limitations，
//!    不 panic。
//!
//! 明确**不做**：
//! * 不把 `date` 换算成绝对时间，不推断时区；
//! * 不解析 `contacts` 联系人表，不做号码归一化或归属地判断；
//! * 不合并 `-wal` / `-shm` / `-journal` 边车文件；
//! * 不执行检材内的 SQL 文本，不联网。
//!
//! 模块：
//! * [`stats`]：只读 SQL 统计与错误分类（纯逻辑，可用临时目录里的合成库单测）；
//! * [`render`]：把统计映射成 LPP v1 结果（结果树、artifacts、findings）；
//! * [`analyzer`]：`FileAnalyzer` 实现，只经 SDK 句柄与宿主交互。

pub mod analyzer;
pub mod render;
pub mod stats;

#[cfg(test)]
mod test_host;
#[cfg(test)]
mod tests;

use lovelycarve_plugin::{export_plugin, Plugin};

/// 插件 id，必须与 plugin.toml 的 `id` 完全一致。
pub const PLUGIN_ID: &str = "org.ljnljn.android-contacts2";
/// 显示名，必须与 plugin.toml 的 `name` 一致。
pub const PLUGIN_NAME: &str = "Android contacts2.db 通话记录统计";
/// 版本，与 Cargo 包版本和 plugin.toml 一致。
pub const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 文件分析器名，必须与 plugin.toml 的 `[[capabilities]].analyzer` 一致。
pub const ANALYZER_FILE: &str = "ljnljn.android-contacts2.file";

fn plugin() -> Plugin {
    Plugin::new(PLUGIN_ID, PLUGIN_NAME, PLUGIN_VERSION).file_analyzer(analyzer::Contacts2Calls)
}

export_plugin!(plugin);
