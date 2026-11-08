//! 启动失败的**结构**（M161：presenter 只给阶段 + 原文，文案由界面按 i18n key 现求）。
//!
//! 从 `dto/instances.rs` 拆出：那边贴着 400 行硬线，而这两个类型是「启动」这条线上的
//! 独立一块（磁贴状态行、收尾事件、Toast 三处都读它）。

use serde::{Deserialize, Serialize};

/// 启动失败发生在**哪一步**（M161：界面文案按它选 i18n key，presenter 不写散文）。
///
/// 分档只在「用户该去做什么」不同的地方切，不按代码结构切：
///
/// | 档 | 意思 | 用户下一步 |
/// |---|---|---|
/// | [`Self::VersionManifest`] | `versions/<id>.json` 读不出来 / 解析不了 | 去版本管理重装这个版本 |
/// | [`Self::Inheritance`] | 继承链断了（父版本缺失） | 同上（父版本缺了） |
/// | [`Self::InstanceConfig`] | 实例设置文件读不出来 | 去实例页看一眼那个实例 |
/// | [`Self::Isolation`] | 隔离目录派生失败（路径/权限） | 换隔离档位或修目录权限 |
/// | [`Self::Identity`] | 账户装配失败（找不到账户 / 凭据过期） | 去账户页重新登录 |
/// | [`Self::Spawn`] | 命令行装配完了但进程起不来 | 看 Java 与日志 |
///
/// 「跑起来了但崩了」不在这张表里：那是**结局**，由 `LogOutcome` 表达（三个变体各有 i18n key）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchFailureStage {
    /// 版本 JSON 读不出来 / 解析不了
    VersionManifest,
    /// 继承链断裂
    Inheritance,
    /// 实例设置文件读不出来
    InstanceConfig,
    /// 隔离目录派生失败
    Isolation,
    /// 账户装配失败
    Identity,
    /// 进程起不来
    Spawn,
}

impl LaunchFailureStage {
    /// 稳定 slug（日志 / 事件流 / 测试用；**不要改名**）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::VersionManifest => "version_manifest",
            Self::Inheritance => "inheritance",
            Self::InstanceConfig => "instance_config",
            Self::Isolation => "isolation",
            Self::Identity => "identity",
            Self::Spawn => "spawn",
        }
    }
}

/// 一次启动失败：**哪一步** + 那一步的原文。
///
/// `detail` 是当场的原始错误文本（core / io / 账户层给的），**故意不翻译**：里面有路径、
/// 版本 id、HTTP 状态这类机器细节，翻成人话就丢了取证价值。要本地化的是「哪一步」
/// （[`LaunchFailureStage`]），界面按它选词。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFailureView {
    /// 失败阶段
    pub stage: LaunchFailureStage,
    /// 原始错误文本（不翻译）
    pub detail: String,
}

impl LaunchFailureView {
    /// 建一条。
    pub fn new(stage: LaunchFailureStage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            detail: detail.into(),
        }
    }
}

/// 磁贴/详情上「这一场为什么是失败态」的**完整原因**。
///
/// 两种**互斥**的形状，混成一种就会说假话（M161 复审 P1-1 抓的就是这个）：
///
/// | 形状 | 意思 | 界面该说什么 |
/// |---|---|---|
/// | [`Self::Stage`] | 进程**没起来**（装配/启动阶段失败） | 「启动失败 · <哪一步>：<原文>」 |
/// | [`Self::Crashed`] | 进程起来过，**崩了** | 「启动失败 · 游戏崩溃（退出码 N）」 |
/// | [`Self::SilentFailure`] | 进程起来过，但 exit 0 却没真正跑起来 | 「启动失败 · 退出码 0，但游戏没起来」 |
///
/// 为什么把结局也放在这里：磁贴的状态行与 Toast 必须说同一件事。早先「起来了又崩」被
/// 硬塞进 [`LaunchFailureStage::Spawn`]（因为 `LaunchFinished.failure` 在这次是 `None`），
/// 于是磁贴写「启动游戏进程失败」而 Toast 写「游戏崩溃」——同一时刻两句话打架，而且
/// `detail` 还漏出了 `exit Some(1)` 这种 Rust Debug 文本。
///
/// `detail` 只在 [`Self::Stage`] 里有：那是 core / io 给的原文（路径、版本 id、os error），
/// **故意不翻译**。结局类没有什么「原文」可给，退出码是数字，措辞由界面按语言出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchFailureCause {
    /// 装配/启动阶段失败：进程**没起来**
    Stage {
        /// 失败在上面的哪一步
        stage: LaunchFailureStage,
        /// 那一步的原始错误文本（不翻译）
        detail: String,
    },
    /// 进程起来过，结局是崩溃
    Crashed {
        /// 退出码（`None` = 平台没给）
        exit_code: Option<i32>,
    },
    /// 进程起来过，但疑似静默失败（exit 0 却没真正跑起来）
    SilentFailure {
        /// 退出码（通常是 0）
        exit_code: Option<i32>,
    },
}

impl LaunchFailureCause {
    /// 稳定 slug（日志 / 测试用；**不要改名**）。
    #[must_use]
    pub const fn slug(&self) -> &'static str {
        match self {
            Self::Stage { .. } => "stage",
            Self::Crashed { .. } => "crashed",
            Self::SilentFailure { .. } => "silent_failure",
        }
    }

    /// 进程到底有没有起来过。
    #[must_use]
    pub const fn started(&self) -> bool {
        !matches!(self, Self::Stage { .. })
    }

    /// 由一次收尾组装原因。
    ///
    /// `failure` 有值 = 装配/启动阶段失败（进程没起来）；`None` = 跑起来了，此时**只能**
    /// 看 `outcome`（`Ok` 不该走到这里，兜底按崩溃处理）。
    #[must_use]
    pub fn of(
        outcome: crate::dto::LogOutcome,
        failure: Option<&LaunchFailureView>,
        exit_code: Option<i32>,
    ) -> Self {
        match failure {
            Some(failure) => Self::Stage {
                stage: failure.stage,
                detail: failure.detail.clone(),
            },
            None if outcome == crate::dto::LogOutcome::SilentFail => {
                Self::SilentFailure { exit_code }
            }
            None => Self::Crashed { exit_code },
        }
    }
}
