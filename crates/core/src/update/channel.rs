//! 更新渠道（通道）与准入规则。
//!
//! 四个渠道是一条**稳定度阶梯**，`Ord` 的派生顺序就是阶梯顺序
//! （`Stable < Beta < Alpha < Nightly`），所以「本渠道收不收那份发布」只要一次比较：
//! `manifest_channel <= requested_channel`。nightly 收全部，stable 只收 stable。
//!
//! 除了清单自报的渠道，还要看版本号里的预发布阶段（[`Stage`]）：清单可以撒谎
//! （`channel: "stable"` 但版本是 `1.0.0-alpha.2`），阶段这条判据不信清单只信版本号本身。
//! 两道判据**同时**成立才准入，见 [`Channel::admits`] 与 [`Channel::admits_stage`]。
//!
//! 准入阶梯（与渠道顺序同一套口径，别在别处另立一张表）：
//!
//! | 渠道 | 收正式版 | 收 rc | 收 beta | 收 alpha | 收认不出的预发布 |
//! | ---- | -------- | ----- | ------- | -------- | ---------------- |
//! | stable  | ✓ |   |   |   |   |
//! | beta    | ✓ | ✓ |   |   |   |
//! | alpha   | ✓ | ✓ | ✓ |   |   |
//! | nightly | ✓ | ✓ | ✓ | ✓ | ✓ |

use std::fmt;
use std::str::FromStr;

use super::error::{UpdateError, UpdateErrorKind};
use super::version::Stage;

/// 更新渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    /// 稳定版（默认渠道）。
    Stable,
    /// 公测版。
    Beta,
    /// 内测版。
    Alpha,
    /// 每日构建（收所有预发布，包括认不出的标识）。
    Nightly,
}

impl Channel {
    /// 全部渠道，从最稳到最激进。
    pub const ALL: [Self; 4] = [Self::Stable, Self::Beta, Self::Alpha, Self::Nightly];

    /// 清单文件名与日志用的稳定字符串 id（**不要改名**：清单里的 `channel` 字段就是它）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
            Self::Nightly => "nightly",
        }
    }

    /// 解析渠道名（大小写与首尾空白不敏感）。
    ///
    /// # Errors
    ///
    /// 认不出的名字返回 [`UpdateErrorKind::InvalidManifest`]（清单里的 `channel` 写错了）。
    pub fn parse(text: &str) -> Result<Self, UpdateError> {
        let name = text.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|channel| channel.as_str() == name)
            .ok_or_else(|| {
                let allowed = Self::ALL
                    .iter()
                    .map(|channel| channel.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ");
                UpdateError::new(
                    UpdateErrorKind::InvalidManifest,
                    format!("未知更新渠道 `{text}`（只认 {allowed}）"),
                )
            })
    }

    /// 本渠道是否接受「发布自报 `release` 渠道」的那份版本。
    ///
    /// 判定就是阶梯比较（`Ord` 的派生顺序即阶梯），不在别处另立一张表。
    #[must_use]
    pub fn admits(self, release: Self) -> bool {
        release <= self
    }

    /// 本渠道是否接受该预发布阶段（正式版对所有渠道都成立）。
    #[must_use]
    pub const fn admits_stage(self, stage: Stage) -> bool {
        matches!(
            (self, stage),
            (Self::Stable, Stage::Stable)
                | (Self::Beta, Stage::Stable | Stage::ReleaseCandidate)
                | (
                    Self::Alpha,
                    Stage::Stable | Stage::ReleaseCandidate | Stage::Beta
                )
                | (Self::Nightly, _)
        )
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Channel {
    type Err = UpdateError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}
