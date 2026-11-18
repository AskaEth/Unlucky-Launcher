//! M116 的**受控对照实验**：同进程里造几种窗口样式，逐个读 DWM 属性。
//!
//! ## 为什么需要它
//!
//! 用户的观察是「同一台机器上别的窗口都有圆角，只有我们的没有」。于是「没圆角」至少有三种
//! 可能：①这是 DWM 的默认行为（我们没申请）；②某个**样式位**把它 opt-out 了；③属性根本设不上。
//!
//! 只在自己的窗口上读一次，分不清 ①②③。所以这里**同一进程**里造几个窗口，除「样式」外
//! 条件全相同：
//!
//! | 标签 | 样式 | 它代表什么 |
//! |---|---|---|
//! | `decorated` | `WS_OVERLAPPEDWINDOW` | 用户说的「别的窗口」（对照） |
//! | `probe:popup` | 裸 `WS_POPUP` | **对照，不是产品形状**：带 owner 的弹出窗才用这一位 |
//! | `probe:popup+layered` | `WS_POPUP` + `WS_EX_LAYERED`(α) | **对照**：把两个嫌疑同时打开 |
//! | `product` | [`PRODUCT_STYLE`]（α = 1，不补分层） | 产品的**真实**样式（出厂态） |
//! | `product+alpha` | [`PRODUCT_STYLE`] + `WS_EX_LAYERED`(α) | 产品真实样式 + 用户把整窗 α 拉低 |
//!
//! ## ⚠️ 产品窗口**不是** `WS_POPUP`（M116 r4 复审订正）
//!
//! iced/winit 在 `decorations = false` 下**不会**用 `WS_POPUP`：winit 的
//! `window_state.rs::to_window_styles` 起点是 `WS_CAPTION|WS_BORDER|WS_CLIPSIBLINGS|WS_SYSMENU`，
//! 可缩放/可最大化时再加 `WS_THICKFRAME|WS_MINIMIZEBOX|WS_MAXIMIZEBOX`；`WS_POPUP` 只在
//! `WindowFlags::POPUP`（**带 owner** 的弹出窗）时才加。产品的「无边框观感」来自
//! `WM_NCCALCSIZE` 把客户区铺满，**不是**样式位。
//!
//! 所以本文件早期那句「`popup+layered` = 本产品窗口的完整形状」是**错的**：那一行的读数
//! （尤其 `Δframe = +0,+0`）只属于假形状，不能拿去解释产品窗口。要解释产品，看 `product`
//! 与 `product+alpha` 两行。
//!
//! ## 这个实验能证明什么、不能证明什么
//!
//! **能**：属性层能不能开（`DwmSetWindowAttribute` 的 `HRESULT`）、开完 DWM 侧回读是多少、
//! 各样式之间 `DWMWA_VISIBLE_FRAME_BORDER_THICKNESS` 与
//! `DWMWA_EXTENDED_FRAME_BOUNDS - GetWindowRect` 的数值差是否随样式变化。
//!
//! **不能**：角上到底是不是透明的（那要合成输出像素，本机 RDP 断连拿不到）。所以本文件
//! 的读数只回答「属性层」，观感归真机验收——报告里逐条标注。

#[cfg(target_os = "windows")]
use super::alpha::{apply_window_alpha, ensure_layered};
// `CornerOutcome` 与平台无关（`ProbeRow` 的字段类型、非 Windows 的 `empty_row` 都用它），
// 其余九个符号只服务于 Windows 的 `run_one` —— 分开写，否则非 Windows 上整条导入无人使用
// （`unused_imports`，M147）。
use super::dwm::CornerOutcome;
#[cfg(target_os = "windows")]
use super::dwm::{
    corner_preference_hr, extended_frame_bounds, set_preference, visible_border_thickness,
    window_rect, window_style, DWMWCP_DONOTROUND, DWMWCP_ROUND,
};

/// `WS_POPUP`（`0x8000_0000`）：**带 owner 的弹出窗**才用它；产品主窗口没有 owner，不用。
pub const WS_POPUP: u32 = 0x8000_0000;
/// `WS_OVERLAPPEDWINDOW`：普通有边框窗口（对照组的「别的窗口」）。
pub const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;

/// `WS_CAPTION`（`WS_BORDER | WS_DLGFRAME`）。
const WS_CAPTION: u32 = 0x00C0_0000;
/// `WS_BORDER`（= `WS_THICKFRAME` 的低位别名，`0x0080_0000`）。
const WS_BORDER: u32 = 0x0080_0000;
/// `WS_CLIPSIBLINGS`：子窗口层叠顺序位（DWM 不看它，winit 的起点里有）。
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
/// `WS_SYSMENU`。
const WS_SYSMENU: u32 = 0x0008_0000;
/// `WS_THICKFRAME`（可缩放边框）。
const WS_THICKFRAME: u32 = 0x0004_0000;
/// `WS_MINIMIZEBOX`。
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
/// `WS_MAXIMIZEBOX`。
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;

/// **产品窗口的真实样式位**（iced/winit，`decorations = false`，可缩放、可最大化、可最小化）。
///
/// 出处（可复核）：winit `window_state.rs::to_window_styles` 从
/// `WS_CAPTION|WS_BORDER|WS_CLIPSIBLINGS|WS_SYSMENU` 起手，`RESIZABLE` / `MAXIMIZABLE` /
/// `MINIMIZABLE` 分别加 `WS_THICKFRAME` / `WS_MAXIMIZEBOX` / `WS_MINIMIZEBOX`；`WS_POPUP`
/// 只在 `WindowFlags::POPUP`（带 owner）时才加——产品主窗口不带。
///
/// 这些**位**是输入（照抄 winit 的产物，带出处）；表里那一行 `product` 的**读数**是跑出来的，
/// 不是我写死的。
pub const PRODUCT_STYLE: u32 = WS_CAPTION
    | WS_BORDER
    | WS_CLIPSIBLINGS
    | WS_SYSMENU
    | WS_THICKFRAME
    | WS_MINIMIZEBOX
    | WS_MAXIMIZEBOX;

/// `WS_EX_LAYERED` 的 `u32` 形态（`create_probe_window` 的参数类型）。
const WS_EX_LAYERED_U32: u32 = 0x0008_0000;

/// 实验里的一行（一个窗口样式）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRow {
    /// 这一行是什么样式（见模块文档的表）。
    pub label: &'static str,
    /// 创建时请求的样式位。
    pub style: u32,
    /// 创建时请求的扩展样式位。
    pub ex_style: u32,
    /// 读回的 `GWL_STYLE`（证明请求的样式真的生效了）。
    pub style_readback: Option<isize>,
    /// 什么都没设时 DWM 侧的偏好值（`0` = `DWMWCP_DEFAULT`）。
    pub preference_default: Option<u32>,
    /// 读上面那一格时的 `HRESULT`（`0` = `S_OK`；非 0 说明这个会话读不出 DWM 属性）。
    pub preference_read_hresult: u32,
    /// 设为 [`DWMWCP_ROUND`] 的结局（含回读）。
    pub set_round: CornerOutcome,
    /// 设为 [`DWMWCP_DONOTROUND`] 的结局（含回读）。
    pub set_square: CornerOutcome,
    /// 默认状态下的可见边框厚度（像素）。
    pub border_default: Option<u32>,
    /// 圆角状态下的可见边框厚度（像素）。
    pub border_round: Option<u32>,
    /// 直角状态下的可见边框厚度（像素）。
    pub border_square: Option<u32>,
    /// `extended_frame_bounds - GetWindowRect` = `(dx, dy)`（各边差值之和）；
    /// `None` = 任一侧读不到。
    pub frame_delta: Option<(i32, i32)>,
    /// 窗口是否真的造出来了（`false` = 本机造不出窗口，整行无效）。
    pub created: bool,
}

impl ProbeRow {
    /// 表格里的一行（对齐成文本，报告直接贴）。
    ///
    /// **一格一格拼**而不是一条长 `format!`：位置参数一多，「多了/少了一个占位符」在编译期报、
    /// 在报告里却表现为整张表错位；这里列数由 [`TABLE_HEADER`] 与 [`Self::cells`] 同时决定，
    /// 单测比对竖线数。
    #[must_use]
    pub fn describe(&self) -> String {
        format!("| {} |", self.cells().join(" | "))
    }

    /// 各列的原始文本（与 [`TABLE_HEADER`] 同序）。
    #[must_use]
    pub fn cells(&self) -> Vec<String> {
        let show = |value: Option<u32>| value.map_or_else(|| "?".to_owned(), |v| v.to_string());
        let delta = self
            .frame_delta
            .map_or_else(|| "?".to_owned(), |(dx, dy)| format!("{dx:+},{dy:+}"));
        vec![
            format!("{:<14}", self.label),
            format!("0x{:08X}", self.style),
            format!("0x{:08X}", self.ex_style),
            self.style_readback
                .map_or_else(|| "?".to_owned(), |style| format!("0x{:08X}", style as u32)),
            show(self.preference_default),
            format!("0x{:08X}", self.preference_read_hresult),
            show(self.border_default),
            show(self.border_round),
            show(self.border_square),
            delta,
            self.set_round.describe(),
            self.set_square.describe(),
            if self.created { "ok" } else { "FAILED" }.to_owned(),
        ]
    }
}

/// 表头（与 [`ProbeRow::describe`] 的列一一对应）。
pub const TABLE_HEADER: &str = "| style | requested | exstyle | readback | pref | read_hr | bdef | brnd | bsq | Δframe | set ROUND | set SQUARE | created |";

/// 跑一遍受控对照实验（非 Windows 返回空表）。
///
/// 前两行是对照（**不是产品形状**），后两行才是产品：`product` = 出厂态（α = 1，不补分层），
/// `product+alpha` = 用户把整窗 α 拉低之后的真实形状。
///
/// 每个窗口用完立刻销毁：这是诊断路径，不许留 HWND。
#[must_use]
pub fn controlled_experiment() -> Vec<ProbeRow> {
    let mut rows = Vec::new();
    for (label, style, ex_style) in [
        ("decorated", WS_OVERLAPPEDWINDOW, 0u32),
        ("probe:popup", WS_POPUP, 0u32),
        ("probe:popup+layered", WS_POPUP, WS_EX_LAYERED_U32),
        ("product", PRODUCT_STYLE, 0u32),
        ("product+alpha", PRODUCT_STYLE, WS_EX_LAYERED_U32),
    ] {
        rows.push(run_one(label, style, ex_style));
    }
    rows
}

/// 一个样式的完整读数。`created = false` 时其余字段全 `None`（**不许**拿 0 冒充读数）。
#[cfg(target_os = "windows")]
fn run_one(label: &'static str, style: u32, ex_style: u32) -> ProbeRow {
    use super::{create_probe_window, destroy_probe_window, show_probe_window};

    let hwnd = create_probe_window(style, ex_style);
    if hwnd == 0 {
        return empty_row(label, style, ex_style);
    }
    show_probe_window(hwnd);
    // D36 的整窗 α：只有 layered 那一行才有意义（`apply_window_alpha` 对 α=1 不下发）。
    if ex_style & WS_EX_LAYERED_U32 != 0 {
        ensure_layered(hwnd);
        let _ = apply_window_alpha(hwnd, Some(0.8));
    }

    let (preference_default, preference_read_hresult) = corner_preference_hr(hwnd);
    let border_default = visible_border_thickness(hwnd);
    let set_round = set_preference(hwnd, DWMWCP_ROUND);
    let border_round = visible_border_thickness(hwnd);
    let set_square = set_preference(hwnd, DWMWCP_DONOTROUND);
    let border_square = visible_border_thickness(hwnd);

    let frame_delta =
        window_rect(hwnd)
            .zip(extended_frame_bounds(hwnd))
            .map(|(plain, extended)| {
                (
                    (extended.0 - plain.0) + (extended.2 - plain.2),
                    (extended.1 - plain.1) + (extended.3 - plain.3),
                )
            });

    let row = ProbeRow {
        label,
        style,
        ex_style,
        style_readback: window_style(hwnd),
        preference_default,
        preference_read_hresult,
        set_round,
        set_square,
        border_default,
        border_round,
        border_square,
        frame_delta,
        created: true,
    };
    destroy_probe_window(hwnd);
    row
}

/// 非 Windows：造不出 Win32 窗口，整行标记 `created = false`（不许编读数）。
#[cfg(not(target_os = "windows"))]
fn run_one(label: &'static str, style: u32, ex_style: u32) -> ProbeRow {
    empty_row(label, style, ex_style)
}

fn empty_row(label: &'static str, style: u32, ex_style: u32) -> ProbeRow {
    ProbeRow {
        label,
        style,
        ex_style,
        style_readback: None,
        preference_default: None,
        preference_read_hresult: 0,
        set_round: CornerOutcome::UnsupportedPlatform,
        set_square: CornerOutcome::UnsupportedPlatform,
        border_default: None,
        border_round: None,
        border_square: None,
        frame_delta: None,
        created: false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::dwm::frame_label;
    use super::{
        controlled_experiment, ProbeRow, PRODUCT_STYLE, TABLE_HEADER, WS_OVERLAPPEDWINDOW, WS_POPUP,
    };

    /// **抓得住**：样式常量写错（`WS_POPUP` 少一个 0 就变成别的位，对照实验的前提就没了）。
    #[test]
    fn the_style_constants_match_the_win32_header() {
        assert_eq!(WS_POPUP, 0x8000_0000);
        assert_eq!(WS_OVERLAPPEDWINDOW, 0x00CF_0000);
    }

    /// **产品窗口不是 popup**（M116 r4 复审的核心事实，钉住它，别再写回去）。
    ///
    /// 抓得住：有人把 `PRODUCT_STYLE` 改回 `WS_POPUP`（那会让「产品形状」整条证据链回到错的假设上）；
    /// 也抓得住 `PRODUCT_STYLE` 丢掉 `WS_CAPTION`/`WS_THICKFRAME`（那会让 `Δframe` 从 `+0,−7`
    /// 掉成 `+0,+0`，把「产品没有边框带」这个错结论复活）。
    /// 抓不住：真进程上 winit 到底置了哪些位（那看 `[corner] … frame=` 的运行时读数）。
    #[test]
    fn the_product_window_is_overlapped_not_popup() {
        assert_eq!(
            PRODUCT_STYLE & WS_POPUP,
            0,
            "产品主窗口没有 owner ⇒ 不带 WS_POPUP"
        );
        assert_ne!(
            PRODUCT_STYLE & 0x00C0_0000,
            0,
            "WS_CAPTION 在（winit 起点）"
        );
        assert_ne!(PRODUCT_STYLE & 0x0004_0000, 0, "WS_THICKFRAME 在（可缩放）");
        assert_eq!(frame_label(PRODUCT_STYLE as isize), "overlapped");
        assert_eq!(frame_label(WS_POPUP as isize), "popup");
    }

    /// 实验固定五行，顺序固定：两行对照 + 两行产品 + `decorated` 参照。
    ///
    /// 抓得住：有人把分层那一行或产品行删了/调换——那样「layered 会不会让 DWM 忽略圆角请求」
    /// 与「产品真实形状有没有边框带」这两条对照就都没了。
    /// 抓不住：读数本身（那要么真机跑，要么看报告里的表）。
    #[test]
    fn the_experiment_always_covers_the_same_five_rows_in_order() {
        let rows = controlled_experiment();
        assert_eq!(rows.len(), 5);
        let labels: Vec<_> = rows.iter().map(|row| row.label).collect();
        assert_eq!(
            labels,
            [
                "decorated",
                "probe:popup",
                "probe:popup+layered",
                "product",
                "product+alpha"
            ]
        );
        assert_eq!(rows[0].style, WS_OVERLAPPEDWINDOW);
        assert_eq!(rows[1].style, WS_POPUP);
        assert_eq!(rows[2].ex_style & 0x0008_0000, 0x0008_0000);
        // 产品两行：样式位相同（都是真实样式），只有 layered 一位不同。
        assert_eq!(rows[3].style, PRODUCT_STYLE);
        assert_eq!(rows[4].style, PRODUCT_STYLE);
        assert_eq!(rows[3].ex_style & 0x0008_0000, 0, "出厂态 α=1 ⇒ 不补分层");
        assert_eq!(rows[4].ex_style & 0x0008_0000, 0x0008_0000, "α<1 ⇒ 补分层");
    }

    /// 表头与每行的竖线数一致（`format!` 里多/少一列会让表格在报告里错位）。
    #[test]
    fn the_table_columns_line_up() {
        let header_pipes = TABLE_HEADER.matches('|').count();
        for row in controlled_experiment() {
            let line = row.describe();
            assert_eq!(
                line.matches('|').count(),
                header_pipes,
                "列数不一致：{line}"
            );
        }
        // 没造出窗口的行必须**没有**读数（不许用 0 冒充）。
        let failed = ProbeRow {
            created: false,
            ..controlled_experiment().into_iter().next().expect("五行")
        };
        assert!(failed.describe().contains("FAILED"));
    }
}
