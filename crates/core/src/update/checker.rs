//! 检查更新：把「清单 + 当前版本 + 渠道 + 目标平台」收敛成一个结论。
//!
//! 判定顺序（每一步都是纯函数，可单测）：
//!
//! 1. 取清单（源没有该渠道清单 = 还没发过版 = 已是最新）；
//! 2. 清单自报渠道与请求渠道不符 → [`UpdateErrorKind::ChannelMismatch`]（拿错文件是源的问题，
//!    不能悄悄当成「没有更新」）；
//! 3. 准入过滤：[`Channel::admits`]（清单渠道阶梯）+ [`Channel::admits_stage`]（版本号里的
//!    预发布阶段）**同时**成立。清单可以撒谎，版本号不会；
//! 4. 只留版本号**严格大于**当前版本的候选（降级不做，nightly 想回退请手工下载）；
//! 5. 按版本号从高到低找**第一个有本平台产物**的候选：命中就是 [`CheckOutcome::Available`]，
//!    一个都没有则 [`CheckOutcome::MissingArtifact`]（有新版但没给你的平台发包，这是发布侧的
//!    漏项，不能对用户说「已是最新」）；
//! 6. 候选为空 → [`CheckOutcome::UpToDate`]。
//!
//! 强制更新（[`Urgency::Forced`]）有两个来源：发布自带的 `force`，或当前版本低于该发布的
//! `min_supported`（发布侧收回老版本支持）。两者都不看渠道——强制就是强制。

use std::path::Path;

use super::channel::Channel;
use super::error::{UpdateError, UpdateErrorKind};
use super::manifest::{Artifact, Release, ReleaseManifest};
use super::source::UpdateSource;
use super::target::Target;
use super::verify::{stage, StagedPackage};
use super::version::Version;

/// 这次更新急不急。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// 可选：用户想升再升。
    Optional,
    /// 强制：`force` 或当前版本低于发布侧的最低支持版本。
    Forced,
}

impl Urgency {
    /// 稳定字符串 id（事件流用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Optional => "optional",
            Self::Forced => "forced",
        }
    }
}

/// 一个可安装的候选：哪个发布、哪个产物、急不急。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    /// 发布信息（版本、说明、产物列表）。
    pub release: Release,
    /// 本平台要装的那个产物。
    pub artifact: Artifact,
    /// 急不急。
    pub urgency: Urgency,
}

/// 检查更新的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// 已是最新。
    UpToDate {
        /// 当前版本。
        current: Version,
        /// 该渠道内准入的最高版本（清单里没有可用发布时为 `None`）。
        latest: Option<Version>,
    },
    /// 有可装的更新。
    Available(Box<UpdateCandidate>),
    /// 有更高的版本，但里面没有本平台的产物。
    MissingArtifact {
        /// 那个更高版本（没有本平台产物的那份发布）。
        release: Box<Release>,
    },
}

impl CheckOutcome {
    /// 是否有可装的更新。
    #[must_use]
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Available(_))
    }

    /// 可安装候选（没有则为 `None`）。
    #[must_use]
    pub fn candidate(&self) -> Option<&UpdateCandidate> {
        match self {
            Self::Available(candidate) => Some(candidate),
            _ => None,
        }
    }
}

/// 检查更新的入口：绑定一个源与一个目标平台，之后每次检查只传「当前版本 + 渠道」。
#[derive(Debug, Clone)]
pub struct UpdateChecker<S> {
    source: S,
    target: Target,
}

impl<S: UpdateSource> UpdateChecker<S> {
    /// 用显式目标平台构造（跨平台代查、测试都靠它）。
    #[must_use]
    pub fn new(source: S, target: Target) -> Self {
        Self { source, target }
    }

    /// 用当前编译目标构造。
    #[must_use]
    pub fn for_current_target(source: S) -> Self {
        Self::new(source, Target::current())
    }

    /// 绑定的源。
    #[must_use]
    pub const fn source(&self) -> &S {
        &self.source
    }

    /// 绑定的目标平台。
    #[must_use]
    pub const fn target(&self) -> Target {
        self.target
    }

    /// 查一次更新。
    ///
    /// # Errors
    ///
    /// 清单取不到、清单不合法、渠道不符都返回 [`UpdateError`]。
    /// 「没有更新」不是错误，走 [`CheckOutcome::UpToDate`]。
    pub fn check(&self, current: &Version, channel: Channel) -> Result<CheckOutcome, UpdateError> {
        let Some(manifest) = super::source::verified_manifest(&self.source, channel)? else {
            return Ok(CheckOutcome::UpToDate {
                current: current.clone(),
                latest: None,
            });
        };
        if manifest.channel != channel {
            return Err(UpdateError::new(
                UpdateErrorKind::ChannelMismatch,
                format!("请求的是 {channel} 渠道，清单自报 {}", manifest.channel),
            ));
        }
        let admitted = admitted(&manifest, channel).collect::<Vec<_>>();
        let latest = admitted.iter().map(|release| release.version.clone()).max();
        let mut candidates = admitted
            .into_iter()
            .filter(|release| release.version > *current)
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.version.cmp(&left.version));

        for release in &candidates {
            if let Some(artifact) = release.artifact_for(self.target) {
                return Ok(CheckOutcome::Available(Box::new(UpdateCandidate {
                    urgency: urgency_of(release, current),
                    release: (*release).clone(),
                    artifact: artifact.clone(),
                })));
            }
        }
        match candidates.first() {
            Some(release) => Ok(CheckOutcome::MissingArtifact {
                release: Box::new((*release).clone()),
            }),
            None => Ok(CheckOutcome::UpToDate {
                current: current.clone(),
                latest,
            }),
        }
    }

    /// 把候选的包体取下来、落暂存、校验。成功返回的 [`StagedPackage`] 可直接交给
    /// [`super::install::install_staged`]。
    ///
    /// # Errors
    ///
    /// 取包失败透传；长度或摘要不符分别返回 `SizeMismatch` / `DigestMismatch`，
    /// 且暂存文件会被删掉（见 [`stage`]）。
    pub fn prepare(
        &self,
        candidate: &UpdateCandidate,
        staging_dir: &Path,
    ) -> Result<StagedPackage, UpdateError> {
        stage(&self.source, &candidate.artifact, staging_dir)
    }
}

/// 该渠道准入的全部发布（不看版本高低）。
fn admitted(manifest: &ReleaseManifest, channel: Channel) -> impl Iterator<Item = &Release> {
    manifest.releases.iter().filter(move |release| {
        channel.admits(release.channel) && channel.admits_stage(release.version.stage())
    })
}

fn urgency_of(release: &Release, current: &Version) -> Urgency {
    if release.force || release.below_min_supported(current) {
        Urgency::Forced
    } else {
        Urgency::Optional
    }
}
