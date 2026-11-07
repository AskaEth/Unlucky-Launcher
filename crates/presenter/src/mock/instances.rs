//! 截图/演示用的**实例场景**（只在 mock 后端里被调用）。
//!
//! D30 要求实例页出状态矩阵：空态 / 有实例 / 运行中（带 pid）/ 启动中（带百分比）。
//! 截图通道强制 mock 后端（`AppOptions::mock`），所以这四态的**数据**必须由 mock 造：
//! 走 `ULL_INSTANCE_STATE` 选场景，与 `ULL_ACCOUNT_STATE`（M114）/ `ULL_CONTENT_STATE`（M116）
//! 同一套取证开关。
//!
//! ## 三个约束（塔台 M120 批的）
//!
//! 1. **不设变量 = 今天那 4 条**（Idle×3 + Failed×1）——[`crate::mock_data::mock_instances`]
//!    原样返回，产品/单测的行为一字未变；
//! 2. **这个开关只影响 mock 通道**：真 host 的实例来自 `core::game` 的快照，读不到这里；
//! 3. `running` 的 pid 是**写死的 mock 值**（`1234`），不是真进程 pid——它证明的是
//!    「`运行中 · pid {pid}` 这条文案的数字与分隔符不随语言变形」，不是「我能读到真 pid」。
//!
//! ```text
//! ULL_INSTANCE_STATE=empty ULL_START_ROUTE=instances ULL_LANG=zh_cn \
//!   unlucky-launcher-iced --screenshot=instances-empty-zh.png --software-renderer
//! ```

use crate::dto::{InstanceRuntime, InstanceSummary, LaunchStage, Progress};
use crate::mock_data::mock_instances;

/// 挑演示场景的环境变量。
pub(crate) const STATE_ENV: &str = "ULL_INSTANCE_STATE";

/// 场景名（未知值按 `demo` 处理，与 `ULL_ACCOUNT_STATE` 同一条兜底口径）。
pub(super) fn scenario() -> String {
    std::env::var(STATE_ENV)
        .unwrap_or_else(|_| "demo".to_string())
        .trim()
        .to_lowercase()
}

/// 场景对应的实例列表。
///
/// | 场景 | 列表 | 页面看起来 |
/// |---|---|---|
/// | `empty` | 空表 | 空列表 + 「未选择实例」空态（`detail: None`） |
/// | `demo`（缺省 / 未知值） | 4 条 Idle×3 + Failed×1 | 今天的样子（M90 起的基线） |
/// | `running` | 4 条，其中 1 条 `Running{pid}`、1 条 `Busy{42%}` | 列表 overline 与详情状态行都带运行态 |
pub(super) fn instances(scenario: &str) -> Vec<InstanceSummary> {
    match scenario {
        "empty" => Vec::new(),
        "running" => running(),
        _ => mock_instances(),
    }
}

/// 运行态矩阵：同一批实例换 runtime（名字/版本/加载器都还是 `mock_data` 那一份）。
///
/// 三种态各留一条，是为了**一屏看完**状态文案的三种形状：正在跑（`pid`）、启动中（百分比）、
/// 启动失败（原因）。少一条就得再出一张图，而图多一张就多一处可以撒谎的地方。
fn running() -> Vec<InstanceSummary> {
    let mut list = mock_instances();
    for instance in &mut list {
        match instance.id.as_str() {
            "survival" => instance.runtime = InstanceRuntime::Running { pid: 1234 },
            "vanilla-latest" => {
                instance.runtime = InstanceRuntime::Busy {
                    stage: LaunchStage::Download,
                    progress: Progress {
                        done: 630,
                        total: 1_500,
                    },
                };
            }
            _ => {}
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **默认行为不变**（塔台三条约束的第 1 条）：不设变量就是 `mock_data` 那一份。
    ///
    /// 抓得住：有人把 `demo` 分支改成了「顺手多造几条」——那会让所有既有截图与断言漂。
    #[test]
    fn the_demo_scenario_is_exactly_the_old_mock_list() {
        for unknown in ["demo", "", "bogus"] {
            let got = instances(unknown);
            let want = mock_instances();
            assert_eq!(got.len(), want.len(), "{unknown:?}");
            assert_eq!(got, want, "{unknown:?} 必须与 mock_data 逐字段相同");
        }
    }

    #[test]
    fn the_empty_scenario_has_no_instances() {
        assert!(instances("empty").is_empty());
    }

    /// 运行态矩阵：`Running` / `Busy` 各一条，且**名字与版本没被动手脚**（它们不是场景的一部分）。
    #[test]
    fn the_running_scenario_covers_running_and_busy() {
        let list = instances("running");
        assert_eq!(list.len(), mock_instances().len());
        assert!(
            list.iter().any(
                |item| matches!(item.runtime, InstanceRuntime::Running { pid } if pid == 1234)
            ),
            "要有一条带固定 pid 的运行中实例"
        );
        assert!(list.iter().any(|item| matches!(
            item.runtime,
            InstanceRuntime::Busy {
                stage: LaunchStage::Download,
                ..
            }
        )));
        assert_eq!(list[0].name, mock_instances()[0].name);
    }

    /// 环境变量名是**契约**（出图脚本与报告都写它，改名等于让文档说谎）。
    #[test]
    fn the_scenario_variable_is_named() {
        assert_eq!(STATE_ENV, "ULL_INSTANCE_STATE");
    }
}
