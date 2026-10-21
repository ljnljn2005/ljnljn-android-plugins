//! `file_analyzer` 实现：物化逻辑文件 → 只读 SQLite 统计 → LPP 结果。
//!
//! 硬约束：
//! * 插件拿不到检材真实路径，只能用 `File` 的逻辑句柄；
//! * 打开 SQLite 需要路径，因此必须用 `File::materialize()`（plugin.toml 已声明
//!   `file_materialize` 权限）；
//! * 分析结束（或失败）后租约会随 `MaterializedFile` 的 Drop 释放；
//! * 任何数据库问题都转成带上下文的 LPP 错误，不让 panic 越过 ABI 边界。

use lovelycarve_plugin::{AnalysisResult, Error, File, FileAnalyzer, LogLevel, Result};

use crate::render;
use crate::stats::{self, Failure};

/// 文件分析器。
#[derive(Debug, Default, Clone, Copy)]
pub struct Contacts2Calls;

impl FileAnalyzer for Contacts2Calls {
    fn analyzer(&self) -> &str {
        crate::ANALYZER_FILE
    }

    fn analyze(&self, file: &File) -> Result<AnalysisResult> {
        let context = file.context();
        context.check_cancelled()?;
        let source = source_name(file);
        if file.size() == 0 {
            return Err(Error::new(
                "DB_ERROR",
                format!("{source}：文件为空（0 字节），不是有效的 SQLite 3 数据库"),
            ));
        }
        if file.size() > stats::MAX_DB_BYTES {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                format!(
                    "{source}：文件大小 {} 字节超过本插件上限 {} 字节，未物化",
                    file.size(),
                    stats::MAX_DB_BYTES
                ),
            ));
        }

        // 宿主临时副本；Drop 时通过 file.release 归还租约。
        let materialized = file.materialize().map_err(|error| {
            Error::new(
                error.code,
                format!("{source}：无法取得宿主物化副本：{}", error.message),
            )
        })?;
        if materialized.size() == 0 {
            return Err(Error::new(
                "DB_ERROR",
                format!("{source}：文件为空（0 字节），不是有效的 SQLite 3 数据库"),
            ));
        }

        let outcome = stats::analyze_path(materialized.path(), &|| context.is_cancelled());
        let stats = match outcome {
            Ok(stats) => stats,
            Err(Failure::Cancelled) => return Err(Error::cancelled()),
            Err(Failure::Db(message)) => {
                return Err(Error::new("DB_ERROR", format!("{source}：{message}")));
            }
        };

        let result = render::build_result(&stats, &source, file.id());
        context.log(
            LogLevel::Debug,
            &format!(
                "contacts2.db calls: table={:?} queried={} rows={} numbers={:?} types={:?} artifacts={}",
                stats.table_name,
                stats.table_queried,
                stats.total_rows,
                stats.number_count,
                stats.type_count,
                result.artifacts.len()
            ),
        );
        drop(materialized);
        Ok(result)
    }
}

/// 宿主只给显示名；空名时退化为逻辑 file id。反斜杠统一成正斜杠便于展示。
fn source_name(file: &File) -> String {
    let name = file.name().trim();
    if name.is_empty() {
        format!("file:{}", file.id())
    } else {
        name.replace('\\', "/")
    }
}
