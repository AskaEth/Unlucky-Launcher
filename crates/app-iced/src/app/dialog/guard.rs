//! 「下载 Java {major}」这条出口的**状态机**与它的判据（M334；从 `dialog.rs` 拆出：那边贴着
//! 400 行硬线，而这一段是自成一体的纯逻辑）。
//!
//! ## 为什么要在 app 侧自己记一份
//!
//! presenter 那条通道（`CoreEvent::JavaChanged`）说的是「Java 页现在长什么样」，它不区分
//! 「正在装的这一个是不是我在这个框里点的那一个」——`busy` 一翻就照实说「装好了 / 失败了」，
//! 会把设置页里别的安装过程也算到这个框头上。所以这里只认**点过之后**的那一段。
//!
//! ## 为什么还要「代次」（复审 r1 P1）
//!
//! `Failed` 是终态；用户失败之后再点一次下载时，`state.java` 里躺着的仍然是**上一次失败
//! 留下的**旧快照（`busy = false`、没有那个大版本、链上写着上次的失败）。没有代次这道闸，
//! 「再点一次」会在**同一拍**被旧快照重新判回 `Failed`：用户点了没反应，而后台真的又起了
//! 一条安装（复审实测：连点两次 ⇒ 两条 `InstallJavaFor` 都派出去了）。
//!
//! 这里的「代次」= **快照本身变了**：{@link JavaGuardCard::consumed} 记着已经消化过的那一份，
//! 逐字段相同的一份一律丢弃。不用自增计数器，是因为它要挂在 `CoreEvent::JavaChanged` 的落地处
//! （`presenter/src/state.rs`），而那格不在本条 scope。

use unlucky_presenter::dto::{JavaChainOutcome, JavaPageView};

/// 「下载 Java {major}」这条出口的进度。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GuardInstall {
    /// 没点过（或者框刚弹出来）
    #[default]
    Idle,
    /// 已经派了 `InstallJavaFor`，还没收到任何快照
    Requested,
    /// 收到 `busy = true`：正在下载
    Running,
    /// 装好了（托管目录里已经有这个 Java {major}）
    Done,
    /// 装失败（三种**互斥**的形状，见 [`GuardInstallFailure`]）
    Failed {
        /// 哪一种失败
        failure: GuardInstallFailure,
    },
}

/// 一次「下载 Java {major}」失败**是什么形状**（M334 r1 P2-1：三种都不许含糊）。
///
/// 为什么分三档而不是一句「失败了」：生产那条路上（presenter 逐家调 core 的 `java::install`）
/// 失败有三种真实形状，用户下一步完全不同——链上全跳过 ⇒ 换版本；某一家失败 ⇒ 看那家的原因
/// （网络、摘要校验）；连落点都没解析出来 ⇒ 修设置/权限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardInstallFailure {
    /// 有一家真试过但失败：`vendor` 是发行版 id（**不上屏**，界面按目录文案取名）
    Vendor {
        /// 发行版 id（`temurin` / `oracle` / …）
        vendor: String,
        /// core 的原文（**不翻译**：里面有 HTTP 状态码、路径这类取证材料）
        detail: String,
    },
    /// 链上全是「跳过」：**没有一家发行版提供**这个大版本（点名没有意义）
    NoVendor,
    /// 还没走到任何一家（安装位置解析不出来这类本地问题；链上是 `vendor` 为空的那一跳）
    Local {
        /// core/io 的原文（不翻译）
        detail: String,
    },
}

/// 一份 Java 页快照 + 目标大版本 ⇒ 这条出口现在该是哪一档（`None` = 还没结论）。
///
/// 纯函数、**不看 App**：产物直接可测（`app::dialog::tests::m334`）。
///
/// 三种失败形状都只读 `chain`（presenter 逐家调 core 之后写进去的那条），**不读 `message`**
/// ——`message` 是 presenter 的中文串，对话框一个字节都不许读它（英文界面会同屏两语）。
pub(super) fn classify_install(page: &JavaPageView, major: u32) -> Option<GuardInstall> {
    if page.busy {
        return Some(GuardInstall::Running);
    }
    if page.majors().contains(&major)
        || page
            .chain
            .iter()
            .any(|step| step.outcome == JavaChainOutcome::Hit)
    {
        return Some(GuardInstall::Done);
    }
    // 「还没走到任何一家」那一跳的 `vendor` 是空串（presenter 的 `install::LOCAL_STEP`）。
    if let Some(step) = page
        .chain
        .iter()
        .find(|step| step.vendor.is_empty() && step.outcome == JavaChainOutcome::Failed)
    {
        return Some(GuardInstall::Failed {
            failure: GuardInstallFailure::Local {
                detail: step.detail.clone(),
            },
        });
    }
    if let Some(step) = page
        .chain
        .iter()
        .find(|step| step.outcome == JavaChainOutcome::Failed)
    {
        return Some(GuardInstall::Failed {
            failure: GuardInstallFailure::Vendor {
                vendor: step.vendor.clone(),
                detail: step.detail.clone(),
            },
        });
    }
    if page
        .chain
        .iter()
        .all(|step| step.outcome == JavaChainOutcome::Pending)
    {
        // 「全是待尝试」是**还没结论**（`refresh_java` 在 `java_chain` 为空时会发这种计划链），
        // 不是「没有一家提供」——别把没结论说成失败（复审 r2 nit 1）。
        return None;
    }
    if !page.chain.is_empty() {
        // 有内容、没有 `Failed` 也没有 `Hit` ⇒ 全是「跳过」：没有一家提供这个版本。
        return Some(GuardInstall::Failed {
            failure: GuardInstallFailure::NoVendor,
        });
    }
    None
}
