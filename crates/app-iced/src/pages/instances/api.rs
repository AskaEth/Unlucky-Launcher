//! 实例页的**接线口**：文案 / 腔调漏斗、入口意图、强制视觉态映射。
//!
//! M416：从 `pages/instances.rs` 整块搬出（那边只剩几行余量），**只搬不改**——函数体逐字不动、
//! 可见性一致；对外路径由 `instances.rs` 的 `pub use` 原样保留，所以
//! `pages::saves_failure_line` / `pages::instances::forced_state` 这些路径逐字不变。
//!
//! 这一族都不画像素：它们只把「取哪句话」「用哪个颜色角色」「派哪个意图」「把控件钉到哪一态」
//! 各钉在一处，画的地方（`view` / `list_column` / `detail.rs` / `mods` / `saves`）照旧读它们。

use unlucky_design::color::Role;

use crate::app::components::buttons::Button;
use crate::app::components::interaction::Interaction;

use super::{mods, saves, Action, Message, Texts};

/// 存档区失败的一句话（app 层就地画红字用；取词入口仍是 `Texts`）。
///
/// 本版**不弹 Toast**：只读列表的失败离用户的眼睛很近（就在卡片里），再弹一条是刷屏。
pub fn saves_failure_line(
    failure: &unlucky_presenter::dto::SavesFailureView,
    catalog: unlucky_i18n::Catalog,
) -> String {
    saves::failure_line(failure, Texts::of(catalog))
}

/// 删除世界的确认框文案：`(标题, 正文, 主按钮, 次按钮)`。
///
/// 模态框在 app 层建（`state.dialogs.push`），但**取词入口仍是本页的 `Texts`**——与
/// [`saves_failure_line`] 同一条口径：页面文案只有一个入口，测试扫的也是那一份源码。
pub fn saves_delete_prompt(
    catalog: unlucky_i18n::Catalog,
    dir: &str,
) -> (String, String, String, String) {
    let texts = Texts::of(catalog);
    (
        texts.saves_delete_title().to_string(),
        texts.saves_delete_body(dir),
        texts.saves_delete_confirm().to_string(),
        texts.saves_cancel().to_string(),
    )
}

/// 启停失败/读目录失败的一句话（app 层弹 Toast 用；取词入口仍是 `Texts`）。
pub fn mods_failure_line(
    failure: &unlucky_presenter::dto::ModFailureView,
    catalog: unlucky_i18n::Catalog,
) -> String {
    mods::failure_line(failure, Texts::of(catalog))
}

/// 状态行的**腔调**（M314 / A6）：这一档的运行状态用哪个颜色角色。`None` = 这一档不画状态行。
///
/// 为什么在页面层：颜色是**视觉**决定，页面与它的测试都读这一处；update 侧
/// （`app/state/refresh.rs::sync_launch_tone`）只把它报给帧泵，页面
/// （`instances/detail.rs`）只按泵给的进度把两档合成——三处读的是同一张表。
///
/// 三档的角色口径：
///
/// | 档 | 角色 | 为什么 |
/// |---|---|---|
/// | `Busy`（启动中） | `OnSurfaceVariant` | 进行中的读数比正文弱一档：它在动，但不是结论 |
/// | `Running`（起来了） | `OnSurface` | 稳定事实，正文色 |
/// | `Failed`（失败） | `Error` | **红句**。全仓的失败色只有这一个角色（mods / saves / 内容库同款） |
///
/// `Idle` 给 `None`：那一档**没有**状态行（与 `app/view/instances.rs::runtime_status` 同一判据，
/// 它只对 `Idle` 返回 `None`）。出现 / 消失是版面变化，不走交叉淡入（见 `pump/launch.rs`）。
pub fn launch_tone(runtime: &unlucky_presenter::dto::InstanceRuntime) -> Option<Role> {
    use unlucky_presenter::dto::InstanceRuntime;
    match runtime {
        InstanceRuntime::Idle => None,
        InstanceRuntime::Busy { .. } => Some(Role::OnSurfaceVariant),
        InstanceRuntime::Running { .. } => Some(Role::OnSurface),
        InstanceRuntime::Failed { .. } => Some(Role::Error),
    }
}

/// 自动发现的实例按下「启动」时那句提示（**这一次**用的是哪一边的运行目录）。
///
/// 取值口径只有一处：`Texts::discovered_run`，判据是快照里的 `isolation`
/// （`shared` = 共用游戏根；其余 = 它自己的版本目录）。界面层不自己判盘。
pub fn discovered_run_toast(catalog: unlucky_i18n::Catalog, isolation: &str) -> String {
    Texts::of(catalog).discovered_run(isolation).to_string()
}

/// 「导入已有版本」向导的入口意图——**页面里唯一产地**。
///
/// 两个入口共用它：页头的「导入」与空态的主行动（一个实例都没有时用户只看得到后一个）。
/// 落点是 `app/update/page_messages.rs` 的 `Action::Import` 臂（→ `AdoptMessage::Open`），
/// 所以中间任何一段被拆掉，向导就再也打不开——`instances/tests/empty_entry.rs` 专门钉这条链。
pub(crate) fn adopt_action() -> Message {
    Message::Action(Action::Import)
}

/// `--force-state` 的落点：只改**画出来的**状态，不动任何状态机（cli `ForcedState` 的同一条纪律）。
pub(in crate::pages) fn forced_state<'a, M>(
    button: Button<'a, M>,
    forced: Option<Interaction>,
) -> Button<'a, M> {
    match forced {
        Some(state) => button.state(state),
        None => button,
    }
}
