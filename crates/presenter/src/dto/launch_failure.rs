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

/// 「老版本**原版** + 高版本 Java」那条建议的**证据强度**（M349）。
///
/// 与 core 的 `LegacyAdvisoryTier` 一对一（**不带内部 code**：事件里传的是枚举，界面按它选 key）：
/// 只有我们真跑过的那个版本算 `Measured`，其余 `<1.13` 原版一律 `Unmeasured`——判据在 core 的
/// `game::java::advisory::legacy_advisory_facts`，presenter 一个字都不重判。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdvisoryTier {
    /// 这一组（版本 + 更高主版本 Java）真跑过：到主菜单，进世界未验
    Measured,
    /// 同一条放宽下的其余 `<1.13` 原版：**一次都没实测过**
    Unmeasured,
}

/// 建议句里那组**证据**：我们到底在哪个组合上实测过（M349 r2）。
///
/// 为什么它必须从 presenter 过来、而不是界面自己写一句「1.12.2 + Java 17」：那是 core 的两个常量
/// （`MEASURED_LEGACY_VERSION` / `MEASURED_LEGACY_JAVA`），命令行那句与界面那句读的是**同一批
/// i18n key**，证据数字只能有一个来源——写死在目录里，改常量时两侧就漂（复审 P2-2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvisoryEvidence {
    /// 实测过的那个版本 id（`1.12.2`）
    pub version_id: String,
    /// 实测时用的 Java 主版本（`17`）
    pub java: u32,
}

/// 「Java 与目标版本不符」的方向（M331；与 core 的 `JavaMismatch` 一对一，**不带内部 code**）。
///
/// 为什么方向要显式过来而不是让界面拿三个数字自己推：`check` 的判据（先看下限、再看上限）
/// 在 core，界面只该挑句子。内部的 `java_too_old` / `java_too_new` 那种 code 也不上屏
/// ——它在这里只是一个枚举分支，界面按它选 i18n key 的半句。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JavaGuardDirection {
    /// 太老：该版本要求 `required_min` 及以上
    TooOld,
    /// 太新：该版本配 LaunchWrapper 系加载器（Forge / LiteLoader）时只能用 `required_max` 及以下
    /// （原版 `<1.13` 没有上限，不会再走到这一支 —— M348）
    TooNew,
    /// **只是建议**（M349）：`<1.13` 的原版配了比设计值更高的 Java。**不拦**，用户可以直接继续。
    ///
    /// 与另外两支共用同一个「启动前的框」（三个出口一样：去设置改 Java / 仍要启动 / 下载 Java N），
    /// 差别只在正文与标题——这就是为什么它是同一个枚举的第三支，而不是第二套对话框。
    Advisory(AdvisoryTier),
}

/// 一次**被拦下**的启动：Java 与目标版本不符，且用户没显式指定要试那个 Java（M331）。
///
/// ## 为什么不复用 [`LaunchFailureStage`] / [`LaunchFailureCause`]
///
/// 那两族说的都是「**受理了、起了、然后**在某一步失败或崩了」——磁贴据此写「启动失败」。
/// 拦下是**根本没有受理进程**（`Launcher::spawn` 一次都没调用），说成「启动失败」就是
/// 在界面上说假话：用户会去翻日志找崩溃现场，而那里一个字都没有。所以这是第三种事实，
/// 单独一档，界面弹的是**启动前**的对话框（三个出口：去设置改 Java / 仍要启动 /
/// 下载 Java {install_major}——最后那个是 M334 加的），不是崩溃卡片。
///
/// ## M349：它也承载「只是建议」那一条
///
/// `direction == Advisory(_)` 时语义是「**没有拦**，只是先问一句」：老版本原版配更高的 Java
/// 不在任何硬约束里（M348），但一句话都不说就是「静默放行」。两条共用这一个结构，是因为
/// 界面上它们是**同一个框**（同一套三枚出口、同一段生命周期、同一个「仍在启动表之外」的位置），
/// 分成两个 DTO 就会有两份 `open_*_dialog`、两份按钮分流。
///
/// ⚠️ 名字里的 `Blocked` 因此是历史包袱（改名要动 presenter 的 `state` 与 app-iced 的
/// `update`，都不在 M349 的 scope 里）。语义以 `direction` 为准：`Blocked` 这个事件名现在读作
/// 「启动前的一道闸」——拦下或先问一句。
///
/// ## 为什么一个字的散文都没有
///
/// 与 [`crate::dto::LaunchFailureView`] 同一条纪律：presenter 不依赖 `unlucky-i18n`，
/// 任何写死的中文都会在英文界面上露出来。这里只给结构（哪个实例、什么方向、几个版本号），
/// 整句由界面按语言现求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchBlockedView {
    /// 实例 id（「仍要启动」按它放行：不靠上下文猜，并发点两条也不会放错）
    pub id: crate::dto::InstanceId,
    /// 实例名（对话框里指名，用户才知道是哪一条）
    pub name: String,
    /// 绑定的版本 id（`1.12.2`）
    pub version_id: String,
    /// 不符的方向（第三支 = 只是建议，M349）
    pub direction: JavaGuardDirection,
    /// 目标版本要求的 Java 下限
    pub required_min: u32,
    /// 要求的 Java 上限（`None` = 没有上限）
    pub required_max: Option<u32>,
    /// 这次挑中的 Java 主版本
    pub actual: u32,
    /// 该给这个版本装哪个大版本的 Java（「下载 Java {major}」那个出口用的数字）。
    ///
    /// 由 presenter 从 [`JavaGuardDirection`] 与那两个数算出来，**界面不自己推**：
    /// `TooOld` ⇒ 要下限（`required_min`）；`TooNew` ⇒ 要上限（`maximum`——`TooNew` 这一支
    /// 必然带着上限，core 的 `JavaMismatch::TooNew { maximum, .. }` 里那一个）；
    /// `Advisory` ⇒ 下限（老版本原版该用的就是那个 8）。
    ///
    /// ## `None` = **这个出口落地不了，别画它**（M349 r3 / 复审 r2 P2-1）
    ///
    /// 只有一种情形：**实例层钉了 `launch.javaPath`**。那份钉死的 Java 优先级最高
    /// （`pick_java` 的显式分支只探测它），装进托管目录的任何一份都**不会被挑中**——画一枚
    /// 「下载 Java 8」出来，用户点完装好、再点「现在启动」，回来的还是同一个框（复审实测过：
    /// 连点两次、两次都弹、一次都没起进程）。所以这里不给数字 = 不给那枚按钮；界面改给
    /// 「仍要启动」+「去实例启动参数」两个**都能落地**的出口。
    ///
    /// 用 `Option` 而不是 `installable: bool` + 一个数字：两个字段会互相矛盾（`false` 却带着
    /// `8`），而「不可装」本来就没有数字可给。
    pub install_major: Option<u32>,
    /// 建议句里的那组**证据**（`Advisory` 那一支才有；硬拦那一支是 `None`）。
    ///
    /// 值来自 core 的 `MEASURED_LEGACY_VERSION` / `MEASURED_LEGACY_JAVA`——界面拿它填
    /// `{measured_version}` / `{measured_java}`，于是改常量时命令行与界面一起变。
    pub evidence: Option<AdvisoryEvidence>,
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
