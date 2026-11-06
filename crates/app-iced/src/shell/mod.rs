//! 首屏骨架（**不做业务**）：自绘标题栏 + 左侧导航轨 + 内容区 + **右侧 AI 助手栏**（M104）。
//!
//! ## 与参考判据的对应（`research/12b-component-fidelity.md`）
//!
//! | 部件 | 参考 | 令牌落点 |
//! |---|---|---|
//! | 标题栏高度 | 16.2 桌面标题栏 **40**（登记偏离） | `LM.titlebar_height = 48`（M63 §1.4/R13 桌面档，令牌优先） |
//! | 标题栏按钮 | 16.5 官方图标 **24** → 本项目 **20**（M86 项目偏离，用户点名「图标稍大」） | `icons::CAPTION_PX`（令牌 `TITLEBAR_CAPTION_ICON_PX`）+ `LM.titlebar_button_width = 46` |
//! | 页头 | 16.1 官方 AppBar **64**；本项目 **56** | `LM.page_header_height = 56`（已有偏离登记） |
//! | 卡片 | 3.x Outlined：**Medium(12)**、L0、`OutlineVariant` 1px、`SurfaceContainerLow` | `Component::OutlinedCard.spec()` + `STROKE_PX` |
//! | 字级 | 16.4 标题 `TitleLarge`；卡片 `TitleMedium` / `BodySmall` | `Ui::style` / `Ui::emphasized` |
//!
//! 导航轨在 [`nav`] 子模块里（12b §5 的判据表也在那边）。
//!
//! 一屏只出现 4 档字号（22 / 16 / 14 / 12），且**没有实心 accent**（rubric K2：一屏最多一个实心
//! Primary，骨架期一个都不要）。阴影只给浮层，骨架里没有浮层，所以**全屏零阴影**（rubric K3）。

mod nav;

/// AI 助手栏与 AI 独立页（M104）：**物理目录是 `crates/app-iced/src/pages/agent/`**，
/// 用 `#[path]` 挂到外壳的命名空间。
///
/// 为什么是 `#[path]` 而不是在 `pages/mod.rs` 加一行 `mod agent;`：`pages/mod.rs` 是四条页面线
/// 共用的声明文件（M104 的 scope 没给它），而右栏是**外壳的一部分**（它挂在任何页面右边）。
/// 先例是 `app/mod.rs` 挂 `../components/mod.rs`。物理路径只有一份，逻辑名字有两个用途：
/// 宿主（右栏）归外壳，视图（会话组件）归页面目录。
#[path = "../pages/agent/mod.rs"]
pub(crate) mod agent;

// M114：轨底账户头像位的状态也归外壳（`NavItem` + `RailAccount` 一起导出）。
pub use nav::{NavItem, RailAccount};

use iced::widget::text::LineHeight;
use iced::widget::{button, column, container, mouse_area, row, space, stack, text};
use iced::{mouse, Alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow};
use iced::{window::Direction, Theme as IcedTheme};

use unlucky_design::color::{Role, StateLayer};
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::Lang;

use crate::app::Message;
use crate::cli::ForcedState;
use crate::icons;
use crate::theme::Ui;

/// 导航轨的一帧状态（M114）：主序列高亮项 + 轨底账户头像位。
///
/// 绑成一个结构体是为了让 [`view_with_account`] 的参数停在 8 个以内（clippy `too_many_arguments`），
/// 也把「这两个值永远一起传」这件事写进类型：谁加导航项都绕不开轨底那一格。
pub struct NavRail {
    /// 高亮哪个主序列项（账户页是 `NavItem::Account`）
    pub selected: NavItem,
    /// 轨底头像位的状态（**按值持有**：它是每帧现算的，引用会让整棵树的生存期挂到临时值上）
    pub account: nav::RailAccount,
}

/// 整棵树（**内容槽位版**）：外壳画标题栏 / 缩放热区 / 导航轨 / 右栏，中间那块交给调用方。
///
/// 产品页的唯一入口（M90 r2 / P1-1；M104 起多两样东西：右侧助手栏与拖拽取指针层；
/// M114 起导航轨还会画轨底的账户头像位）。
///
/// 分工写死：
///
/// - 本模块负责：标题栏（拖动面 = **整条 48px 栏里从窗口左缘到第一个 caption** 的 `mouse_area`，
///   没有单独的 8px 热区；三个 caption 键各 46×48）、东/南两条缩放热区（各 `Space::X1` = 4px）、
///   导航轨、右栏、根容器底色；
/// - 调用方负责：`content`（页面内容区）。它拿到的可用宽度是 `窗口宽 − rail 88 − 右栏`，
///   而页面自己的 `layout::frame_metrics` 仍以**中栏可用宽**为输入（M104 的 `content_width`）。
///
/// 面板 α（D36 / M93）不从这里传：它是**表面色**的属性，唯一施加点是 `Ui::layer`
/// （`crate::theme`）。外壳的根容器 / 标题栏 / 导航轨都走 `ui.layer(..)`，因此自动同源。
///
/// 产品页入口：**内容槽位 + 背景层 + 右栏 + 拖拽层 + 轨底账户头像位**。
///
/// `background` 由 `pages::appearance::layer` 产出（已与 `Message::Page` 映射过）；垫在 `stack!`
/// 最底层，于是「面板 α < 1」时能真的看见它——这就是 D34 在像素上的落点。
// 9 个形参：M104 的 `right`/`overlay`、M106 的 `lang`（导航轨标签按语言取词），
// 加上 M114 把「高亮项 + 轨底账户态」捆成的 `NavRail`（捆它是为了少一个形参）。
// 再拆就得动 `app/view.rs` / `account_page.rs` 的调用点，而那正是 M104/M105/M109 反复设锚点的
// 地方——先就地 `allow`（主线原本也在这条上 `allow`），等下次合流再结构体化。
#[allow(clippy::too_many_arguments)]
pub fn view_with_account<'a>(
    ui: &'a Ui,
    nav_state: NavRail,
    app_title: &'a str,
    background: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
    right: Option<Element<'a, Message>>,
    overlay: Option<Element<'a, Message>>,
    forced: Option<ForcedState>,
    lang: Lang,
) -> Element<'a, Message> {
    // 右栏（M104）是 `row` 的**第三个兄弟**而不是叠层：它是布局宽度的一部分，压在上面会让
    // 中栏以为自己还有那么宽（那就是任务里禁止的「溢出」）。它的宽度由 `agent::docked` 自己声明。
    // M106：导航轨标签按当前界面语言取词（`lang` 一路从 `App::lang()` 递进来）。
    // M114：`nav_state` 只影响轨底那一格，`right`/`overlay` 的语义一字未动。
    let mut work = row![
        nav::rail(ui, nav_state.selected, &nav_state.account, forced, lang),
        content
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    if let Some(right) = right {
        work = work.push(right);
    }

    let work_area = column![titlebar(ui, app_title, forced), work]
        .width(Length::Fill)
        .height(Length::Fill);

    window_frame(ui, background, work_area.into(), overlay)
}

/// 无边框窗口的「外壳壳子」：背景层 + 工作区 + 两条缩放热区 + 根容器底色。
///
/// ## 圆角生效时**不**内缩热区（M116）
///
/// Win11 / Server 2025 的 DWM 圆角是**合成层**的裁切：它只改「画出来的像素」，**不改窗口矩形**，
/// 也不改命中测试（`WM_NCHITTEST` / 我们的 `mouse_area` 走的仍是整窗矩形）。所以：
///
/// - 标题栏拖动面、三个 caption 键（46×48）、东/南两条 4px 缩放热区**都不动**；
/// - 角上那一小块（圆角弧以外的像素）在屏幕上看不见，但仍会响应——落在南侧热区里就是缩放、
///   落在标题栏里就是拖动。两者都是无害的常规操作，比「内缩后角上出现 4px 死区」更好；
/// - 反过来**不能**把根容器改成圆角容器来「配合」外框：那只会在四角露出底色方块
///   （背景图/画布被裁成圆弧而外框仍然方），是自绘圆角的经典失败形态。
///
/// ⚠️ 上一条是本机**读不到**的部分：RDP 断连导致抓屏失效（见 `DevDocs/artifacts/m116/README.md`），
/// 「角上那块到底响不响应」没有任何像素证据，只有「DWM 不改窗口矩形」这条 API 事实。
/// 真机验收时请按 M116 报告里的清单补一条「在圆角窗口上拖边缩放仍命中」的手测。
fn window_frame<'a>(
    ui: &'a Ui,
    background: Option<Element<'a, Message>>,
    work_area: Element<'a, Message>,
    overlay: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    // 无边框窗口没有系统缩放边框 → 自己给两条热区（东/南）。
    // **必须叠层而不是兄弟列**：当兄弟列会吃掉 4px 布局宽（右留白变 28、caption 按钮离窗角 4px，
    // 视觉复审 P3-1）。叠层容器自己铺满，再用 align 把 4px 热区贴到边上，布局宽保持 1180。
    // 东侧热区从标题栏下方开始，避免吃掉三个 caption 按钮最右 4px 的点击区。
    let thickness = ui.space(Space::X1);

    let east = container(
        column![
            space::vertical().height(ui.metrics().titlebar_height),
            resize_edge(
                Direction::East,
                (Length::Fixed(thickness), Length::Fill),
                mouse::Interaction::ResizingHorizontally,
            ),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::End);

    let south = container(resize_edge(
        Direction::South,
        (Length::Fill, Length::Fixed(thickness)),
        mouse::Interaction::ResizingVertically,
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::End);

    // 背景在最底层（M93 / D34）。有背景层时**根容器不再盖实色画布**：否则那张不透明的
    // `Canvas` 会把背景图整块挡住（r2 实测：panel α=1 时背景完全看不见）。
    // 各面板（标题栏 / 导航轨 / 卡片 / 页头带）自己画底色，空隙里露出的就是背景图。
    // 拖拽层（M104）压在最上面：它只在「按下右栗分隔条」期间存在，作用是把指针 x 变成拖拽消息
    // （`mouse_area::on_move` 只在自己的边界内回报坐标，所以这一层必须铺满整窗）。
    let has_background = background.is_some();
    let shell = match (background, overlay) {
        (Some(layer), Some(overlay)) => stack![layer, work_area, east, south, overlay],
        (Some(layer), None) => stack![layer, work_area, east, south],
        (None, Some(overlay)) => stack![work_area, east, south, overlay],
        (None, None) => stack![work_area, east, south],
    };

    container(shell)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| {
            if has_background {
                container::Style::default()
            } else {
                filled(ui.layer(MaterialLayer::Canvas))
            }
        })
        .into()
}

/// 窗口缩放热区（透明；玻璃/底色由根容器提供）。
///
/// 拖动时调 `window::drag_resize(id, direction)`——Windows/X11 走 winit 原生实现，
/// **macOS 返回 `NotSupported`**（M60 Q4），如实登记在交接文档里。
fn resize_edge<'a>(
    direction: Direction,
    size: (Length, Length),
    interaction: mouse::Interaction,
) -> Element<'a, Message> {
    let (width, height) = size;

    mouse_area(container(space::horizontal()).width(width).height(height))
        .interaction(interaction)
        .on_press(Message::ResizeStart(direction))
        .into()
}

// ---------------------------------------------------------------- 标题栏

fn titlebar<'a>(ui: &'a Ui, title: &'a str, forced: Option<ForcedState>) -> Element<'a, Message> {
    let metrics = ui.metrics();

    // 拖动热区：从窗口左缘到第一个按钮（按钮自己不能被拖区吞掉，所以只包左侧一段）
    let drag_area = mouse_area(
        container(label(
            ui,
            TypeRole::LabelLarge,
            true,
            title,
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding::from([0.0, ui.space(Space::X4)])),
    )
    .on_press(Message::DragWindow);

    let bar = row![
        drag_area,
        caption(ui, IconKind::Minimize, Message::Minimize, forced),
        caption(ui, IconKind::Maximize, Message::ToggleMaximize, forced),
        caption(ui, IconKind::Close, Message::Close, forced),
    ]
    .width(Length::Fill)
    .height(metrics.titlebar_height)
    .align_y(Alignment::Center);

    container(bar)
        .width(Length::Fill)
        .height(metrics.titlebar_height)
        .style(move |_theme| filled(ui.layer(MaterialLayer::Canvas)))
        .into()
}

/// 标题栏按钮（自绘最小化/最大化/关闭）。
///
/// 图标必须**再包一层固定尺寸的容器**：iced 的 `button` 在 layout 里给内容 Fill 约束，
/// 直接把 `svg(...).width(24)` 塞进 button，usvg 会按 Contain 撑满内区（实测渲染盒 46 → 视觉线宽
/// 从 1.5 变 2.9，视觉复审 P2-2）。Container 尊重子元素的 Length，所以这层是「锁尺寸」用的。
///
/// **图标尺寸 = [`icons::CAPTION_PX`]（20，项目偏离；官方 24），容器 46×48 不动**（M86）：
/// 用户原话是「保持其容器尺寸不变的前提下把图标改小些」，所以缩放只落在图标盒与 SVG 位图上，
/// `metrics.titlebar_button_width` / `titlebar_height` 一个都不许跟着改。
fn caption<'a>(
    ui: &'a Ui,
    kind: IconKind,
    message: Message,
    forced: Option<ForcedState>,
) -> Element<'a, Message> {
    let metrics = ui.metrics();
    // 用 `center(Length::Fill)` 而不是只设 `width(20).height(20)`：后者会让容器缩到 20×20 并贴到
    // button 内容的左上角（button 不居中「收缩后」的内容，r1 实测图标跑到 cell 左上 (left+11.5, 11.5)）；
    // `center(Fill)` = 撑满按钮内容区 + **两轴**居中，字形仍是 20 盒、命中区仍是 46×48（Q21：
    // 只设 `align_y` 不设 `align_x` 会让图标贴左）。
    let icon = container(icons::view_at::<Message>(
        kind,
        ui.color(Role::OnSurfaceVariant),
        icons::CAPTION_PX,
    ))
    .width(icons::CAPTION_PX)
    .height(icons::CAPTION_PX)
    .center(Length::Fill);

    button(icon)
        .width(metrics.titlebar_button_width)
        .height(metrics.titlebar_height)
        .padding(0)
        .style(move |_theme: &IcedTheme, status| {
            let background = match overridden(forced, status) {
                button::Status::Hovered => Some(Background::Color(ui.state_over(
                    StateLayer::Hover,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                button::Status::Pressed => Some(Background::Color(ui.state_over(
                    StateLayer::Pressed,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                button::Status::Active | button::Status::Disabled => None,
            };

            button::Style {
                background,
                text_color: ui.color(Role::OnSurfaceVariant),
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .on_press(message)
        .into()
}

// ---------------------------------------------------------------- 小工具

/// 一个文字元素：字号/行高/字重全部来自令牌排版档。
pub(super) fn label<'a>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: &'a str,
    color: Color,
) -> Element<'a, Message> {
    let style = if emphasized {
        ui.emphasized(role)
    } else {
        ui.style(role)
    };

    text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(ui.font_of(style))
        .color(color)
        .into()
}

/// 取证用的状态覆盖（`--force-state`）：只改画出来的状态，不动任何状态机。
pub(super) fn overridden(forced: Option<ForcedState>, status: button::Status) -> button::Status {
    match forced {
        // `Rest` = 取证图的常态：**明确**画成 Active，把系统光标带来的 hover/pressed 压掉。
        // 截图通道默认就是它（见 `cli::Options::parse` 的尾部），否则同一份代码会因指针位置出不同的图。
        Some(ForcedState::Rest) => button::Status::Active,
        Some(ForcedState::Hover) => button::Status::Hovered,
        Some(ForcedState::Pressed) => button::Status::Pressed,
        None => status,
    }
}

pub(super) fn filled(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
        ..container::Style::default()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
