//! 导航轨**底部账户头像位**（M114）：状态结构体 + 那个按钮。
//!
//! 为什么单独一个文件：`shell/nav.rs` 贴着 400 行硬线，而这块是 M114 新增的一整块
//! （未登录/已登录两态、窄高收成图标条的判据、点击跳账户页）。

use iced::widget::{button, column, container, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme as IcedTheme};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::{LayoutMetrics, Space};
use unlucky_design::typography::TypeRole;

use crate::app::components::interaction::{self, Interaction};
use crate::app::Message;
use crate::cli::ForcedState;
use crate::icons;
use crate::shell::overridden;
use crate::theme::Ui;

use super::tip::tip;
use super::{nav_item_height, NavItem};

/// 导航轨底部的**账户头像位**状态（M114）+ 轨自身的折叠态（M300）。
///
/// 为什么单独传而不是塞进 `NavItem`：它不是主序列里的第 6 项（那会让轨道越堆越长），
/// 而是固定在**底部**的一个头像位；未登录/已登录两态要在**形状上**就不同（描边剪影 vs 首字母块）。
///
/// ## 为什么折叠态也搭这个结构（M300）
///
/// `shell::NavRail`（`shell/mod.rs`）是 `nav::rail(..)` 唯一的入参通道，而那个文件不在本
/// mission 的 scope 里；`RailAccount` 是**唯一**由 `app/view/rail.rs`（scope 内）构造、又直接递给
/// `nav::rail`（scope 内）的值。于是轨的「这一帧多宽 / 标签收不收」搭它进来，一个字段都不新增到
/// `NavRail` 上。代价是这个结构现在有两个职责——已经比「动 `shell/mod.rs` 去加第三个字段」便宜，
/// 且它们确实是同一块几何的两个输入（轨的宽与轨底的内容）。
///
/// `f32` 的进度进场后 `Eq` 不再成立（`PartialEq` 仍在）：调用方只比字段，不做整结构相等。
#[derive(Debug, Clone, PartialEq)]
pub struct RailAccount {
    /// 是否已登录（至少有一个账户）
    pub signed_in: bool,
    /// 首字母块的字（已登录时有意义）
    pub initial: String,
    /// 账户名（已登录时作为标签；未登录时是 i18n 的「未登录」）
    pub label: String,
    /// 窄高度/窄宽下的图标条形态（隐藏文字标签，只留头像位）
    pub compact: bool,
    /// 用户把轨折叠了（M300）：只留图标 + tooltip。判据只有一处（`ItemView::labels_visible`）。
    nav_collapsed: bool,
    /// 折叠进度的**这一帧**取值（0 = 折叠 64，1 = 展开 88）。中间帧由 `motion::nav(open)` 给。
    nav_progress: f32,
}

impl RailAccount {
    /// 未登录态。
    pub fn signed_out(label: impl Into<String>) -> Self {
        Self {
            signed_in: false,
            initial: String::new(),
            label: label.into(),
            compact: false,
            // 出厂：轨展开（`UiPrefs::nav_collapsed` 的出厂值也是 false）
            nav_collapsed: false,
            nav_progress: 1.0,
        }
    }

    /// 已登录态。
    pub fn signed_in(initial: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            signed_in: true,
            initial: initial.into(),
            label: label.into(),
            compact: false,
            nav_collapsed: false,
            nav_progress: 1.0,
        }
    }

    /// 图标条形态（导航项太多/太窄时收起文字标签）。
    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// 轨的折叠档 + **这一帧**的过渡进度（M300）。
    ///
    /// 两个值必须一起给：只给 `collapsed` 会让布局按一个过期的进度算宽，只给进度会让标签在
    /// 折叠档里还画着。`progress` 会被夹到 `0..=1`（`nav_width` 里再夹一次，这里只是让调用方
    /// 传什么都不会把轨画成负数宽）。
    #[must_use]
    pub fn nav(mut self, collapsed: bool, progress: f32) -> Self {
        self.nav_collapsed = collapsed;
        self.nav_progress = progress.clamp(0.0, 1.0);
        self
    }

    /// 轨现在是不是折叠档（标签该不该收）。
    #[must_use]
    pub fn nav_collapsed(&self) -> bool {
        self.nav_collapsed
    }

    /// 这一帧的轨宽（展开 88 ⇄ 折叠 64；中间帧按动效进度插值）。
    ///
    /// **唯一实现点是 `super::nav_width`**：`nav::rail`、`account_item`、`toggle_button` 与用例
    /// 都读它，于是「谁把这四个调用点各算一遍」在结构上不可能。
    #[must_use]
    pub fn nav_width(&self, metrics: LayoutMetrics) -> f32 {
        super::nav_width(metrics, self.nav_progress)
    }
}

/// 轨底的账户头像位：未登录 = 描边剪影，已登录 = 首字母块（两态在**形状**上就不同）。
pub(super) fn account_item<'a>(
    ui: &'a Ui,
    selected: NavItem,
    account: &RailAccount,
    indicator: (f32, f32),
    forced: Option<ForcedState>,
) -> Element<'a, Message> {
    let (indicator_width, indicator_height) = indicator;
    let active = selected == NavItem::Account;
    // 取证态与运行期态是**同一个** `Interaction`：强制态由 `overridden` 直接推（`None` → `Active`
    // → `Enabled`），运行期的真态由 `interaction::tracked` 的取样层在 draw 里给。
    let state = Interaction::from_status(overridden(forced, button::Status::Active));
    let label_color = if active {
        ui.color(Role::Secondary)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let avatar: Element<'a, Message> = if account.signed_in {
        let style = ui.emphasized(TypeRole::TitleMedium);
        container(
            text(account.initial.clone())
                .size(style.size_px)
                .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                    style.line_height_px,
                )))
                .font(ui.font_of(style))
                .color(ui.color(Role::OnSecondaryContainer)),
        )
        .width(Length::Fixed(avatar_size(ui, indicator_width)))
        .height(Length::Fixed(avatar_size(ui, indicator_width)))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            // 状态层画在**头像这个圆**上（M283），不再摊到 88×52 的按钮上。
            background: avatar_color(ui, state, true).map(Background::Color),
            border: Border {
                radius: ui.radius(ShapeScale::Full, avatar_size(ui, indicator_width)),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
    } else {
        container(icons::view::<Message>(
            IconKind::User,
            ui.color(Role::OnSurfaceVariant),
        ))
        .width(Length::Fixed(avatar_size(ui, indicator_width)))
        .height(Length::Fixed(avatar_size(ui, indicator_width)))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            // 未登录的头像静息只有描边：状态层照样落在**这个圆**里（M283）。
            background: avatar_color(ui, state, false).map(Background::Color),
            border: Border {
                color: ui.color(Role::Outline),
                width: 1.0,
                radius: ui.radius(ShapeScale::Full, avatar_size(ui, indicator_width)),
            },
            ..container::Style::default()
        })
        .into()
    };

    let width = account.nav_width(ui.metrics());
    let height = nav_item_height(ui, indicator_height);
    // M300：折叠档与「窄高图标条」是同一条判据——名字（"Not signed in" / 账户名）在 64 宽的轨里
    // 放不下，收起来之后可读名改由 tooltip 给（与主序列项同一条纪律）。
    let labels_visible = !account.compact && !account.nav_collapsed();
    let mut stack = column![avatar]
        .spacing(ui.space(Space::X1))
        .width(width)
        .align_x(Alignment::Center);
    if labels_visible {
        // 用**拥有的**字符串画标签：`RailAccount` 是调用方临时算出来的状态，
        // 借它的 `&str` 会让整棵树跟着那个临时值一起活（E0515）。
        let style = ui.style(TypeRole::LabelSmall);
        stack = stack.push(
            text(account.label.clone())
                .size(style.size_px)
                .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                    style.line_height_px,
                )))
                .font(ui.font_of(style))
                .color(label_color),
        );
    }

    let clickable: Element<'a, Message> = button(stack)
        .width(width)
        .height(height)
        .padding(0)
        .style(move |_theme: &IcedTheme, _status| button::Style {
            // 底色**恒为 `None`**：状态层由头像圆给（M283，同一个元素不许并存两种形状）。
            background: None,
            text_color: label_color,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        })
        .on_press(Message::Select(NavItem::Account))
        .into();
    // A8：运行期态套状态取样控件（强制态 = 取证，**不套**：取证图不许有过渡中间帧）。
    let clickable = if forced.is_none() {
        interaction::tracked(clickable, true)
    } else {
        clickable
    };
    // M300：名字不画的时候，头像位也必须说得出自己是谁（否则折叠态的轨底就是一个孤零零的剪影）。
    if labels_visible {
        clickable
    } else {
        tip(ui, clickable, account.label.clone())
    }
}

/// 头像圆的底色（**几何**的唯一实现点）：hover / pressed / 过渡帧都画在这个 Ø40 圆上。
///
/// **颜色**只有一个来源：已登录支走 [`interaction::selected_fill`]（全仓唯一的选中底色入口，
/// M299 收口了这里的私有拷贝），未登录支走 [`interaction::layer_color`]。两条分支不许合并：
/// 已登录的头像静息是实色 `SecondaryContainer`（`selected_fill` 的兜底），未登录的头像静息只有
/// 描边、不填底（`layer_color` 给 `None`）。
fn avatar_color(ui: &Ui, state: Interaction, signed_in: bool) -> Option<Color> {
    if signed_in {
        // 同一个唯一入口（M299）：已登录头像的静息底是实色 `SecondaryContainer`，
        // hover / pressed 叠在它上面——此前这里是第五份私有拷贝。
        Some(interaction::selected_fill(
            ui,
            state,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer,
        ))
    } else {
        // 头像直接摆在导航轨上（轨画的是 `MaterialLayer::Canvas`）：状态层要叠在同一个角色上。
        interaction::layer_color(
            ui,
            state,
            Role::OnSurface,
            MaterialLayer::Canvas.surface_role(),
        )
    }
}

/// 头像位边长 = 指示器宽 − 16，但不超过 48（12b 里没有这个控件，按令牌栅格取整）。
fn avatar_size(ui: &Ui, indicator_width: f32) -> f32 {
    (indicator_width - Space::X4.dp()).min(ui.space(Space::X8) + Space::X2.dp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_states_are_distinguishable() {
        let out = RailAccount::signed_out("Not signed in");
        assert!(!out.signed_in);
        let on = RailAccount::signed_in("S", "Steve");
        assert!(on.signed_in);
        assert_eq!(on.initial, "S");
        let compacted = on.clone().compact(true);
        assert!(compacted.compact, "compact(true) 必须收成图标条");
    }

    /// **已登录头像也跟态走**，且走的是全仓唯一的那个入口（M299）。
    ///
    /// 抓得住：把 `signed_in` 支改回「选中就不叠层」⇒ hover / pressed 都等于静息，前两条红；
    /// hover 与 pressed 同源 ⇒ 第三条红。未登录支（只有描边、不填底）一并钉住。
    /// 抓不住：像素（`DevDocs/artifacts/m299/shell-account-*`）；Ø40 几何本次没动。
    #[test]
    fn the_signed_in_avatar_reacts_to_hover_and_press() {
        use unlucky_design::color::SchemeFlavor;
        use unlucky_design::typography::Platform;

        use crate::theme::Families;

        let ui = Ui::with_platform(
            SchemeFlavor::Light,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        );
        let fill = |state| avatar_color(&ui, state, true).expect("已登录静息也有底");
        let rest = fill(Interaction::Enabled);
        let hover = fill(Interaction::Hovered);
        let pressed = fill(Interaction::Pressed);

        assert_eq!(rest, ui.color(Role::SecondaryContainer));
        assert_ne!(hover, rest, "已登录 + hover 必须与静息不同");
        assert_ne!(pressed, rest, "已登录 + pressed 必须与静息不同");
        assert_ne!(hover, pressed, "8% 与 10% 必须能区分");

        // 未登录：静息不填底，hover 才有状态层
        assert_eq!(avatar_color(&ui, Interaction::Enabled, false), None);
        assert!(avatar_color(&ui, Interaction::Hovered, false).is_some());
    }

    #[test]
    fn the_avatar_slot_fits_the_rail() {
        use unlucky_design::color::SchemeFlavor;
        use unlucky_design::typography::Platform;

        use crate::theme::Families;

        let ui = Ui::with_platform(
            SchemeFlavor::Light,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        );
        let width = super::super::indicator_size(ui.metrics()).0;
        let size = avatar_size(&ui, width);
        assert!(size > 0.0);
        // 展开档是两档里宽的那一档：头像位连它都放得下，折叠档才不需要另算一遍。
        let rail = ui.metrics().nav_rail_width;
        assert!(size <= rail, "头像位不能比导航轨还宽（{size} > {rail}）");
    }
}
