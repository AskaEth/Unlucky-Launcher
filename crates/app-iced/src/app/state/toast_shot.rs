//! 出图取证用的 Toast 夹具（`ULL_TOAST_FIXTURE`；产品路径零影响）。
//!
//! # 它解决什么
//!
//! M298 的交付图要证明「提示真的画在窗口底部」，而产品路径的 toast 由事件推出来
//! （安装取消 / 启动收尾 / 复制成功），截图通道在第 3 帧就出图：等不到那条事件。
//! 处理方式与 `mods_shot` / `saves_shot` 一致——**状态注入**，注在「快照 → 页面模型」之前，
//! 于是覆盖层、胶囊几何、i18n 文案走的都是产品代码。
//!
//! # 边界（不要把它读成产品能力）
//!
//! - 只在环境变量存在时生效（`ULL_TOAST_FIXTURE` 非空且不为 `0`）；
//! - 推的是**一条真实形状的提示**：文案由产品函数 `launch_toast` 按**生效语言**现求，
//!   不是夹具里写死的一句中文；
//! - 剩余寿命走 `ToastCenter` 的默认 TTL（4 s），过期仍由 `app/update/clock.rs` 的 `expire` 管。

use unlucky_presenter::dto::{LaunchFinished, LogOutcome};
use unlucky_presenter::toast::ToastKind;

use super::App;

/// 夹具开关是否打开（`ULL_TOAST_FIXTURE` 非空且不为 `0`）。
fn enabled() -> bool {
    std::env::var("ULL_TOAST_FIXTURE")
        .map(|value| {
            let value = value.trim();
            !value.is_empty() && value != "0"
        })
        .unwrap_or(false)
}

/// 推一条「游戏崩溃」的收尾提示（就是用户在真机上看到的那一句）。
pub(super) fn seed(app: &mut App) {
    if !enabled() {
        return;
    }
    // 形状取自真实收尾事件：崩溃 + 退出码 1 + 跑了 12 秒。
    let finished = LaunchFinished {
        id: "survival".to_string(),
        session: String::new(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 12_000,
        failure: None,
    };
    let text = crate::app::view::instances::launch_toast(&finished, app.lang());
    app.state.toasts.push(ToastKind::Error, text);
}
