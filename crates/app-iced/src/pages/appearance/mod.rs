//! # 外观接线（M93 / APPEARWIRE）：**键 → 像素**
//!
//! 本模块把 `appearance.*` 那批键接到真正的窗口与真正的像素上，并把「哪里是判据、哪里是观感」
//! 分开写清楚。底座（M91）已经给了键、钳位判据与唯一的不安全落点；这里补的是**调用方**。
//!
//! ## 两层透明度（D34 / D36）
//!
//! | 层 | 作用范围 | 实现 | CPU 档 | 下限 |
//! |---|---|---|---|---|
//! | 整窗 α | 整扇窗口（标题栏 + 导航轨 + 内容 + 卡片） | `SetLayeredWindowAttributes(LWA_ALPHA)`，DWM 常数乘法 | **允许** | 0.08（α=0 = 整窗穿透，拒绝） |
//! | 面板 α | 只有面板/卡片表面这一层（背景图能透出来） | 容器色的 alpha（[`section`] / `shell`） | **强制 1.0** | 0.4 |
//!
//! 真值表在 [`alpha`] 的模块文档里（三层 × 三个后端，含「钳位还是拒绝」）。
//!
//! ## 模块分工
//!
//! | 文件 | 是什么 | 有没有 IO |
//! |---|---|---|
//! | [`alpha`] | 两层 α 的钳位链、CPU 档降级、滑块映射、下发字节 | 无（纯函数） |
//! | [`background`] | 背景图解码（png/jpeg/bmp）、平铺网格、暗化系数、可读诊断 | 读盘一次（`load`） |
//! | `palette` | D38 三级优先序的调用侧 + 提示文案 | 无（像素由 `background` 给） |
//! | `state` | 运行期状态：把上面三件的输入输出连起来 | 只在 `refresh_background` |
//! | `section` | 设置页「外观」分组（真控件） | 无 |
//! | `picture` | 背景层 Element：真图直出（Cover/Contain/Tile）+ 暗化叠层 | 无 |
//!
//! ## 偏离登记（D30 要求逐条写清）
//!
//! | # | 偏离 | 为什么 | 什么时候收口 |
//! |---|---|---|---|
//! | 1 | ~~背景图按 36dp 马赛克画~~ → **r2 已销**：iced 的 `image-without-codecs` 打开，真图直出（`ContentFit` + 平铺网格） | r1 时 `Cargo.toml` 不在 scope；M93 r2 由 tower 扩 scope 解决 | ✅ |
//! | 2 | ~~面板 α 只覆盖外壳层~~ → **r2 已销**：α 收敛到 `Ui::layer`（唯一施加点），`frame::card` / 页头带 / 外壳 / 外观卡片自动同源 | M93 r2 扩了 `pages/frame.rs` + `theme.rs` 的 scope | ✅ |
//! | 3 | 桌面壁纸取色（第二级）永远 `Absent` | 壁纸探测（M73）还没落地；优先序本身仍按 design 的三级走，测试里用合成像素证明第二级会被用到 | M73 |
//! | 4 | 色卡档**不换** `Ui` 的 scheme | `resolve_source` 的色卡档返回 `DEFAULT_CARD`（天蓝卡）的方案，与令牌 baseline 逐色不同；应用它会把全部既有验收图的像素改掉。取色档（①②）才换 | 色卡选择 UI 落地时一并决议 |
//! | 5 | `Backend::Auto` 不触发 CPU 档降级 | 「强制 1.0」是 CPU 渲染档的规则，而 `Auto` 是「探测前的请求值」；探测结果没有回灌 `Backend` | 让 `probe` 把结果回灌 `Options::backend` |
//! | 6 | ~~背景图只支持 png~~ → **r2 已销**：`image` 补了 `jpeg`/`bmp`，单测与截图都用 `.jpg` 举证 | 同上 | ✅ |
//! | 7 | `bg_dim` 是「往**面板底色**退」，不是「叠黑」 | 浅色主题下叠黑会把亮图压成暗块，深色正文反而更读不清；向底色退在两种主题下都单调变好（见 [`background::dim_alpha`]） | 若将来为了「夜间压暗」需要真叠黑，加一档独立键（不要复用这个） |
//! | 8 | ~~页头文字压在画布上~~ → **r2 已销**：`frame::header_bar` 加了面板底色 | M93 r2 扩了 `pages/frame.rs` 的 scope | ✅ |
//!
//! ## 取证
//!
//! 每一处改动都打一行 [`state::State::log_line`]：`[appearance] window_alpha=… panel_alpha=…
//! byte=… corner=…`；圆角另有一条独立管线，行形如
//! `[corner] <ctx> corner=policy=system source=dwm os=10.0.26100 excluded_reason=none
//! radius=system pref=2 readback=? read_hr=0x80070057 frame=overlapped layered=on
//! suppressed_by=window_alpha`（真进程实跑的两条见 `DevDocs/artifacts/m116/README.md`；
//! `frame=` 恒为 `overlapped`/`other`，**不会是 `popup`**）
//! （见 `app::update::appearance` 与 `platform::win32::corner::format_line`）。数值是**下发前**的
//! 字节/属性值（不是观感），因为在 RDP + 虚拟显示适配器上「看上去多透明 / 圆不圆」不可信。
//! 截图矩阵用 `ULL_APPEARANCE_*` 环境变量灌值（见 [`state`] 的常量）。

use iced::widget::container;
use iced::{Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::pages::frame;
use crate::pages::Message;
use crate::theme::Ui;

pub mod alpha;
pub mod background;

mod palette;
mod picture;
mod section;
mod state;

/// 状态提示的 **tonal notice**（`section.rs` 的 400 行硬顶把它放在模块根）。
///
/// `SurfaceContainerHighest` 底 + `ShapeScale::Small` 圆角 + 两侧 `X2` 内边距，文字 `OnSurfaceVariant`。
/// 用它而不是「换色的正文」：容器才是 M3 表达「当前状态」的形态，也和说明段在**形态**上分开
/// （不靠色相、不新增令牌）。调用点见 [`section::section`] 的圆角一节。
fn notice<'a>(ui: &'a Ui, text: &str) -> Element<'a, Message> {
    let background = ui.color(Role::SurfaceContainerHighest);
    let tone = ui.color(Role::OnSurfaceVariant);
    let body = frame::text_role_owned(ui, TypeRole::BodySmall, true, text.to_owned(), tone);
    let pad = ui.space(Space::X2);
    let radius = ui.radius_dp(ShapeScale::Small);
    container(body)
        .width(Length::Fill)
        .padding(Padding::from([ui.space(Space::X1), pad]))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(background)),
            border: Border {
                radius,
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub use picture::layer;
pub use section::section;
pub use state::State;

/// 设置页「外观」分组在左列里的下标（0 = General，1 = Appearance …）。
pub const APPEARANCE_GROUP: usize = 1;

/// 设置页「外观」分组的显示名。
pub const GROUP_LABEL: &str = "Appearance";

/// 一次外观改动（设置页 → `App`）。
///
/// 值都是**用户请求值**（未钳位）：钳位与降级只发生在 [`alpha`]，这样「用户设了 0.5、CPU 档实际
/// 1.0」这种差别在状态里能看出来，日志也就不会把请求值写成生效值。
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// 整窗透明度（请求值，0.08~1.0）。
    WindowOpacity(f32),
    /// 面板透明度（请求值，0.4~1.0）。
    PanelAlpha(f32),
    /// 背景图开关。
    BgEnabled(bool),
    /// 背景图路径（空串 = 清掉）。
    BgImage(String),
    /// 填充档。
    BgFill(unlucky_presenter::prefs::BgFill),
    /// 暗化强度（0~0.95）。
    BgDim(f32),
    /// 配色来源档。
    PaletteSource(unlucky_presenter::prefs::PaletteSource),
    /// 窗口圆角档位（M116：跟随系统 / 直角）。
    Corner(unlucky_presenter::prefs::WindowCorner),
    /// 设置页切分组（左列点选）。
    SelectGroup(usize),
}

impl Change {
    /// 这条改动对应哪个 schema 键（写回配置用；`SelectGroup` 不落盘）。
    #[must_use]
    pub fn key(&self) -> Option<&'static str> {
        Some(match self {
            Self::WindowOpacity(_) => unlucky_presenter::prefs::keys::WINDOW_OPACITY,
            Self::PanelAlpha(_) => unlucky_presenter::prefs::keys::PANEL_ALPHA,
            Self::BgEnabled(_) => unlucky_presenter::prefs::keys::BG_IMAGE_ENABLED,
            Self::BgImage(_) => unlucky_presenter::prefs::keys::BG_IMAGE,
            Self::BgFill(_) => unlucky_presenter::prefs::keys::BG_FILL,
            Self::BgDim(_) => unlucky_presenter::prefs::keys::BG_DIM,
            Self::PaletteSource(_) => unlucky_presenter::prefs::keys::PALETTE_SOURCE,
            Self::Corner(_) => unlucky_presenter::prefs::keys::WINDOW_CORNER,
            Self::SelectGroup(_) => return None,
        })
    }

    /// 这条改动的落盘字符串形态（与 `schema` 的取值词表逐字一致）。
    ///
    /// `None` = 不落盘（分组选择）。数字一律给 `f64` 的十进制字面量（`store.set` 按 schema 类型转）。
    #[must_use]
    pub fn value(&self) -> Option<String> {
        Some(match self {
            Self::WindowOpacity(alpha) => format!("{alpha}"),
            Self::PanelAlpha(alpha) => format!("{alpha}"),
            Self::BgEnabled(enabled) => enabled.to_string(),
            Self::BgImage(path) => path.clone(),
            Self::BgFill(fill) => fill.as_str().to_owned(),
            Self::BgDim(dim) => format!("{dim}"),
            Self::PaletteSource(source) => source.as_str().to_owned(),
            Self::Corner(corner) => corner.as_str().to_owned(),
            Self::SelectGroup(_) => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Change, APPEARANCE_GROUP};
    use unlucky_presenter::prefs::{keys, BgFill, PaletteSource};

    /// 抓得住：每条改动都点名到自己那个 schema 键（写错键 = 静默写到别的配置项上）。
    /// 抓不住：这些键是否真的在 core 的 schema 白名单里（那由 presenter/core 的测试守）。
    #[test]
    fn every_change_names_its_own_schema_key() {
        assert_eq!(APPEARANCE_GROUP, 1, "左列第 1 项是 Appearance");
        let cases = [
            (Change::WindowOpacity(0.8), keys::WINDOW_OPACITY),
            (Change::PanelAlpha(0.6), keys::PANEL_ALPHA),
            (Change::BgEnabled(true), keys::BG_IMAGE_ENABLED),
            (Change::BgImage("D:/x.png".to_owned()), keys::BG_IMAGE),
            (Change::BgFill(BgFill::Tile), keys::BG_FILL),
            (Change::BgDim(0.5), keys::BG_DIM),
            (
                Change::PaletteSource(PaletteSource::Wallpaper),
                keys::PALETTE_SOURCE,
            ),
            (
                Change::Corner(unlucky_presenter::prefs::WindowCorner::Square),
                keys::WINDOW_CORNER,
            ),
        ];
        for (change, key) in cases {
            assert_eq!(change.key(), Some(key), "{change:?}");
            assert!(change.value().is_some());
        }
        assert_eq!(Change::SelectGroup(2).key(), None, "切分组不落盘");
        assert_eq!(Change::SelectGroup(2).value(), None);
    }

    /// 抓得住：落盘字符串必须是 schema 认识的词表（枚举档写错就整条配置读回默认值）。
    /// 抓不住：数值是否在范围内（钳位在 core 侧，越界写入会被 schema 拒绝）。
    #[test]
    fn persisted_values_use_the_schema_vocabulary() {
        assert_eq!(
            Change::BgFill(BgFill::Contain).value().as_deref(),
            Some("contain")
        );
        assert_eq!(
            Change::PaletteSource(PaletteSource::BackgroundImage)
                .value()
                .as_deref(),
            Some("background-image")
        );
        assert_eq!(Change::BgEnabled(false).value().as_deref(), Some("false"));
        assert_eq!(Change::WindowOpacity(0.8).value().as_deref(), Some("0.8"));
    }
}
