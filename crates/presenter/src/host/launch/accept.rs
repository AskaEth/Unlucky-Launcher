//! 一次启动的**受理**与停止的实例解析（M304：读已发布快照，不在调用方线程扫盘）。
//!
//! 从 `host/mod.rs` 拆出来只为两件事：守住 400 行硬线，以及让「点启动这条状态机」与
//! `launch` 模块的其余部分（装配 / 起进程 / 收尾）放在一起。判断顺序见 [`CoreHost::start_launch`]。
//!
//! 四个方法的可见性是 `pub(in crate::host)` 而不是 `pub(super)`：`host::launch_state_tests`
//! 是 `host` 的子模块、不是 `launch` 的子模块，它要直接调这几个入口（测真实状态流转）。
//!
//! ## M331：Java 不符就在这里**拦下**
//!
//! 判据全在 core（`JavaMismatch::check` + `select::decide`），本文件只做两件事：**在受理那一刻
//! 判一次**、**判下来是 Block 就什么都不起**。CLI 早就是这么用的
//! （`crates/cli/src/commands/launch.rs` 的 `select::decide(java.mismatch, explicit_java).block()`），
//! 而 GUI 此前只挑不判 —— 于是托管里只有 17/21、用户点开 1.12.2 **+ Forge**（LaunchWrapper
//! 只能用 8）时界面照常起，游戏必崩，用户只看到「游戏崩溃 退出码 1」。
//!
//! 拦下的位置刻意排在 [`CoreHost::mark_pending`] **之前**：被拦下不算「受理」——
//! 不标 pending、不发 `Busy`、不 spawn，于是磁贴仍是「空闲」，界面弹的是**启动前**的对话框
//! 而不是崩溃卡片（拦下 ≠ 启动失败，理由见 [`crate::dto::LaunchBlockedView`]）。
//!
//! ## M349：不拦的那一半也要开口
//!
//! M348 之后 `<1.13` 的**原版**不再有上限，于是「1.7.10 + Java 21」这类组合一路放行——
//! 命令行会给一句 `建议:`，GUI 此前**一个字都不说**（崩了才知道）。现在同一条判据
//! （core 的 `legacy_advisory_facts`）在这里也判一次：命中就发一条
//! `direction = Advisory(_)` 的启动前事件并**停下**，用户点「仍要启动」才真起进程
//! （[`CoreHost::start_launch_anyway`]，与拦下那条共用同一个出口）。
//!
//! 位置与拦下相同（`mark_pending` 之前）：先问一句也不是「受理」。

use unlucky_core::game::java::select;
use unlucky_core::game::java::{
    legacy_advisory_facts, JavaMismatch, LegacyAdvisory, LegacyAdvisoryTier,
};
use unlucky_core::launch::launcher_log;

use crate::dto::{
    AdvisoryEvidence, AdvisoryTier, CoreEvent, InstanceRuntime, JavaGuardDirection,
    LaunchBlockedView, LaunchStage, Progress,
};
use crate::toast::ToastKind;

use super::super::{game, launch, launch_args, CoreHost};

impl CoreHost {
    /// 某实例是否已在启动/运行（`pending` 与 `launches` 的并集）。
    ///
    /// 只查 `launches` 不够：`launch::start` 到后台线程回报 `LaunchStarted` 之间有窗口，
    /// 这期间重复点启动会起第二个进程、并把旧 pid 在启动表里顶掉（旧进程变孤儿）。
    pub(in crate::host) fn in_flight(&self, key: &str) -> bool {
        self.pending.contains(key) || self.launches.iter().any(|live| live.instance == key)
    }

    /// 标记「已受理、等后台线程回报」（`launch::start` 之后的窗口）。
    ///
    /// 单测也走这个入口而不是直接改 `pending`，保证测的是真实状态流转。
    pub(in crate::host) fn mark_pending(&mut self, id: &str) {
        self.pending.insert(id.to_string());
        self.failed.remove(id);
    }

    /// 启动受理（M304：解析读**已发布快照**，调用方线程一次盘都不扫）。
    ///
    /// 顺序不能换：① 在飞/重复点先挡（一次内存判，最便宜）② 解析 + 单实例复核（微秒级）
    /// ③ **Java 判一次**（M331；不符且没显式指定 ⇒ 到此为止，什么都不起）④ 标 `pending` +
    /// 发 `Busy` ⑤ 起后台线程。④ 在 ⑤ 之前 ⇒ 「按钮进 Busy」**不依赖**后台线程回来，
    /// 这正是「点下去没反应」的判据（`host::launch::target_tests`）。
    pub(in crate::host) fn start_launch(&mut self, id: &str) {
        self.accept_launch(id, false);
    }

    /// 用户看完「Java 不符」的对话框后仍然要启动（M331）。
    ///
    /// 等价于 CLI 的 `--java`：那是用户**显式**指定要试这个 Java，`decide` 一路 `Proceed`。
    /// 只放过这一次——这里**不写任何设置**（不把挑中的路径钉进实例 `javaPath`）：用户点的是
    /// 「就这一次」，不是「以后都这样」；下一次点启动照旧会被拦。
    pub(in crate::host) fn start_launch_anyway(&mut self, id: &str) {
        self.accept_launch(id, true);
    }

    /// 受理本体（`explicitly_chosen` = 用户明确指定了要用的 Java）。
    fn accept_launch(&mut self, id: &str, explicitly_chosen: bool) {
        if self.in_flight(id) {
            self.toast(ToastKind::Warn, "这个实例已经在启动/运行中");
            return;
        }
        let target =
            match launch::resolve(&self.env, &self.published, id, self.active_account.clone()) {
                Ok(target) => target,
                Err(launch::Rejection { reason, stale }) => {
                    // 拒绝要**具名**：用户点的是磁贴，磁贴与盘面对不上必须说出来是哪一条、为什么。
                    self.toast(ToastKind::Error, reason);
                    // 过期 ⇒ 立刻重扫（磁贴自己会消失 / 换成新版本）。「还在扫描中」不重扫：那会把
                    // 在飞那一轮的代次顶掉、结果被 `apply_instances` 丢掉，占位反而更久。
                    if stale {
                        self.refresh_instances();
                    }
                    return;
                }
            };
        let instance_id = target.id.clone();
        if self.in_flight(&instance_id) {
            self.toast(ToastKind::Warn, "这个实例已经在启动/运行中");
            return;
        }
        // M331 / D4：挑 Java 与判不符是**同一份**要求（实例自己那个根，M274 的口径），
        // 判据两行都在 core；这里只负责「被判 Block 就停在这里」。
        //
        // 「用户明确指定过 Java」有**两个**来源，两个都算（与 CLI 同一口径：两边调的都是
        // core 的 `select::explicit_java`，不是各写一份）：
        // ① 刚点了「仍要启动」（`explicitly_chosen`）；② 实例层钉过 `launch.javaPath`
        // （`explicit_java`，判据是 core 的 `select::explicit_java`）。② 不算的话，用户自己钉了
        // JDK 却每次点启动都被拦，而同配置的 CLI 直接放行——同一件事两条路径两种结果。
        //
        // M334 / P38：②**不包含全局默认** `java.selected_path`。早先这里读的是三级回落后的
        // 生效值（实例 → 全局），于是凡是在设置里「设为默认」过一次的用户，这道闸对所有实例
        // 都失效：点 1.12.2 + Forge 照常起、必崩，用户只看到「退出码 1」。全局默认照样参与**挑哪个
        // Java**（`preflight_java` 的生效值），只是不再给「显式指定」这个身份。
        let (java, explicit_java) = self.preflight_java(&instance_id, &target.version_id);
        if let Some(mismatch) =
            select::decide(java.mismatch, explicitly_chosen || explicit_java).block()
        {
            self.block_launch(&target, mismatch, &java);
            return;
        }
        // M349：「老版本原版 + 更高的 Java」不拦，但**也不静默放行**——先问一句。
        //
        // 判据与命令行同一个函数（core 的 `legacy_advisory_facts`）：要不要说、属于哪一档、
        // 五个数字全在那边算一次，这里只负责把它变成一个「启动前的框」。
        //
        // 唯一跳过它的时机是「用户已经点了仍要启动」（`explicitly_chosen`）——那是这一步的
        // 回答，再问一遍就成了死循环。**实例层钉过 javaPath（`explicit_java`）不跳过**：
        // 命令行的 `--java` 也照样打这句建议（`cli/commands/java_launch.rs` 不看 explicit），
        // 两边必须说同一句话，否则同一个配置在两条路径上两种待遇。
        //
        // 主版本用**探测到的**那一个（`probed_major`），不是给 adapter 的兜底值：探不到版本号
        // 时编一句「这次用的是 Java 17」就是在编事实（M349 r2 / 复审 P3-3），命令行同一口径
        // 是「不知道就不说」（`cli/commands/java_launch.rs` 的 `runtime.major?`）。
        if !explicitly_chosen {
            if let Some(advisory) = java
                .probed_major
                .and_then(|major| legacy_advisory_facts(&target.version_id, &java.required, major))
            {
                self.advise_launch(&target, &advisory, &java, explicit_java);
                return;
            }
        }
        self.mark_pending(&instance_id);
        if let Some(notice) = game::mode_change_notice(&self.env, &instance_id) {
            self.push_notice(notice);
        }
        self.send(CoreEvent::InstanceRuntimeChanged {
            id: instance_id.clone(),
            runtime: InstanceRuntime::Busy {
                stage: LaunchStage::Launch,
                progress: Progress { done: 0, total: 1 },
            },
        });
        launch::start(self.env.clone(), self.tx.clone(), target, java);
    }

    /// 挑 Java 并带回「与要求差在哪」，以及**用户是不是自己钉过 Java**。
    ///
    /// 「算不算用户显式指定」只有一个判据——core 的 [`select::explicit_java`]，只吃
    /// **实例层**（`launch_args::pinned_java`）与命令行；全局默认 `java.selected_path` 不算
    /// （M334 / P38：它一算，D4 那道闸对设过全局默认的所有实例失效）。
    ///
    /// 「用哪个 Java」是另一件事，优先级是：**实例层钉的** → **全局默认**（只在它满足要求时）
    /// → 自动挑。第二档要不要退回自动挑，判据同样在 core（[`select::fall_back_to_auto`]）——
    /// **与 CLI 共用同一个函数**（复审 r2 P2-2：退回逻辑写两遍，就是把这笔「同一功能两条路径」
    /// 从判据搬到挑 Java 上）。
    fn preflight_java(&self, instance_id: &str, version_id: &str) -> (launch::JavaPick, bool) {
        let effective = launch_args::resolved(&self.env, instance_id);
        let pinned = launch_args::pinned_java(&self.env, instance_id);
        let game = game::game_for(&self.env, instance_id);
        // GUI 这条路没有命令行，所以第二项恒 `None`（CLI 那一侧喂它自己的 `--java`）。
        let explicit = select::explicit_java(pinned.as_deref(), None);
        let chosen = pinned.as_deref().or(effective.java_path.as_deref());
        let preferred = launch::pick_java(&self.env, &game, version_id, chosen);
        // 「全局默认这一份不合适」有**两个**理由（M349 r2 / 复审 P2-1）：判下来不符（M334），
        // 或它会让「老版本原版 + 更高 Java」那条建议成立——后者看起来只是提示，实际是**死路**：
        // 用户照着建议装好 Java 8 之后，如果不退回自动挑，下一拍挑中的仍是这份全局默认
        // （`pick_java` 的显式分支只探测那一个路径），于是同一个框再弹一次、永远起不来。
        //
        // **`pinned` 不退回**：`fall_back_to_auto` 的第一条前提就是「实例层没钉」（core 的
        // `explicit_java`）。用户自己钉的 JDK 是他明确的选择，偷偷换成别的才是另一种不尊重；
        // 而钉过的那份与版本不符时本来就走硬拦（M331），不是这条建议。
        let unsuitable = preferred.mismatch.is_some()
            || preferred.probed_major.is_some_and(|major| {
                legacy_advisory_facts(version_id, &preferred.required, major).is_some()
            });
        let pick = if select::fall_back_to_auto(
            pinned.as_deref(),
            None,
            effective.java_path.as_deref(),
            unsuitable,
        ) {
            // 退回之后只用**满足要求**的那一份（挑不到就保留全局默认那份，拦下时那句
            // 「现在用的是 Java 17」才说得准）。
            let auto = launch::pick_java(&self.env, &game, version_id, None);
            if auto.mismatch.is_none() {
                auto
            } else {
                preferred
            }
        } else {
            preferred
        };
        (pick, explicit)
    }

    /// 只是**建议**（M349）：老版本原版 + 更高的 Java，不拦，但先问一句。
    ///
    /// 与 [`Self::block_launch`] 同一套做法（登记日志 + 发一条 `CoreEvent::LaunchBlocked`、
    /// 不标 `pending`、不发 `Busy`）：界面上它们是**同一个框**，差别全在 `direction`
    /// （`Advisory(_)` 那支换个标题与正文、按钮一样）。
    ///
    /// 为什么也走 `LaunchBlocked` 而不是新加一条事件：见 `dto::LaunchBlockedView` 的文档
    /// ——两条共用一个框，新事件会带来第二份 `open_*_dialog` 与第二份按钮分流。
    ///
    /// `pinned`（实例层钉过 `launch.javaPath`）决定**有没有装机出口**（M349 r3 / 复审 r2 P2-1）：
    /// 钉死那份的优先级最高，装到托管目录里的任何一份都不会被挑中，于是「下载 Java N」注定
    /// 被影子掉——画出来只会让用户点一圈回到同一个框。这一档改给「仍要启动」+「去实例启动参数」。
    fn advise_launch(
        &mut self,
        target: &launch::LaunchTarget,
        advisory: &LegacyAdvisory,
        java: &launch::JavaPick,
        pinned: bool,
    ) {
        launcher_log::set_dir(self.env.layout.logs());
        // 日志里也**不再拼那句话**（M349 r2）：文案只有目录里那一份，两壳按语言现求。
        // 这里记的是判据事实，事故之后看得出「为什么问、挑的是哪份」。
        launcher_log::record(
            "launch",
            &format!(
                "instance={} 建议（未拦）：{} 是按 Java {} 设计的，这次挑中 Java {}（{}）",
                target.id, advisory.version_id, advisory.minimum, advisory.actual, java.summary
            ),
        );
        self.send(CoreEvent::LaunchBlocked(Box::new(LaunchBlockedView {
            id: target.id.clone(),
            name: target.name.clone(),
            version_id: target.version_id.clone(),
            direction: JavaGuardDirection::Advisory(match advisory.tier {
                LegacyAdvisoryTier::Measured => AdvisoryTier::Measured,
                LegacyAdvisoryTier::Unmeasured => AdvisoryTier::Unmeasured,
            }),
            required_min: advisory.minimum,
            // 老版本原版**没有上限**（M348）——这条建议之所以存在就是因为上限不在这条线上。
            required_max: None,
            actual: advisory.actual,
            // 「下载 Java 8」要装的是该版本**按设计**用的那个（下限）；钉过实例 Java 时
            // 这个出口落地不了（见字段文档），给 `None` 让界面换成别的两枚。
            install_major: (!pinned).then_some(advisory.minimum),
            // 证据（哪一组实测过）：界面拿它填 `{measured_version}` / `{measured_java}`，
            // 值来自 core 的常量，不在目录里写死。
            evidence: Some(AdvisoryEvidence {
                version_id: advisory.measured_version.to_string(),
                java: advisory.measured_java,
            }),
        })));
    }

    /// 拦下一次启动：登记事实 + 发事件。
    ///
    /// **不动**运行时态（不 `mark_pending`、不发 `Busy`、不写 `failed`）：拦下不是失败，
    /// 磁贴该保持用户点之前的样子。启动器日志里照样记一条（事故之后要看得出「它为什么没起」）。
    fn block_launch(
        &mut self,
        target: &launch::LaunchTarget,
        mismatch: JavaMismatch,
        java: &launch::JavaPick,
    ) {
        launcher_log::set_dir(self.env.layout.logs());
        launcher_log::record(
            "launch",
            &format!(
                "instance={} 拦下：{}（挑中的是 {}）",
                target.id,
                mismatch.message(),
                java.summary
            ),
        );
        let direction = match mismatch {
            JavaMismatch::TooOld { .. } => JavaGuardDirection::TooOld,
            JavaMismatch::TooNew { .. } => JavaGuardDirection::TooNew,
        };
        self.send(CoreEvent::LaunchBlocked(Box::new(LaunchBlockedView {
            id: target.id.clone(),
            name: target.name.clone(),
            version_id: target.version_id.clone(),
            direction,
            required_min: java.required.minimum,
            required_max: java.required.maximum,
            actual: mismatch.actual(),
            // 「下载 Java {major}」要装的是**满足要求的那一个**：太老 ⇒ 抬到下限；
            // 太新（LaunchWrapper 系加载器只能用 8）⇒ 降到上限。都在 core 的 `JavaMismatch` 里，这里只取值。
            // 硬拦这一档一定可装：能走到这里说明用户没钉 Java（钉过 ⇒ `decide` 一路放行），
            // 全局默认那份影子不到托管目录。
            install_major: Some(match mismatch {
                JavaMismatch::TooOld { required, .. } => required,
                JavaMismatch::TooNew { maximum, .. } => maximum,
            }),
            // 硬拦没有「实测证据」这回事（它不是建议）。
            evidence: None,
        })));
    }

    /// 某实例在跑的 pid（按实例 id 查启动表；不再有「最近一次启动」这种单槽）。
    ///
    /// 兜底分支读**已发布快照**的 `Running { pid }`（M304）：host 自己发的运行时事件会经
    /// `handle::apply_instances` 写回快照，而快照就是这里读的那一份——「点停止」不再为此扫一次盘。
    pub(in crate::host) fn running_pid(&self, id: &str) -> Option<u32> {
        if let Some(live) = self.launches.iter().find(|live| live.instance == id) {
            return Some(live.pid);
        }
        self.published
            .iter()
            .find(|instance| instance.id == id)
            .and_then(|instance| match instance.runtime {
                InstanceRuntime::Running { pid } => Some(pid),
                _ => None,
            })
    }
}
