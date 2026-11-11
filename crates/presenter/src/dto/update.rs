//! 自更新 DTO（M159）：一次检查的结论、候选、失败分类、就位状态。
//!
//! 口径四条：
//!
//! 1. **「检查失败」不是「已是最新」**——两者是 [`UpdateOutcome`] 的两个变体，各有自己的文案。
//!    把它们折叠成一个（例如失败时也发 `UpToDate`）会让用户以为「已经最新」，而网络其实一次都没通；
//! 2. **「有更高版本但没给你的平台发包」也不是「已是最新」**（[`UpdateOutcome::MissingArtifact`]）：
//!    那是发布侧的漏项，说成「最新」等于把发布的错报成用户的好消息；
//! 3. **版本比较只发生在 core**（[`unlucky_core::update::UpdateChecker`]）：本模块只搬运结论，
//!    UI 与 CLI 都不许自己比版本号（mission：GUI 与 CLI 行为一致，不许两套逻辑）；
//! 4. 本模块不含 core 类型（D42：dto 只放扁平 IPC 类型）。
//!
//! 「忽略这个版本」（[`UpdateView::skipped`]）记的是**版本号**，不是「别再问我了」：
//!
//! * 用户点「忽略这个版本」时写下**当时那个版本号**（会话级，不落盘）；
//! * 同一次运行里如果又出现**更高**的版本（0.2.3 被忽略后冒出 0.2.4），提示条**照常弹**——
//!   忽略的是那一个版本，文案也确实是「忽略这个版本」；
//! * 设置页的「立即检查」会清掉它（手动检查 = 用户要看真结果），静默检查**永不**清。

use serde::{Deserialize, Serialize};

/// 检查更新的一次结论。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UpdateOutcome {
    /// 还没查过（启动那一刻的状态）。
    #[default]
    Idle,
    /// 正在查（网络在后台线程）。
    Checking,
    /// 已是最新。
    UpToDate {
        /// 该渠道准入的最高版本；清单里还没有任何发布时为 `None`（「还没发过版」不是错误）。
        latest: Option<String>,
    },
    /// 有可装的更新。
    Available(Box<UpdateCandidateView>),
    /// 有更高的版本，但里面没有本平台的产物（发布侧漏项，不能对用户说「已是最新」）。
    MissingArtifact {
        /// 那个更高版本。
        version: String,
    },
    /// 检查失败（连不上 / 清单坏了 / 渠道不符 …）。
    Failed(UpdateFailureView),
}

impl UpdateOutcome {
    /// 是不是正在查。
    #[must_use]
    pub const fn is_checking(&self) -> bool {
        matches!(self, Self::Checking)
    }

    /// 可装的候选（没有则为 `None`）。
    #[must_use]
    pub fn candidate(&self) -> Option<&UpdateCandidateView> {
        match self {
            Self::Available(candidate) => Some(candidate),
            _ => None,
        }
    }

    /// 失败信息（不是失败则为 `None`）。
    #[must_use]
    pub const fn failure(&self) -> Option<&UpdateFailureView> {
        match self {
            Self::Failed(failure) => Some(failure),
            _ => None,
        }
    }

    /// 稳定 id（日志 / 测试 / 事件流用；**不要改名**）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Checking => "checking",
            Self::UpToDate { .. } => "up_to_date",
            Self::Available(_) => "available",
            Self::MissingArtifact { .. } => "missing_artifact",
            Self::Failed(_) => "failed",
        }
    }
}

/// 一个可安装的候选（提示条与设置页共用的四件）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCandidateView {
    /// 新版本号（清单里的原文，如 `0.2.3`）。
    pub version: String,
    /// 包体字节数（提示条上的「体积」）。
    pub size: u64,
    /// 更新说明摘要（清单里的 `notes`，已截断；空串 = 发布没写说明）。
    pub notes: String,
    /// 急不急（`true` = 发布标了强制，或当前版本低于发布侧的最低支持版本）。
    pub forced: bool,
}

/// 检查失败的分类与人话。
///
/// `message` 直接来自 core 的错误（中文、含路径/URL 细节），UI 只在它前面加一句自己的话；
/// `kind` 是 core 的稳定分类 id（`network` / `invalid_manifest` / `digest_mismatch` …），
/// 给「要不要显示重试」这类判据用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateFailureView {
    /// 稳定分类 id（`UpdateErrorKind::as_str`）。
    pub kind: String,
    /// 人话原因。
    pub message: String,
    /// 换个时间/换个源再试是否可能变好。
    pub retryable: bool,
}

impl UpdateFailureView {
    /// 建一条。
    pub fn new(kind: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            kind: kind.to_string(),
            message: message.into(),
            retryable,
        }
    }
}

/// 已经就位的新版本（下载 + 校验 + 替换都成功，等用户重启）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadyUpdate {
    /// 新版本号。
    pub version: String,
    /// 替换后的目标路径（人读展示；「下次启动生效」说的就是它）。
    pub path: String,
    /// 旧版本被挪到哪（`None` = 原本没有文件 / 已清理）。
    pub backup: Option<String>,
}

/// 提示条此刻要显示什么。
///
/// 「检查失败」只有在**用户主动查**的时候才弹条（[`UpdateView::silent`] = `false`）：
/// 启动时的静默检查失败是常态（离线、公司网、镜像没同步），拿一条红条迎面上来等于每次开
/// 启动器先骂网络一次。失败原因仍然进设置页的「更新」区，那里随时能看。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateBanner<'a> {
    /// 有新版本（可下载）。
    Available(&'a UpdateCandidateView),
    /// 正在下载/校验/替换。
    Applying,
    /// 已就位，等重启。
    Ready(&'a ReadyUpdate),
    /// 检查/应用失败（只在用户主动查之后出现）。
    Failed(&'a UpdateFailureView),
}

/// 更新页 / 提示条的全部状态（一条快照整体替换）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateView {
    /// 当前版本（`CARGO_PKG_VERSION`，由 host 填）。
    pub current_version: String,
    /// 渠道（D50：先只发 stable）。
    pub channel: String,
    /// 启动时是否静默检查（设置页开关）。
    pub auto_check: bool,
    /// 检查结论。
    pub outcome: UpdateOutcome,
    /// 上次检查时间（`MM-DD HH:MM`；空串 = 没查过）。
    pub checked_at: String,
    /// 这一份结论来自**静默**检查（启动时/自动）而不是用户点的「立即检查」。
    pub silent: bool,
    /// 正在下载/校验/替换（一条慢操作，期间禁用「立即安装」）。
    pub applying: bool,
    /// 已经就位的新版本（等重启）。
    pub ready: Option<ReadyUpdate>,
    /// 用户点了「忽略这个版本」时记下的**版本号**（`None` = 没忽略）。
    ///
    /// 只压住**这一个**版本：0.2.3 被忽略后 0.2.4 仍然提示（文案是「忽略这个版本」，
    /// 不是「别再问我」）。设置页手动检查会把它清空。
    pub skipped: Option<String>,
}

impl Default for UpdateView {
    /// 出厂状态：当前版本取本进程的版本号（**不是空串**——mock 后端/未接线的路径下，
    /// 设置页的「当前版本」那一行至少要显示自己的版本，而不是一片空白）。
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"), "stable", true)
    }
}

impl UpdateView {
    /// 建一份初始状态。
    #[must_use]
    pub fn new(current: impl Into<String>, channel: &str, auto_check: bool) -> Self {
        Self {
            current_version: current.into(),
            channel: channel.to_string(),
            auto_check,
            outcome: UpdateOutcome::Idle,
            checked_at: String::new(),
            silent: false,
            applying: false,
            ready: None,
            skipped: None,
        }
    }

    /// 提示条此刻要显示什么（`None` = 不显示）。
    ///
    /// 优先级：就位 > 进行中 > 失败（仅主动查） > 有新版本（未被忽略）。
    #[must_use]
    pub fn banner(&self) -> Option<UpdateBanner<'_>> {
        if let Some(ready) = &self.ready {
            return Some(UpdateBanner::Ready(ready));
        }
        if self.applying {
            return Some(UpdateBanner::Applying);
        }
        if let Some(failure) = self.outcome.failure() {
            return (!self.silent).then_some(UpdateBanner::Failed(failure));
        }
        self.outcome
            .candidate()
            .filter(|candidate| self.skipped.as_deref() != Some(candidate.version.as_str()))
            .map(UpdateBanner::Available)
    }

    /// 有没有「需要用户看一眼」的结论（提示条 / 就位后的重启提示 / 强制更新）。
    #[must_use]
    pub fn needs_attention(&self) -> bool {
        self.banner().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(version: &str) -> UpdateCandidateView {
        UpdateCandidateView {
            version: version.to_string(),
            size: 21_000_000,
            notes: "修掉启动崩溃".to_string(),
            forced: false,
        }
    }

    #[test]
    fn failure_is_not_up_to_date() {
        let mut view = UpdateView {
            outcome: UpdateOutcome::Failed(UpdateFailureView::new(
                "network",
                "取更新清单失败：连接被重置",
                true,
            )),
            ..UpdateView::default()
        };
        assert!(view.outcome.failure().is_some());
        assert!(view.outcome.candidate().is_none());
        assert!(!matches!(view.outcome, UpdateOutcome::UpToDate { .. }));
        assert_eq!(view.outcome.as_str(), "failed");

        // 用户主动查出来的失败 → 提示条要说清原因（不是「已是最新」）。
        assert!(matches!(view.banner(), Some(UpdateBanner::Failed(_))));
        // 静默检查的失败 → 不弹条（离线是常态，别每次开机先骂网络一次）。
        view.silent = true;
        assert!(view.banner().is_none(), "静默检查失败不该变成提示条");
        assert!(
            view.outcome.failure().is_some(),
            "但失败原因仍留在快照里（设置页要看）"
        );
    }

    #[test]
    fn skipping_one_version_does_not_silence_the_next_one() {
        let mut view = UpdateView {
            outcome: UpdateOutcome::Available(Box::new(candidate("0.2.3"))),
            ..UpdateView::default()
        };
        assert!(matches!(view.banner(), Some(UpdateBanner::Available(_))));

        view.skipped = Some("0.2.3".to_string());
        assert!(view.banner().is_none(), "忽略 0.2.3 之后它自己不再打扰");
        // 候选没被丢掉：设置页仍能显示「有新版本 0.2.3」并手动装
        assert_eq!(
            view.outcome.candidate().map(|item| item.version.as_str()),
            Some("0.2.3")
        );

        // 同一次运行里冒出更高的版本：**照常提示**（文案是「忽略这个版本」，不是「别再问我」）。
        view.outcome = UpdateOutcome::Available(Box::new(candidate("0.2.4")));
        assert!(
            matches!(view.banner(), Some(UpdateBanner::Available(c)) if c.version == "0.2.4"),
            "0.2.3 被忽略不该压住 0.2.4"
        );
    }

    #[test]
    fn ready_state_replaces_the_banner() {
        let view = UpdateView {
            outcome: UpdateOutcome::Available(Box::new(candidate("0.2.3"))),
            ready: Some(ReadyUpdate {
                version: "0.2.3".to_string(),
                path: "C:/ULL/unlucky-launcher.exe".to_string(),
                backup: Some("C:/ULL/.unlucky-launcher.exe.ullold-1-0".to_string()),
            }),
            ..UpdateView::default()
        };
        assert!(matches!(view.banner(), Some(UpdateBanner::Ready(_))));
        assert!(view.needs_attention(), "就位之后要提示重启");
    }

    #[test]
    fn up_to_date_and_idle_show_no_banner() {
        let mut view = UpdateView::default();
        assert!(view.banner().is_none(), "Idle 还没查过，不该弹条");
        view.outcome = UpdateOutcome::UpToDate {
            latest: Some("0.2.2".to_string()),
        };
        assert!(view.banner().is_none(), "已是最新没有任何要说的");
        view.outcome = UpdateOutcome::MissingArtifact {
            version: "0.2.3".to_string(),
        };
        assert!(view.banner().is_none(), "缺产物不进提示条，进设置页原因");
    }

    #[test]
    fn view_round_trips_as_ipc() {
        let mut view = UpdateView::new("0.2.2", "stable", false);
        view.checked_at = "11-08 12:00".to_string();
        view.outcome = UpdateOutcome::UpToDate {
            latest: Some("0.2.2".to_string()),
        };
        let text = serde_json::to_string(&view).expect("序列化");
        let back: UpdateView = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, view);
        assert!(!back.auto_check);
    }
}
