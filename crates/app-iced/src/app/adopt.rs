//! 「导入已有版本」向导（M223 GUIADOPT）：实例页页头的入口 + 一张盖在页面上的向导卡。
//!
//! ## 四段（与 presenter 的 `AdoptState` 一一对应）
//!
//! | 段 | 画什么 | 从哪来 | 文件 |
//! |---|---|---|---|
//! | Pick | 候选版本 + 目标档 + 实例名 | `AdoptEvent::Scan` | [`pick`] |
//! | Preview | 落点 / 隔离档 / **`target_notes()` 原文** | `AdoptEvent::Planned` | [`panes`] |
//! | Failed | 稳定码 + 人读原因 + **`problems[]` 逐条** | `AdoptEvent::Failed` | [`panes`] |
//! | Done | 实例名 / 运行目录 / 提示原文 | `AdoptEvent::Done` | [`panes`] |
//!
//! 「现在画哪一段」的判据只有一处：presenter 的 `AdoptState::stage`，外面套一条收窄
//! [`Form::stage`]——`Preview` 只在用户**点过预检**（`Form::checked`）之后才算数。收窄的理由：
//! 共享档 / 版本目录档一被点选就要显示后果原文（§5.2），那要用一次**零写盘**的预检换；
//! 但那次结果不该把用户直接甩到确认页（实例名还没填完）。
//!
//! 卡壳与页脚在 [`shell`]（定高可滚 / 收高两种几何），三段视图在 [`pick`] 与 [`panes`]，
//! 等宽与正文小工具在 `text`，D30 出图种子在 `shot`（`ULL_ADOPT_STATE`，只在 `--screenshot` 下生效）。
//!
//! ## 三条纪律（契约 §5 的 1/2/3）
//!
//! 1. **运行目录不派生**：Preview / Done 上那个路径**原样**来自回程事件（缺省档可能带 `-2` 后缀，
//!    只有 core 知道最后用了哪个）。本模块没有任何「拼一个目录名」的代码。
//! 2. **后果话术不重写**：共享档 / 版本目录档的后果句在 Preview 段**逐条**画 `plan.notes`
//!    （core `target_notes()` 的原文）。Pick 段只放一句短提示（选之前就得知道会碰别人的目录），
//!    真正的后果原文在确认按钮之前一定会出现。
//! 3. **不折叠问题**：Failed 段把 `problems` 一条一行画出来，标题只说「导入失败」，原因全在下面。
//!
//! ## 为什么是「盖一层」而不是实例页里的一段
//!
//! 向导有四段状态、每段内容都不一样，塞进 list-detail 的正文列会把版式判据（424 散文宽、
//! 列表独立滚动）搅乱。盖一层与 M187 的模态同一手法（遮罩 + 居中卡），但**不是**
//! `DialogQueue` 那种「标题 + 正文 + 两个按钮」的窄形态：向导要画候选列表、单选档位与输入框。
//!
//! ## 表单字段归 `App`，事件真相归 presenter
//!
//! [`Form`] 只装「用户正在填的东西」（选中哪个版本、展示名、目标档、有没有在等回程）；
//! 候选、预检、失败、完成全部来自 `AppState.adopt`（presenter 的 `AdoptState`）。两边各管一半，
//! 于是「画出来的落点」与「真正写盘的落点」永远是同一份事实。

mod panes;
mod pick;
mod shell;
/// D30 出图用的状态夹具（只在 `--screenshot` 下按环境变量生效）。
pub(crate) mod shot;
mod text;

#[cfg(test)]
mod tests;

use iced::widget::{container, mouse_area, opaque, Space};
use iced::{Alignment, Background, Element, Length};

use unlucky_design::spacing::Space as Sp;
use unlucky_i18n::{Catalog, Lang};
use unlucky_presenter::dto::{
    AdoptCandidateView, AdoptPlanView, AdoptStage, AdoptState, AdoptTargetView,
};

use crate::app::components::feedback::dialog::scrim;
use crate::theme::Ui;

use super::message::Message;

/// 向导卡宽度：比模态卡（560）宽一档——候选行要放「版本 id + 状态」，目标档是三枚 chip。
const CARD_WIDTH: f32 = 640.0;

/// 卡片的最大高（窗口更矮时整卡按窗口走：内容在中间区滚）。
const CARD_MAX_HEIGHT: f32 = 620.0;

/// 正在等哪一条命令的回程。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Busy {
    /// 空闲（可以点）
    #[default]
    Idle,
    /// 正在扫描 `versions/`
    Scanning,
    /// 正在预检（零写盘）
    Checking,
    /// 正在真正登记
    Importing,
}

impl Busy {
    /// 有没有在等回程。
    #[must_use]
    pub const fn waiting(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// 向导的表单态（**归 `App`**，页面借一帧）。
#[derive(Debug, Default)]
pub struct Form {
    /// 面板开着吗
    pub open: bool,
    /// 选中的版本 id（`None` = 还没选）
    pub selected: Option<String>,
    /// 展示名（空 = 用版本 id）
    pub name: String,
    /// 运行目录档（缺省 [`AdoptTargetView::InstanceDir`]，**不是** isolated）
    pub target: AdoptTargetView,
    /// 在等在飞的那一条命令
    pub busy: Busy,
    /// 用户已经点过「预检」（没点过就留在 Pick 段，哪怕后台已经顺手算过一次落点）。
    ///
    /// 为什么要有这一位：共享档 / 版本目录档一旦被点选，界面就要立刻显示 `target_notes()` 的
    /// **后果原文**（契约 §5.2）——那是 core 唯一会产出这段文字的地方，所以点选时顺手发一次
    /// **零写盘**的预检。但那一次结果不该把用户甩到「可以导入」的确认页：确认必须由显式的
    /// `Check` 触发。于是 `checked` 记住「这次预检是不是用户要的」。
    pub checked: bool,
}

impl Form {
    /// 打开面板（回到 Pick 段、清掉上一轮的选中与名字）。
    pub fn open(&mut self) {
        *self = Self {
            open: true,
            busy: Busy::Scanning,
            ..Self::default()
        };
    }

    /// 关掉面板（全清）。
    pub fn close(&mut self) {
        *self = Self::default();
    }

    /// 回程到了：不在等任何东西了。
    pub fn settled(&mut self) {
        self.busy = Busy::Idle;
    }

    /// 当前该画哪一段：`Preview` 只在用户**点过预检**之后才算数，其余按 `AdoptState::stage()`。
    ///
    /// 这条收窄是为了「点选共享档 ⇒ 立刻看到后果原文」而不把用户直接甩到确认页：
    /// 自动预检的结果落在 Pick 段里当**提示**读，用户点「预检」之后才进确认段。
    #[must_use]
    pub fn stage(&self, adopt_state: &AdoptState) -> AdoptStage {
        match adopt_state.stage() {
            AdoptStage::Preview if !self.checked => AdoptStage::Pick,
            other => other,
        }
    }

    /// 后台算出来、且**对得上当前选择**的预检（对不上 = 过期的结果，一律不画）。
    ///
    /// 两个判据缺一不可：
    /// - **目标档一致**：改了档之后旧 plan 的 `notes` 说的是别的目录；
    /// - **版本 id 一致**：换了版本之后旧 plan 的 `notes` 说的是别的版本目录
    ///   （版本目录档的后果原文里带 `versions/<id>`，拿它当新版本的后果就是撒谎）。
    ///
    /// 没有选中版本时也返回 `None`：那一刻没有任何版本可言，画谁的预检都不成立。
    /// 这两条都不是「选中即重扫」带来的——`Select` 只改本地字段、不发命令（落点见
    /// `app/update/adopt.rs`），所以过期判定只能在这里做。
    #[must_use]
    pub fn fresh_plan<'a>(&self, adopt_state: &'a AdoptState) -> Option<&'a AdoptPlanView> {
        let selected = self.selected.as_deref()?;
        adopt_state
            .plan
            .as_ref()
            .filter(|plan| plan.target == self.target && plan.version_id == selected)
    }

    /// 当前选中的候选（选中项已不在列表里时返回 `None`，界面自动退回「请选择」）。
    #[must_use]
    pub fn candidate<'a>(&self, adopt_state: &'a AdoptState) -> Option<&'a AdoptCandidateView> {
        let id = self.selected.as_deref()?;
        adopt_state
            .scan
            .as_ref()?
            .candidates
            .iter()
            .find(|candidate| candidate.version_id == id)
    }
}

/// 向导消息（`crate::app::Message::Adopt` 的载荷）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptMessage {
    /// 打开向导（实例页页头「导入」）
    Open,
    /// 关掉（点遮罩 / 关闭按钮）
    Close,
    /// 选中一个候选版本
    Select(String),
    /// 改展示名
    Name(String),
    /// 改运行目录档
    Target(AdoptTargetView),
    /// 预检（零写盘）
    Check,
    /// 确认登记（唯一写盘入口）
    Confirm,
    /// 从 Preview / Failed 退回候选列表（等价于重新扫一次）
    Back,
}

/// 向导消息 → app 消息。
fn adopt(message: AdoptMessage) -> Message {
    Message::Adopt(message)
}

// --------------------------------------------------------------- 视图

/// 把向导叠在页面之上（没打开时原样返回页面树）。
pub(super) fn layer<'a>(
    ui: &'a Ui,
    base: Element<'a, Message>,
    form: &'a Form,
    adopt_state: &'a AdoptState,
    lang: Lang,
) -> Element<'a, Message> {
    if !form.open {
        return base;
    }
    let catalog = lang.catalog();
    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(scrim(ui))),
                ..container::Style::default()
            }),
    )
    .on_press(adopt(AdoptMessage::Close));

    // 透明的一层只负责居中；吃点击的 `opaque` 只包卡片（点卡内不关，点遮罩才关）。
    let centered = container(opaque(shell::card(ui, form, adopt_state, catalog)))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(ui.space(Sp::X6));

    iced::widget::stack![base, backdrop, centered].into()
}

/// 中间区（按段交给 `pick` / `panes`）。
pub(super) fn middle<'a>(
    ui: &'a Ui,
    form: &'a Form,
    adopt_state: &'a AdoptState,
    stage: AdoptStage,
    catalog: Catalog,
) -> Element<'a, Message> {
    match stage {
        AdoptStage::Pick => pick::pick(
            ui,
            form,
            adopt_state.scan.as_ref(),
            form.fresh_plan(adopt_state),
            catalog,
        ),
        AdoptStage::Preview => panes::preview(ui, form.fresh_plan(adopt_state), catalog),
        AdoptStage::Failed => panes::failed(ui, adopt_state.failure.as_ref(), catalog),
        AdoptStage::Done => panes::done(ui, adopt_state.done.as_ref(), catalog),
    }
}
