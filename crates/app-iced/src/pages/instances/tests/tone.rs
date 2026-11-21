//! 状态行**角色表**（A6 的 `pages::instances::launch_tone`）的四档判据（M314 r1 的 p2-3）。
//!
//! 为什么单独立一条：`app/state/refresh/tests.rs` 的 App 级用例只走 `Idle → Running → Failed`
//! 三档，而 `Busy`（真正贯穿整个「启动中」窗口的那一档）**零覆盖**——复审 r1 实测这个空白。
//! 这里四条分支各钉一次，**不起帧泵、不建 App**（纯函数，判据只看角色）。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`every_runtime_phase_has_its_own_tone`] | 四档的角色被合并/互抄（`Busy` 退回正文色、`Failed` 不再是红句、`Idle` 凭空多出一行） |
//!
//! **变异实测**（M314 r1）：把 `Busy` 那支改成 `Some(Role::OnSurface)` ⇒ 红在本文件
//! 「启动中要弱一档」那一句。

use unlucky_design::color::Role;
use unlucky_presenter::dto::{InstanceRuntime, LaunchFailureCause, LaunchStage, Progress};

use super::super::launch_tone;

/// 四档各自的角色（`None` = 这一档不画状态行）。
///
/// 抓得住：①`Busy` 被写成正文色（启动中的「弱一档」消失，与运行中读起来一样）；
/// ②`Failed` 不再是 `Error`（红句没了——用户点名的「失败要红」就靠这一处）；
/// ③`Idle` 给了一个角色（空闲时凭空多出一行，而 `runtime_status` 对 `Idle` 返回 `None`，
/// 两处判据就对不上了）。
/// 抓不住：颜色在屏幕上的观感与对比度（那要出图）。
#[test]
fn every_runtime_phase_has_its_own_tone() {
    // 空闲：没有状态行（与 `app/view/instances.rs::runtime_status` 的 `Idle => None` 同一判据）。
    assert_eq!(launch_tone(&InstanceRuntime::Idle), None, "空闲不画状态行");

    // 启动中：比正文弱一档（它在动，但不是结论）。
    assert_eq!(
        launch_tone(&InstanceRuntime::Busy {
            stage: LaunchStage::Launch,
            progress: Progress::default(),
        }),
        Some(Role::OnSurfaceVariant),
        "启动中要弱一档（Busy 退回 OnSurface 时红在这里）"
    );

    // 运行中：稳定事实，正文色。
    assert_eq!(
        launch_tone(&InstanceRuntime::Running { pid: 4321 }),
        Some(Role::OnSurface),
        "运行中是正文色"
    );

    // 失败：**红句**（全仓失败色只有一个角色，与 mods / saves / 内容库同款）。
    assert_eq!(
        launch_tone(&InstanceRuntime::Failed {
            cause: LaunchFailureCause::Crashed { exit_code: Some(1) },
        }),
        Some(Role::Error),
        "失败必须是 Error（红句）"
    );

    // 反向锚：四档里只有失败用 Error，别把「启动中」也染红。
    assert_ne!(
        launch_tone(&InstanceRuntime::Busy {
            stage: LaunchStage::Download,
            progress: Progress::default(),
        }),
        Some(Role::Error)
    );
}
