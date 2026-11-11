//! # 页面层（P4）：布局系统 + 四页 + **路由/真组件的接入口**（M84 `PAGEFRAME` → M90 `APPSKEL`）
//!
//! ## 这一层交了什么
//!
//! | 阶段 | 交付 | 判据 |
//! |---|---|---|
//! | M84 | **布局系统**（档位/版面/列宽）+ 四页占位视觉 | 下面的版面阈值表 |
//! | **M90（本文件当前状态）** | ①视图模型 [`model`] ②导航项接上 `presenter::router::Route` ③**实例页整页接真组件 + 真数据** | 「接真组件后版面约束一条没破」（见 `pages/tests.rs` 的数值断言） |
//! | M99 `SETTINGSFRAME` | **纯搬移**：导航表搬到 [`nav`]，设置页拆成目录（`pages/settings/**`），i18n 目录按域拆开 | 截图逐字节相同 + `catalog` 金表（`key -> 译文` 全量不变） |
//!
//! **导航轨归属（M90 r3 订正，复审 P2-1）**：产品路径（`app/view.rs`）走
//! `shell::view_with_content`，导航轨由 **`shell/nav.rs` 的手搓实现**画——它与 M78 组件几何同值
//! （宽 88、药丸 56×32），复审逐像素量过「r1 产品图（真组件 rail）vs r2 产品图（shell rail）
//! 在 y≥48 区域差异为 0」；组件版的 `nav::rail` 现在只服务 `pages::app`（`pages_demo` 出口图、
//! harness）与单测。为什么不是「把 rail 挪进内容槽位」：那需要把 `shell` 的消息类型泛型化/映射，
//! 成本远大于收益（见偏离表 #10）。
//!
//! 实例页之外的三页**仍是 M84 的占位视觉**（下载/日志/设置），但它们已经：
//! 走同一个 `Message` → `update` → `Route` 通道，导航轨点得动、路由栈按 `presenter::router` 的语义走。
//! 页面美化是后续 mission 的事，本 mission 只保证「地基是通的」。
//!
//! ## 版面规矩（M84 立的三条，M90 一条没动）
//!
//! 1. **宽度档位**：官方响应式断点 **600 / 840**（[`layout::WidthClass`]）；
//! 2. **版面**：单列 / supporting-pane / list-detail，各自的成立条件与退化阈值写在 [`layout`]；
//! 3. **行宽上限**：正文列硬上限 **424px（逐行实测 67 字符 ≤72）**（[`layout::measure_cap_px`]），
//!    由 [`layout::content_column_width`] / [`layout::frame_metrics`] **无条件施加**，
//!    页面拿不到「更宽」的选项。
//!
//! ## 版面阈值表（`layout` 模块文档是全文，这里给速查）
//!
//! | 版面 | 用它的页面 | 成立条件 | 退化 |
//! |---|---|---|---|
//! | `SingleColumn` | 下载 / 日志（永远）；实例 / 设置（窄窗） | 总是 | — |
//! | `SupportingPane` | 设置 | `w ≥ 600` 且 `w ≥ 720`（240 + 24 + 320 + 2×24 + 88） | 单列（分组按组堆叠） |
//! | `ListDetail` | 实例 | `w ≥ 840`（官方 expanded）且 `w ≥ 816` | 单列（列表 ↔ 详情整页切换） |
//!
//! ## 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`layout`] | **布局系统**：档位 / 版面 / 列宽 / 阈值，纯函数 + 单测（无 iced 控件树） |
//! | [`model`] | **视图模型**：presenter 快照 → 页面要画的东西（页面层不认识 presenter 类型） |
//! | [`frame`] | 页头 / 内容列 / 两列骨架 / 空态 / 卡片 / 分隔线 |
//! | [`nav`] | 导航轨的 `NAV` 表 + 部件版 `rail()`（harness / 单测；产品走 `shell/nav.rs`） |
//! | [`message`] / [`screen`] | `pages::Message` / `Action` / `Screen`（M101 r2 从本文件原样搬出） |
//! | `instances` | 实例页（list-detail）——**M90 唯一打通到真组件 + 真数据真命令的页** |
//! | `settings` | 设置页（supporting-pane，占位视觉）：分组注册表 + 每个分组一个文件 |
//! | `download` / `logs` | 下载 / 日志页（单列，占位视觉） |
//! | [`harness`] | 截图 harness（`examples/pages_demo.rs` 的入口，**不是** `cli.rs` 的一部分） |
//!
//! ## 消息与路由（M90 新增）
//!
//! 页面**不 import `crate::app::Message`**（组件契约 §2 的同一条纪律）：它有自己的
//! [`Message`]，app 层只做一次 `.map`。路由跳转的载荷是 `presenter::router::Route` 本身，
//! 于是「页面里点一行」= 「压一层栈」，语义与 presenter 的 `Router` 天然一致，
//! 中间不再有一层「页面自定义的选中项」需要同步。
//!
//! ### 设置页只有一个投递口（M101 `SETTINGSMSG`）
//!
//! 设置页的四条填页线各自在自己的文件里定义消息与状态，本文件的 [`Message`] 只留一个
//! [`Message::Settings`]（详见 `pages/settings/mod.rs` 的「消息 / 状态」两节）；`Message` 与
//! `Screen` 本身也在 M101 r2 搬到 [`message`] / [`screen`]（`pages/mod.rs` 合并后 397 行，只剩
//! 3 行余量），这里的 `pub use` 保证了 `pages::Message` / `pages::Action` / `pages::Screen`
//! 三条路径一字未变。
//!
//! ## 偏离登记（D30 要求逐条写清；M84 六条 + M90 三条）
//!
//! | # | 偏离 | 为什么 | 什么时候收口 |
//! |---|---|---|---|
//! | 1 | 页头按 M3 口径**铺满页面宽度**（贴页边距），不与正文列对齐 | AppBar 本来就该满宽；1180 下页头右缘 **1156** 与详情列右缘 **896** 有 **260px 分歧** | 真 top bar 落地时按 M3 定语（页头右侧是操作区，不跟随列宽） |
//! | 2 | 空态标题 `HeadlineSmall 24` 比页标题 `TitleLarge 22` 大 | 12b §15.2 明确「空态是唯一允许 Headline 级的场景」，与 §16.4 的页标题档有 spec 张力 | 有意保留，不修 |
//! | 3 | 单列页（下载 / 日志）在 1180 下只用掉 **40.6%** 内容宽（424/1044） | 行宽上限的必然结果；空态居中不显空 | 版本浏览真正接数据后重新决议 `PaneKit` |
//! | 4 | 空态图标字形 **40px**（12b 15.1 是 `[估]` 32~48） | 令牌 `IconSize` 只有 24 一档，`frame::glyph` 按尺寸重算 stroke | 组件库的 `EmptyState`（M77）落地时 |
//! | 5 | 窄窗「Back to instances」没有前导箭头 | 占位文本按钮没有图标位 | 真 top bar / M78 的导航件落地时 |
//! | 6 | 实例页「未选中详情」空态**没有出图** | 演示数据固定选中第 1 条；空态代码路径在（`detail_empty`） | D30 状态矩阵补齐时补一张，不必重跑全套 |
//! | 7 | ~~标题栏仍是 48px 留白带~~ → **已于 M90 r2 修**：产品页现在走 `shell::view_with_content`，标题栏 / 拖动热区 / 两条缩放热区 / 三个 caption 键全在 | r1 的实质问题是**交互面缺失**（无边框窗口 + 产品页没有任何窗口装饰 ⇒ 拖不动、缩不了、点不到关闭），不是「少画了一条栏」 | ✅ 已收口（见下） |
//! | 8 | 实例页的「New instance / Import」仍是**没有后端的按钮**（M90） | presenter 没有「打开新建向导」的指令（`wizard` 只有状态机，`AppCommand` 里无对应项），已报 finding | 向导 mission 补 `AppCommand::OpenWizard` 之后 |
//! | 10 | **两套 rail 各测各的**（M90 r3）：产品走 `shell/nav.rs`，组件版只给 harness/单测 | r2 把产品页接进真外壳后，rail 自然落到 `shell/nav.rs`（像素等价，见上）。但两边的守门是分开的：`shell/nav.rs` 自带 indicator 56×32 的单测，`pages/tests.rs` 守的是组件 rail 的 88 ⇒ **将来哪边先漂没人拦**。统一要泛型化 shell 的消息类型，成本大于收益 | 谁做「rail 单一实现」的 mission（已在报告登记）；在那之前任何改 rail 几何的改动都要两处一起改并各自跑守门 |
//! | 9 | 实例页的正文段落只在**演示数据**里出现（M90） | 真快照里没有「正文描述」这种字段；M84 那段 361 字符的 `NOTE` 是给行宽测量当样本的，不能凭空造到真页面里 | i18n / 实例详情 mission 补真实描述字段 |
//!
//! ### 偏离 #7 的收口记录（M90 r2 / P1-1）
//!
//! r1 的产品页渲染的是 `pages::app`：一条**只有高度、没有任何控件**的标题栏留白带。而
//! `DragWindow` / `ResizeStart` / `Minimize` / `ToggleMaximize` / `Close` 五种消息的**构造点只有
//! `shell/mod.rs`**，窗口又是 `decorations: false` ⇒ 交互模式下拖不动、缩不了、点不到关闭。
//!
//! 修法：`shell::view_with_content(ui, nav, title, content, forced)` 把外壳（标题栏 + 三条热区 +
//! 导航轨）与内容区拆开，产品页把 [`page`] 的树塞进内容槽位；`shell::view`（M68 骨架卡）保留给
//! `Route::Agent` 回落 —— 删了会让私有模块里的 `view` / `shell::nav::rail` 变死代码，`-D warnings` 会红。
//!
//! `pages::app`（本文件）现在**只服务 harness**（`pages_demo` example 与单测）：它的标题栏仍是留白带，
//! 因为 `shell` 的回调类型是 `crate::app::Message`，套不进 `pages::Message` 的树里。
//!
//! ## 截图通道
//!
//! ```text
//! cargo run -p unlucky-app-iced --example pages_demo -- \
//!   --screen=instances --width=1180 --height=760 \
//!   --screenshot=out.png --screenshot-theme=light --software-renderer
//! ```
//!
//! 产品二进制的截图走 `--screenshot`（页面树由 app 层按路由装配），本 harness 只出**单页**图，
//! 用于页面级回归与宽度对照。

pub mod appearance;
pub mod frame;
pub mod harness;
pub mod layout;
pub mod model;
pub mod update;

pub mod account;
pub mod content;
mod download;
mod instances;
mod logs;
mod message;
mod nav;
mod screen;
// M101：设置页的两条漏斗（消息 / 状态）都从这个模块树里出——app 层要构造
// `settings::GroupMessage`、借用 `settings::Groups`，所以最小暴露面是 `pub(crate)`。
// **不是** `pub`：`private_interfaces` 按**类型自身的声明可见性**判（四个 `Message` 枚举与
// `GroupMessage` 声明成 `pub` 就够），把整个设置页模块树暴露成 crate 公开 API 没有回报。
pub(crate) mod settings;

#[cfg(test)]
mod tests;
pub use download::{cancelled_toast, Category, Download, InstallDraft, Loader};
pub(crate) use instances::demo_form as demo_launch_form;
pub use instances::{
    mods_failure_line, mods_from_state, saves_delete_prompt, saves_failure_line, saves_from_state,
    InstanceMessage, LaunchForm, Mods, Saves,
};
pub use layout::{FrameMetrics, PageLayout, PaneKit, WidthClass};
pub use logs::{Confidence, Finding, Level, Line, Logs, Outcome};
pub use message::{Action, Message, UpdateAction};
pub use model::{Detail, Instances, LaunchFacts, Meta, Model, Row};
pub use screen::Screen;
pub use settings::Language;
pub use update::{Banner, Update, UpdateStatus};

use iced::widget::{column, container, row, space};
use iced::{Element, Length};

use unlucky_design::elevation::MaterialLayer;
use unlucky_design::spacing::Space;

use crate::app::components::motion;
use crate::theme::Ui;

use frame::filled;

/// **单页内容**（不含外壳）：交给 `shell::view_with_content` 塞进真标题栏与导航轨之间。
///
/// 产品路径（`app/view.rs`）走 [`page_with_settings`]：外壳由 `shell` 画（拖动/缩放/三个
/// caption 键、rail 88），这里只画「页头 + 内容列」。列宽仍按**整个窗口宽**算
/// （`layout::frame_metrics` 内部会扣掉 rail 88 与页边距），所以外壳换了不影响版面判据。
///
/// 视图模型由**调用方持有**（`App` 在状态变化时重建一次 `Model`），这里只借用：
/// `view(&self)` 不能返回一棵借用局部变量的树。
pub fn page<'a>(
    ui: &'a Ui,
    model: &'a Model,
    form: &'a instances::LaunchForm,
) -> Element<'a, Message> {
    render(
        ui,
        model,
        settings::States {
            appearance: None,
            groups: settings::demo_groups(),
            // harness 路径没有 presenter 快照：空切片 ⇒ 各组画演示行（`Model::settings` 此时是空的）。
            settings: &model.settings,
        },
        Language::default(),
        form,
    )
}

/// 同 [`page`]，但带上**外观状态**（设置页的「外观」分组要读当前值与诊断）。
///
/// 为什么另开一个入口而不是给 `Model` 加字段：`pages/model.rs` 是页面与 presenter 之间的展示
/// 契约（`Model` 只装「要画什么」），外观是**运行时状态**（滑块请求值、已解码的背景图、诊断），
/// 塞进去会让演示数据（`pages::demo`）也得伪造一份外观状态。
///
/// 这条入口拿的是**演示组状态**（[`settings::demo_groups`]）：页面层没有真组状态可用，
/// 产品路径走 [`page_with_settings`]。
pub fn page_with_appearance<'a>(
    ui: &'a Ui,
    model: &'a Model,
    appearance: &'a appearance::State,
    form: &'a instances::LaunchForm,
) -> Element<'a, Message> {
    render(
        ui,
        model,
        settings::States {
            appearance: Some(appearance),
            groups: settings::demo_groups(),
            settings: &model.settings,
        },
        Language::default(),
        form,
    )
}

/// 产品路径：带上**设置页那份只读状态**（外观运行期状态 + 各组自己的状态）。
///
/// 为什么要单独一个入口：组状态的所有者是 `App`（`app/state.rs` 的 `settings` 字段），页面层
/// 只借用一帧。加/改某一组的字段不改变这条签名（`States` 里只有 `&Groups`），所以四条填页线
/// 都不必来这里动一行。
/// `language` 是 M106 加的：「当前档位 + 生效语言」只有 `App` 知道（`App::language()`），而设置页
/// 的 General 组要画它。唯一调用点是 `app/view.rs`；harness 那三条入口（[`page`] /
/// [`page_with_appearance`] / [`app_with_appearance`]）仍用 [`Language::default`]。
pub fn page_with_settings<'a>(
    ui: &'a Ui,
    model: &'a Model,
    states: settings::States<'a>,
    language: Language,
    launch_form: &'a instances::LaunchForm,
) -> Element<'a, Message> {
    render(ui, model, states, language, launch_form)
}

fn render<'a>(
    ui: &'a Ui,
    model: &'a Model,
    states: settings::States<'a>,
    language: Language,
    launch_form: &'a instances::LaunchForm,
) -> Element<'a, Message> {
    let metrics = layout::frame_metrics(model.screen.kit(), model.window_width);

    let page = match model.screen {
        Screen::Instances => instances::view(ui, metrics, model, launch_form),
        // M106：下载页此前写死成 `Lang::En`，现在跟 `model.lang`（页面文案跟着界面语言走）。
        Screen::Download => download::view_sized(
            ui,
            metrics,
            &model.download,
            model.lang,
            model.window_height,
        ),
        // M109：内容库页同样跟 `model.lang`
        // M161：内容库要按**窗口高度**选版面（矮窗口不能把列表压扁），
        // 正文可用高由 `frame::body_height` 从窗口高减掉外壳算出（唯一来源）。
        Screen::Content => {
            content::view(ui, metrics, &model.content, model.lang, model.window_height)
        }
        // M159：设置页「更新」组要画 presenter 的更新快照（不在 `States` 里，理由见该组文档）。
        Screen::Settings => settings::view(ui, metrics, states, language, &model.update),
        Screen::Logs => logs::view(ui, metrics, &model.logs, model.lang),
    };

    // M159：自更新提示条挂在**每一页**的页头之上（有新版 / 正在下载 / 已就位时才出现）。
    // 为什么在这里而不是外壳里：外壳（`shell`）只认窗口装饰与导航，页面层才能按快照决定画什么；
    // 而「每一页都要能看见」这条要求又决定了它不能只挂在实例页上。
    let tree: Element<'a, Message> = match update::banner(ui, &model.update, model.lang.catalog()) {
        Some(banner) => column![banner, page]
            .spacing(ui.space(Space::X3))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        None => page,
    };

    // M192：主体入场（首屏 / 换页 / 列表↔详情）。**这里只采样**——「标识变了就起跑」在 update 侧
    // （`App::sync_motion`）：iced 先算订阅表后建视图，在 view 里插轨道当拍订阅不上（复审 P1-1）。
    // 静息时位移恒为 `(0, 0)`，`shifted` 常驻的那一层零内距，与没有动效时逐像素相同。
    motion::shifted(tree, motion::sample())
}

/// 整窗（**harness 专用**）：标题栏留白带 + 导航轨 + 当前路由的页面。
///
/// 产品路径**不走这里**（它走 [`page`] + `shell::view_with_content`，用真标题栏）。这个入口留给
/// `pages_demo` example 与单测：它们要一张「只有页面、不接 app 消息」的独立图，而 `shell` 的
/// 回调类型是 `crate::app::Message`，套不进 `pages::Message` 的树里。
pub fn app<'a>(
    ui: &'a Ui,
    model: &'a Model,
    form: &'a instances::LaunchForm,
) -> Element<'a, Message> {
    app_with_appearance(ui, model, None, form)
}

/// 同 [`app`]，但可以带上**外观运行期状态**。
///
/// `Some(state)` ⇒ 设置页「外观」分组画**真控件**（`pages/appearance/section/`）；
/// `None` ⇒ 画演示行，与 [`app`] 逐像素等价。harness 的 `--appearance=real` 走的就是这一支：
/// M100 之前 harness 恒传 `None`，于是 D30 要的「带真实外观控件」那一屏**根本不存在**——
/// 报告上写着「设置页已接真控件」，图里却只有演示行（U1 的形状）。
pub fn app_with_appearance<'a>(
    ui: &'a Ui,
    model: &'a Model,
    appearance: Option<&'a appearance::State>,
    form: &'a instances::LaunchForm,
) -> Element<'a, Message> {
    // harness 路径没有真组状态：用演示状态（[`settings::demo_groups`]），外观状态照传。
    let body = render(
        ui,
        model,
        settings::States {
            appearance,
            groups: settings::demo_groups(),
            settings: &model.settings,
        },
        Language::default(),
        form,
    );

    let shell = row![nav::rail(ui, model.active), body]
        .width(Length::Fill)
        .height(Length::Fill);

    let titled = column![titlebar_placeholder(ui), shell]
        .width(Length::Fill)
        .height(Length::Fill);

    container(titled)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| filled(ui.layer(MaterialLayer::Canvas)))
        .into()
}

/// 演示模型（`pages_demo` example 与单测用；真页面走 `app/view.rs` 的 `AppState` 转换）。
///
/// 实例 / 下载 / 日志三页带演示数据，设置页仍是占位视觉。
///
/// 高度也是模型的一部分（M161）：内容库在矮窗口下要换版面，出图时 `--height` 必须
/// 真的传进来，否则「矮窗口那几张图」画的是高窗口的版面。
pub fn demo_model(screen: Screen, window_width: f32, window_height: f32) -> Model {
    let mut model = Model::new(screen, window_width, window_height, Some(screen.nav()));
    // 出图的中英取证（M222）：`ULL_DL_LANG=zh` 时整页走中文目录。
    //
    // 为什么需要这一行：页面文案来自 `model.lang`（`download::view(.., lang)` → `lang.catalog()`），
    // 而 `Model::new` 的 `lang` 写死 `En`、`demo_model` 之前也不碰它——所以 `LC_ALL=zh_CN.UTF-8`
    // 对出图**完全无效**（`i18n::detect_env` 不在这条链上，Windows 上平台源还会压过 env）。
    // 只有 harness / 单测走 `demo_model`，产品路径由 `app/view.rs` 从 `AppState` 建模型，零影响。
    let lang = match std::env::var("ULL_DL_LANG").ok().as_deref().map(str::trim) {
        Some("zh" | "zh_cn" | "zh-CN" | "zh_CN") => unlucky_i18n::Lang::ZhCn,
        _ => unlucky_i18n::Lang::En,
    };
    model.lang = lang;
    match screen {
        Screen::Instances => model.instances = instances::demo(),
        Screen::Download => model.download = download::demo(lang.catalog()),
        Screen::Content => model.content = content::demo(content::ContentKind::Mod, model.lang),
        Screen::Logs => model.logs = logs::demo(model.lang),
        // M159：设置页「更新」组的出图夹具（一个可装的新版本 + 一次成功检查）。
        // 只给这一页：其它页的基线图（尤其实例页的启动参数区）不该因为更新提示条而整体下移。
        Screen::Settings => model.update = update::demo(),
    }
    model
}

/// 标题栏留白带（高 = `LayoutMetrics::titlebar_height`）——**只给 [`app`] 的 harness 路径用**。
///
/// 产品路径的标题栏是 `shell::view_with_content` 画的那个真的（三个 caption 键 + 拖动热区），
/// 见模块文档偏离 #7（M90 r2 已修）。
fn titlebar_placeholder<'a>(ui: &'a Ui) -> Element<'a, Message> {
    container(space::horizontal())
        .width(Length::Fill)
        .height(ui.metrics().titlebar_height)
        .style(move |_theme| filled(ui.layer(MaterialLayer::Canvas)))
        .into()
}
