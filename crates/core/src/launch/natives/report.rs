//! natives 的报告与错误：真解压与预览共用的 [`NativesReport`] / [`NativesError`]。
//!
//! 单独成文件是为了让 `mod.rs` 只留「计划 + 两个入口（`extract` / `preview`）」，
//! 本仓的红线是单文件 ≤400 行。

use std::path::PathBuf;

use super::NativesEra;

/// 解压结果 / 预览结果（进 `natives_extracted` 事件与 `--dry-run` 输出）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativesReport {
    /// 目标目录。
    pub dir: PathBuf,
    /// 时代。
    pub era: NativesEra,
    /// 参与的 jar 数。
    pub jars: usize,
    /// 原生库文件数：真解压时是**本次写下**的数量，预览时是**将会写下**的数量。
    pub files: usize,
    /// 按戳记跳过、未重新解压的文件数（= 上次解压的文件数）。
    pub skipped: usize,
    /// 被排除 / 非原生库而忽略的条目数（`META-INF/`、签名文件）。
    pub ignored: usize,
    /// 目录是否用户自定义。
    pub custom: bool,
    /// 是否按戳记判定「目录已就绪」而整体跳过。
    pub up_to_date: bool,
    /// 报告是否来自只读预览（[`super::preview`]）。true = 盘上零改动，`files` 只是预计值。
    pub planned: bool,
    /// 报告生成时目标目录是否已在盘上。
    ///
    /// 预览时这是**盘上真值**（用于区分「目录已就绪」与「真启动才会建出来」）；
    /// 真解压路径在 `create_dir_all` 之后恒为 true。
    pub dir_exists: bool,
}

impl NativesReport {
    /// 人读一行（CLI / 日志用）。
    pub fn summary(&self) -> String {
        if self.planned {
            return self.planned_summary();
        }
        if self.jars == 0 {
            return format!(
                "natives 目录已就绪（该版本无需解压，LWJGL 运行期自取）→ {}",
                self.dir.display()
            );
        }
        if self.up_to_date {
            return format!(
                "natives 已就绪（{} 个原生库，{} 个 jar 未变）",
                self.skipped, self.jars
            );
        }
        format!(
            "natives 解压 {} 个原生库（{} 个 jar，忽略 {} 个条目）→ {}",
            self.files,
            self.jars,
            self.ignored,
            self.dir.display()
        )
    }

    /// 预览版人读一行：句句带「将 / 未落盘」，别让用户把预览当成已经解好了。
    fn planned_summary(&self) -> String {
        if self.jars == 0 {
            return if self.dir_exists {
                format!(
                    "natives 目录已就绪（该版本无需解压，LWJGL 运行期自取）→ {}",
                    self.dir.display()
                )
            } else {
                format!(
                    "natives 目录尚不存在，真启动会建出来（--dry-run 未落盘）→ {}",
                    self.dir.display()
                )
            };
        }
        if self.up_to_date {
            return format!(
                "natives 已就绪（{} 个原生库，{} 个 jar 未变；--dry-run 未落盘）",
                self.skipped, self.jars
            );
        }
        format!(
            "将解压 {} 个原生库（{} 个 jar，忽略 {} 个条目）→ {}（--dry-run 未落盘）",
            self.files,
            self.jars,
            self.ignored,
            self.dir.display()
        )
    }

    /// `--json` 的 `natives` 块（`ull launch --dry-run` 用；字段名是接口的一部分）。
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "dir": self.dir.display().to_string(),
            "files": self.files,
            "skipped": self.skipped,
            "ignored": self.ignored,
            "jars": self.jars,
            "custom": self.custom,
            "up_to_date": self.up_to_date,
            "era": self.era.as_str(),
            "planned": self.planned,
            "dir_exists": self.dir_exists,
        })
    }
}

/// 解压失败原因。
#[derive(Debug)]
pub enum NativesError {
    /// 目标目录为空（调用方漏填 `natives_dir`）。
    NoDir,
    /// 文件系统操作失败。
    Io {
        /// 操作名。
        op: &'static str,
        /// 相关路径。
        path: PathBuf,
        /// 底层错误。
        source: std::io::Error,
    },
    /// natives jar 打不开 / 不是合法 zip（下载损坏的最常见形态）。
    Zip {
        /// jar 路径。
        path: PathBuf,
        /// 原因。
        reason: String,
    },
}

impl NativesError {
    /// 一行人读描述。
    pub fn message(&self) -> String {
        match self {
            Self::NoDir => "natives 目录未配置（natives_dir 为空）".to_string(),
            Self::Io { op, path, source } => {
                format!("{op} 失败（{}）：{source}", path.display())
            }
            Self::Zip { path, reason } => {
                format!(
                    "natives jar 无法作为 zip 打开（{}）：{reason}",
                    path.display()
                )
            }
        }
    }
}

impl std::fmt::Display for NativesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for NativesError {}
