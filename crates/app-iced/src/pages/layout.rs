//! 页面布局系统：**窗口宽度档位 → 版面 → 列宽**。纯函数，不碰 iced 控件树。
//!
//! 这是本 mission 的核心交付物，四个页面只是它的用例。`layout.rs` 是页面级几何的
//! **唯一定义点**：页面文件里不许再出现列宽字面量。
//!
//! ## 1. 宽度档位（官方响应式断点 600 / 840）
//!
//! 档位定义照官方 M3「Window size classes」（A 级：m3.material.io 的 app-quality/window-size-classes，
//! Compose `WindowSizeClass` 的同一组硬值）：**600dp 与 840dp** 是两个分界。
//!
//! | 档位 | 窗口宽度（逻辑 px @100% DPI） | 页边距（gutter） |
//! |---|---|---|
//! | `Compact` | `< 600` | 16（`Space::X4`） |
//! | `Medium` | `600 ≤ w < 840` | 24（`Space::X6`） |
//! | `Expanded` | `≥ 840` | 24（`Space::X6`） |
//!
//! 页边距取 16/24 而不是 16/24/32：官方 expanded 页边距也是 24，写 32 会白白多一条偏离登记。
//!
//! > 本 crate 的真实窗口下限是 880（`app.rs` 的 `MIN_SIZE`），所以日常只有 `Expanded`；
//! > 600 以下的档位是给「将来允许更窄窗口 / 内嵌面板」准备的，也让布局规则**可测**。
//!
//! ## 2. 三种版面与「什么宽度用哪种」
//!
//! | 版面 | 含义 | 页面上用它 | 成立条件 | 退化到 |
//! |---|---|---|---|---|
//! | `SingleColumn` | 单列，列宽 ≤ 行宽上限 | 下载 / 日志（全部档位）；实例 / 设置（窄窗） | 总是成立 | — |
//! | `SupportingPane` | 主列 + 窄侧列（侧列是辅助，主列是阅读对象） | 设置（分组导航 240 + 表单列） | `w ≥ 600` 且 `w ≥ 720` | `SingleColumn` |
//! | `ListDetail` | 固定列表列（320~360，**独立滚动**）+ 详情列 | 实例 | `w ≥ 840` 且 `w ≥ 816` | `SingleColumn` |
//!
//! 两个门槛都是**算出来的**，不是拍的（见 [`panes_min_window`]）：
//!
//! - 设置：`240（drawer）+ 24（间隙）+ 320（表单列下限）+ 2×24（gutter）+ 88（rail）= 720`；
//! - 实例：`336（列表列）+ 24（间隙）+ 320（详情列下限）+ 2×24（gutter）+ 88（rail）= 816`。
//!
//! 实例的几何门槛 816 低于官方 expanded 断点 840 —— 所以**档位是更严的那一条**，
//! 单测 `expanded_is_wide_enough_for_list_detail` 把这条「余量 24px」钉死。
//!
//! ## 3. 行宽约束（历史事故：M68 的全宽单卡）
//!
//! M68 骨架的内容区是**一张全宽卡**：卡片横跨 x=112..1156（**1044px**），卡片内文本区 **1004px**
//! （边距 24 + 卡片内边距 20）。那张卡里的正文是 `BodySmall` 12px：步进 = 14px 实测值
//! 6.0526 × 12/14 = **5.19px/字符**，所以 137 字符 × 5.19 ≈ **711px** —— 与
//! `DevDocs/ops/screens-m68/shell-light-cpu.png` 里那一行**实测 ink 712px**（x=133..845）吻合。
//! 视觉复审的判词因此是「**不能把全宽单卡当页面模板**，官方桌面口径是 supporting-pane / list-detail」。
//!
//! > **两个订正记录**（P2 复审抓出）：① **712 是那行的墨迹宽，不是列宽**（列宽 1004；旧文档的减法得 1044）；
//! > ② 那行是 **137 字符**（12px × 0.4323em = 5.19px/字符），早期拿 `0.5em` 估算既高估步进、也错了字号。
//! > 现在事故描述一律用**实测步进 × 实际字号**——这也是 `line_length_is_capped_at_72_characters`
//! > 里那条「旧口径 504px 能放 83 字符」护栏的由来。
//!
//! ### 逐行口径：72 字符是**最长一行**的上界，不是平均（r2 复审的教训）
//!
//! 「平均步进 × 72」只能保证**平均**一行 72 字符：r2 复审在交付图里抓到一行 **73 字符**
//! （`guess. Nothing on this page is wired to real data yet: the layout is, the`，ink 428px）。
//! 所以上限**不是算术结果，而是逐行折行模拟的结果**——用 FreeType 量宽 + 贪心词折行，
//! 对实例页那段正文（361 字符）扫不同列宽：
//!
//! | 列宽 | 最长一行 | 该行 ink | 结论 |
//! |---|---|---|---|
//! | 435（= 平均口径上界） | 73 字符 | 428px | ✗ 破了 72 |
//! | 428 | 73 字符 | 428px | ✗ |
//! | 426 | 71 字符 | 426px | ✓ 但只剩 1 字符余量 |
//! | **424**（4px 栅格） | **67 字符** | **402px** | ✓ 取这个：满足上界 + 留 5 字符余量 |
//!
//! 于是 [`measure_cap_px`] = **424**（= `min(逐行安全宽 424, 平均口径上界 435)`），由
//! [`content_column_width`] 与 [`frame_metrics`] 无条件施加：任何窗口宽度、任何版面，
//! 正文列都不会超过它。1180 宽窗口下正文列是 **424**（不是 712）。
//!
//! **作用域**：这条承诺只覆盖 `BodyMedium` 14px 的正文。`BodySmall` 12px 的辅助文案
//! （空态副文案、窄窗提示）在同一列宽下每行可到 ~82 字符——它们都是 1~2 行的短句，不做逐行承诺；
//! 中文（全角 1em）另算，等 i18n mission 定。
//!
//! ### M102 订正：`column` 是**正文（prose）列宽**，不是页面块宽
//!
//! 视觉复审抓到「正文列左缘 423 对标题 110」，根因是**把 424 当成整页块宽再居中**。
//! 现在三个量分开，口径与常量全在 [`block`]（本文件只留指针，免得文档把 file-size 门禁顶到红线）。
//!
//! ### 步进是怎么量的（口径 + 可复现）
//!
//! - **测的是哪组字形**：`BodyMedium` 的字体栈（本机解析到 Segoe UI **Regular**，14px），
//!   **无 letter-spacing**——令牌的 0.5px 字距 iced 0.14 不支持（已登记的令牌丢失），渲染里没有；
//! - **样本**：实例页详情那段正文（`pages/instances/data.rs` 的 `NOTE`，**361 字符**，
//!   大小写 / 标点 / 空格 / `→` 混排，是我们自己的真实正文，不是造出来的 `mmmm`）；
//! - **量法**：FreeType `getlength`（PIL `ImageFont.truetype(segoeui.ttf, 14)`）求整段排版宽，
//!   再除以字符数 → **2185.00px / 361 = 6.0526 px/字符 = 0.43233em**；× 72 = 435.79（这是**平均**口径上界，逐行口径见下）；
//! - **交叉验证**：同一段的首行（78 字符）渲染实测墨迹 **484px**，FreeType 对同一串给 479px
//!   （差 ~1%，抗锯齿外扩），说明「步进」这个口径在 iced 渲染器与 FreeType 之间是一致的；
//! - **CJK 不适用**：全角字形是 1em（14px），中文的 72 字符是 1008px。页面文案走 i18n 之后
//!   要按语言分别定上限，这条留给 i18n mission；本模块只保证英文基线。

use unlucky_design::spacing::{LayoutMetrics, Space};
use unlucky_design::typography::TypeRole;

pub mod block;
pub mod wrap;

#[cfg(test)]
pub(crate) mod source;

/// 官方 M3 窗口档的 medium 断点（600dp）。
pub const BREAKPOINT_MEDIUM: f32 = 600.0;
/// 官方 M3 窗口档的 expanded 断点（840dp）。
pub const BREAKPOINT_EXPANDED: f32 = 840.0;

/// 列表列宽（list-detail 左列）。任务区间 320~360，取 4px 栅格内的 **336**：
/// 落在区间中段，既放得下「名字 + 版本 + 尾随图标」，又给详情列多留 16px。
pub const LIST_COLUMN_WIDTH: f32 = 336.0;

/// 详情列下限：再窄就不再分栏。
///
/// 按实测步进算：45 字符 × 6.05px ≈ 272，取 4px 栅格并留余量 → **320**
/// （320 / 6.05 = 53 字符，仍在行宽下限之上）。
pub const DETAIL_MIN_WIDTH: f32 = 320.0;

/// 侧列宽（设置页分组导航）= D33 的 drawer 宽 240。
///
/// 复用 rail 的**尺寸语义**（88 ⇄ 240 同一容器的两种形态），但**不复制 rail 的实现**：
/// 分组导航是页内元素，没有 hover 药丸、没有二级标签。
pub const SIDE_NAV_WIDTH: f32 = LayoutMetrics::DESKTOP.nav_drawer_width;

/// 表单列下限（supporting-pane 主列）：与详情列下限同源（320 ≈ 45 字符）。
pub const FORM_MIN_WIDTH: f32 = DETAIL_MIN_WIDTH;

/// 两列之间的间隙（`Space::X6` = 24）。
pub const PANES_GAP: f32 = Space::X6.dp();

/// 详情里元信息网格的**单卡最小宽度 = 由卡内最长内容反推**：**160**。
///
/// 推导（不是凑出来的）：
/// - 卡内最长值是 demo 里的 `NeoForge 20.4.237`，Segoe UI 14px 实测 **118px**；
/// - 卡两侧内边距 `LayoutMetrics::card_padding` = 20，故最小外框 = 118 + 2×20 = **158**；
/// - 取 4px 栅格的 **160**（等价于旧值，但这次是**内容推出来的**，不是凑上限凑出来的）。
///
/// r2 复审抓到的回归：上一轮为了塞三列把这里收到 136，忘了算 padding ——
/// 卡内只剩 96px，`NeoForge 20.4.237`（118px）与 `NeoForge 21.1.72`（110px）都会折成两行，
/// 同排另两张卡仍一行 → **卡底不齐**。160 之下 3 列要 3×160 + 2×12 = 504，超出行宽上限，
/// 所以元信息网格在 1180 宽是 **2 列**（2×160 + 12 = 332，卡内可排 166px，装得下 118）。
/// 这也是有意的：mod 加载器版本串只会更长，2 列宽卡比 3 列挤卡更抗将来。
pub const META_CARD_MIN_WIDTH: f32 = 160.0;
/// 元信息网格卡片间距（`Space::X3` = 12）。
pub const META_GAP: f32 = Space::X3.dp();

/// 正文平均字符步进（em）——**实测值**，口径见模块文档 §3。
///
/// `BodyMedium` 14px / Segoe UI Regular / 361 字符英文混排正文，FreeType 实测
/// **6.0526px/字符 = 0.43233em**。
///
/// 旧的 `0.5`（`[估]`）把 72 字符的列宽算成 504px，而实测 504px 能塞 **83 字符**——
/// 「72 字符」这个名字和数字对不上的根因就在这里。换字体栈要重新量（这不是令牌）。
pub const CHAR_ADVANCE_EM: f32 = 0.43233;
/// 舒适行宽的字符数上限（任务建议 60~72，取上界 72）。
pub const MAX_MEASURE_CHARS: f32 = 72.0;
/// 行宽下限的字符数（再窄正文就碎成短行，45 是最低可读长度）。
pub const MIN_MEASURE_CHARS: f32 = 45.0;

/// 导航轨宽度（与 `shell::nav` 同一令牌值；布局系统只读它，不重定义）。
pub const fn rail_width() -> f32 {
    LayoutMetrics::DESKTOP.nav_rail_width
}

/// **逐行**安全宽度（px）：折行模拟下「最长一行 ≤72 字符」的最大 4px 栅格列宽 = **424**。
///
/// 见模块文档 §3 的模拟表：428 会出现 73 字符行，426 起降到 71，424 给到 67（留 5 字符余量）。
/// 这个数是**量出来的**，不是 `advance × 72` 算出来的——后者只保证平均。
pub const LINE_SAFE_WIDTH_PX: f32 = 424.0;

/// 平均口径上界（px）：`floor(14px × 0.43233em × 72)` = **435**。
///
/// 只能保证**平均**一行 72 字符（实测 435 下会冒出 73 字符行），所以它是上界而不是取值。
pub fn measure_average_bound_px() -> f32 {
    (TypeRole::BodyMedium.style().size_px * CHAR_ADVANCE_EM * MAX_MEASURE_CHARS).floor()
}

/// 正文列宽上限（px）= `min(逐行安全宽 424, 平均口径上界 435)` = **424**。
///
/// 硬上限，任何版面 / 任何窗口宽度都不许超；`1180` 窗宽下正文列就是它（而非 M68 的 712）。
pub fn measure_cap_px() -> f32 {
    LINE_SAFE_WIDTH_PX.min(measure_average_bound_px())
}

/// 正文列宽下限（px）：`floor(14 × 0.43233 × 45)` = **272**。
pub fn measure_floor_px() -> f32 {
    (TypeRole::BodyMedium.style().size_px * CHAR_ADVANCE_EM * MIN_MEASURE_CHARS).floor()
}

/// 窗口宽度档位（官方 600 / 840 两个断点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthClass {
    /// `< 600`
    Compact,
    /// `600 ≤ w < 840`
    Medium,
    /// `≥ 840`
    Expanded,
}

impl WidthClass {
    /// 按**窗口宽度**（含导航轨，逻辑 px）判档。
    pub fn of(window_width: f32) -> Self {
        if window_width < BREAKPOINT_MEDIUM {
            Self::Compact
        } else if window_width < BREAKPOINT_EXPANDED {
            Self::Medium
        } else {
            Self::Expanded
        }
    }

    /// 稳定名字（日志/测试/文件名用）。
    pub const fn name(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Medium => "medium",
            Self::Expanded => "expanded",
        }
    }

    /// 页边距：compact 16 / medium 24 / expanded 24（官方 expanded 也是 24）。
    pub const fn gutter(self) -> f32 {
        match self {
            Self::Compact => Space::X4.dp(),
            Self::Medium | Self::Expanded => Space::X6.dp(),
        }
    }
}

/// 三种版面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLayout {
    /// 单列（列宽 ≤ 行宽上限，水平居中）。
    SingleColumn,
    /// 主列 + 辅助侧列。
    SupportingPane,
    /// 固定列表列 + 详情列。
    ListDetail,
}

impl PageLayout {
    /// 稳定名字。
    pub const fn name(self) -> &'static str {
        match self {
            Self::SingleColumn => "single-column",
            Self::SupportingPane => "supporting-pane",
            Self::ListDetail => "list-detail",
        }
    }

    /// 是否有两个列（侧列 + 主列）。
    pub const fn is_two_pane(self) -> bool {
        !matches!(self, Self::SingleColumn)
    }
}

/// 页面声明的「最宽版面」——布局系统按窗口宽度降级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKit {
    /// 单列页（下载 / 日志）。
    Single,
    /// 主列 + 侧列（设置）。
    Supporting,
    /// 列表 + 详情（实例）。
    ListDetail,
}

/// 两列版面成立所需的最小窗口宽度（**公式**，不是拍值）：
/// `Σ列宽 + 间隙 + 2×gutter + rail`。
pub fn panes_min_window(first: f32, second: f32) -> f32 {
    first + second + PANES_GAP + 2.0 * WidthClass::Expanded.gutter() + rail_width()
}

/// `ListDetail` 的几何门槛：336 + 320 + 24 + 48 + 88 = **816**。
pub fn list_detail_min_window() -> f32 {
    panes_min_window(LIST_COLUMN_WIDTH, DETAIL_MIN_WIDTH)
}

/// `SupportingPane` 的几何门槛：240 + 320 + 24 + 48 + 88 = **720**。
pub fn supporting_pane_min_window() -> f32 {
    panes_min_window(SIDE_NAV_WIDTH, FORM_MIN_WIDTH)
}

/// 版面决议：档位与几何门槛**同时**满足才升到两列，否则退化单列。
pub fn resolve(kit: PaneKit, window_width: f32) -> PageLayout {
    let class = WidthClass::of(window_width);
    let wide_enough = match kit {
        PaneKit::Single => false,
        PaneKit::Supporting => {
            class != WidthClass::Compact && window_width >= supporting_pane_min_window()
        }
        PaneKit::ListDetail => {
            class == WidthClass::Expanded && window_width >= list_detail_min_window()
        }
    };

    match (kit, wide_enough) {
        (PaneKit::Single, _) | (_, false) => PageLayout::SingleColumn,
        (PaneKit::Supporting, true) => PageLayout::SupportingPane,
        (PaneKit::ListDetail, true) => PageLayout::ListDetail,
    }
}

/// 页面可用宽度：窗口 − 导航轨 − 两侧页边距（下限 0）。
pub fn available_width(window_width: f32) -> f32 {
    let class = WidthClass::of(window_width);
    (window_width - rail_width() - 2.0 * class.gutter()).max(0.0)
}

/// **给定窗口宽度 → 内容（正文）列宽**。本 mission 的「行宽约束」入口。
///
/// - 单列页：就是正文列宽（水平居中摆放）；
/// - 两列页：这是**主列**的正文列宽；实际值还要看 [`frame_metrics`]（详情列会被列表列挤窄）。
///
/// 返回值永远 ≤ [`measure_cap_px`]：1180 窗宽下是 424（逐行 67 字符，≤72），不是 M68 的 712。
pub fn content_column_width(window_width: f32) -> f32 {
    available_width(window_width).min(measure_cap_px())
}

/// 元信息网格的列数：3 列需要 504（超出行宽上限，1180 下用不到），2 列需要 332，否则 1 列。
pub fn meta_columns(width: f32) -> usize {
    for columns in [3usize, 2] {
        let needed = columns as f32 * META_CARD_MIN_WIDTH + (columns - 1) as f32 * META_GAP;
        if width >= needed {
            return columns;
        }
    }
    1
}

/// 一屏的版面几何（页面直接消费，不再自己算列宽）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameMetrics {
    /// 窗口宽度（输入）。
    pub window_width: f32,
    /// 宽度档位。
    pub class: WidthClass,
    /// 决议出来的版面。
    pub layout: PageLayout,
    /// 页边距。
    pub gutter: f32,
    /// 页面可用宽度（已扣导航轨与页边距）。
    pub available: f32,
    /// 正文列宽（≤ 行宽上限）。两列版面时是**主列**的正文列宽。
    pub column: f32,
    /// 第二列（list-detail 的列表列 / supporting-pane 的侧列）。
    pub secondary: Option<f32>,
    /// 主列所在 pane 的宽度（单列 = 正文列宽）。
    pub main_pane: f32,
    /// 详情元信息网格列数。
    pub meta_columns: usize,
}

/// 一次算齐版面几何。
pub fn frame_metrics(kit: PaneKit, window_width: f32) -> FrameMetrics {
    let class = WidthClass::of(window_width);
    let gutter = class.gutter();
    let available = available_width(window_width);
    let layout = resolve(kit, window_width);

    let (column, secondary, main_pane) = match layout {
        PageLayout::SingleColumn => {
            let column = available.min(measure_cap_px());
            (column, None, column)
        }
        PageLayout::ListDetail => {
            let list = LIST_COLUMN_WIDTH.min(available);
            let pane = (available - list - PANES_GAP).max(0.0);
            (pane.min(measure_cap_px()), Some(list), pane)
        }
        PageLayout::SupportingPane => {
            let side = SIDE_NAV_WIDTH.min(available);
            let pane = (available - side - PANES_GAP).max(0.0);
            (pane.min(measure_cap_px()), Some(side), pane)
        }
    };

    FrameMetrics {
        window_width,
        class,
        layout,
        gutter,
        available,
        column,
        secondary,
        main_pane,
        meta_columns: meta_columns(column),
    }
}

/// 版本管理页的**页内**三栏几何（版本类型栏 / 主版本栏 / 列表列）。
///
/// 页面级版面只有三种（[`PageLayout`]），而版本管理页要在单列里再切三栏——那次裁决住在这里，
/// 与 `PaneKit` 分开：`PaneKit` 决定「页面占多宽」，[`rails`] 决定「这宽度怎么切成三栏」。
pub mod rails;

pub use rails::{
    VersionRailTier, VersionRails, MAJOR_RAIL_WIDTH, RAIL_GAP, RAIL_ICON_WIDTH, TYPE_RAIL_WIDTH,
};

#[cfg(test)]
mod tests;
