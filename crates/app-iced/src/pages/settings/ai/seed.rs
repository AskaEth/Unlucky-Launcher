//! 「助手」分组的**出图种子**（`ULL_AI_STATE`）：M174 的三档 + M390 的 `-yolo` 后缀。
//!
//! 从 `state.rs` 纯搬移出来的唯一理由是 400 行硬线（`guards.yml` 的 file-size 门禁）：
//! M390 往 `State` 里加了 `ai.yolo` 之后那边到 408 行。判据与行为一字未改——`State::screenshot_seed`
//! 仍是同一个函数、同一条入口（`app/state/construct.rs` → `hydrate_ai_settings`）。

use super::outcome::{ProbeKind, ProbeResult};
use super::state::{Probe, State};

impl State {
    /// 取证开关 `ULL_AI_STATE`（只给 `--screenshot` 用，见 `app/state/construct.rs`）。
    ///
    /// 为什么要它：D30 的状态矩阵要出「未配置 / 已配置并测过 / 测试失败」三张图，而真实路径要
    /// 点按钮、要联网。语义与 `ULL_UPDATE_STATE` / `ULL_ACCOUNT_STATE` 一致：只影响这一次进程。
    ///
    /// M390：任一种子可加 `-yolo` 后缀（`configured-yolo` / `verified-yolo` / `unconfigured-yolo`），
    /// 出「YOLO 开着」的那张图——开关两侧的文案不一样（开着才画红字告警），不然后者永远没人看过。
    #[must_use]
    pub fn screenshot_seed(raw: &str) -> Option<Self> {
        let mut state = Self {
            storage: true,
            ..Self::default()
        };
        let raw = raw.trim();
        let (raw, yolo) = match raw.strip_suffix("-yolo") {
            Some(rest) => (rest, true),
            None => (raw, false),
        };
        state.yolo = yolo;
        match raw {
            "unconfigured" => {}
            "configured" => {
                state.base_url = "https://api.deepseek.com/v1".to_string();
                state.model = "deepseek-chat".to_string();
                state.key_chars = 32;
            }
            other => {
                let kind = match other {
                    "verified" => ProbeKind::Passed,
                    "failed-auth" => ProbeKind::Auth,
                    "failed-model" => ProbeKind::ModelNotFound,
                    "failed-unreachable" => ProbeKind::Unreachable,
                    "failed-timeout" => ProbeKind::Timeout,
                    _ => return None,
                };
                state.base_url = "https://api.deepseek.com/v1".to_string();
                state.model = "deepseek-chat".to_string();
                state.key_chars = 32;
                // r1 复审 P2-1：种子的**形状**必须由产品路径（`ProbeResult::from_outcome`）决定，
                // 这里只伪造「结论是哪一档」。手填 `ProbeResult` 的字段会让「失败态目标为空」
                // 这类缺陷只在产品上出现、截图永远看不见。
                state.probe = Probe::Done(ProbeResult::screenshot_seed(
                    kind,
                    state.resolved_base_url(),
                    state.model.clone(),
                    412,
                ));
            }
        }
        Some(state)
    }
}
