//! 「列表与尾巴面板怎么分高」+ 版本行的整行取整（M125 / M222 D30 / M227 / M238）。
//!
//! 从 `rows.rs` 拆出来的理由只有一条：那边撞 400 行硬线（项目自带
//! `components::gate::every_source_file_stays_under_the_line_limit` 会判红）。本文件是
//! [`TailShare`] 的**纯搬移**，加上 M222 新增的三个只读量（[`ROW_PITCH`] / [`LIST_CHROME`] /
//! [`whole_rows`]）与 M227 的 chips 档预算（[`CHIPS_STACK`] / [`CHIPS_LIST_CHROME`] /
//! [`CHIPS_PANEL_BUDGET`] / [`narrow_rows`]）。
//!
//! M238 把三栏档那个钉死的 `PANEL_BUDGET = 300` 换成**按面板内容推导**的 [`panel_budget`]
//! （首屏 + 钉死区 + 内外边距 + 列表↔面板那层），理由：那个常数不含 [`TAIL_CHROME`]、
//! 也不随版本菜单行数变，760/900 档下滚动视口只剩 54/62px，六枚 chip 被裁成 10px。

use crate::pages::layout::VersionRailTier;

use crate::pages::download::install::PanelState;

/// 列表与尾部面板怎么分这块高度（M125 缺陷 1 的修法；M227 拆出 chips 档一支）。
///
/// | 档位 × 面板形态 | 分法 | 为什么 |
/// |---|---|---|
/// | `Wide` / `Narrow` / `Compact` + 面板**不带**加载器版本列表 | [`TailShare::Pinned`] | 原版面板矮（`04` 实测列表 6 行），整块在屏内 |
/// | 任何档 + 选项区**带加载器版本列表** | [`TailShare::Shared`]：面板先吃 [`panel_budget`]，剩下按整行给列表（[`whole_rows`]） | 面板要「首屏 76 + 钉死区（说明 + 菜单 + 实例名）」才算完整；旧规则（对半分）把列表压到 110px，M238 前那个常数 300 又把滚动视口挤到 54px（六枚 chip 只露 10px）。现在 1180×760 是 **132px = 3 整行**、视口 98px |
/// | `Chips` + 面板**占高度**（选项区展开 / 安装中） | [`TailShare::Narrow`]：面板先吃 [`CHIPS_PANEL_BUDGET`]，剩下按整行给列表（[`narrow_rows`]） | 窄档可用高比三栏档少两行 chip（459 vs 580）⇒ 只钉实例名，head/chips/说明/菜单进滚动区；700/720×760 列表 **132px = 3 整行** |
/// | 任何档 + 只画 `Hint`（没选版本，面板只有一行） | [`TailShare::Pinned`] | 一行提示不值得让面板吃预算：700×760 下 main 是 **7 整行**在屏（不是 8——第 8 条墨迹其实是 `Pick a version, then install` 那句 hint，横向范围对不上版本号；要放 8 行得 352px，实测只剩 309~322px），对半分只剩 **4 整行**、下面留 160px 空白（r1 复审 P2-1，r2 修回 7） |
///
/// 分高方式按「档位 + 面板形态」判；**具体像素**由窗口高度算（`pages/download.rs::view_sized`
/// 把 `model.window_height` 经 `frame::body_height` 传下来，见 [`whole_rows`] / [`narrow_rows`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TailShare {
    /// 面板钉在底部（面板矮的档位）。
    Pinned,
    /// 三栏档 + 带版本列表的选项区：面板先吃预算、列表按整行吃剩下的；钉死区含**说明/标签
    /// + 版本菜单 + 实例名**（D30 四审/八审判定：1180 档高度够，菜单钉住最省事）。
    Shared,
    /// chips 档（可用宽 < 600）+ 面板占高度：同一套「先保预算、列表按整行」，
    /// 但面板钉死区**只留实例名**，head / 六枚 chip / 说明 / 版本菜单全在滚动区
    /// （理由见 [`CHIPS_PANEL_BUDGET`]：459 里装不下「钉住菜单」+「3 整行列表」）。
    Narrow,
}

impl TailShare {
    /// 档位 + **面板形态** → 分高方式（纯函数，单测直接钉它）。
    ///
    /// 三条 arm，**不是**「两个条件同时成立」才共享（M227 重写：旧文档只写 M125 那一条，
    /// 把 arm 2 的「任何档位」漏了 —— r8 复审 improve finding）：
    /// 1. `Chips`（可用宽 < 600，多花两行 chip、竖排预算最紧的那一档）**且**面板真占高度
    ///    （`Options` 选中了版本 / `Install` 进度或失败）⇒ [`TailShare::Narrow`]；
    /// 2. **任何档位**的 `Options` 只要 [`has_loader_versions`](super::super::options::OptionsView::has_loader_versions)
    ///    （选项区带加载器版本列表 ⇒ 钉死区 152~200px，见 [`panel_budget`]）⇒ [`TailShare::Shared`]。三栏档只走这条，
    ///    chips 档被 arm 1 先接走；
    /// 3. 其余 ⇒ [`TailShare::Pinned`]：不带版本列表的 `Options`（原版 / 不兼容 / 空表）与
    ///    `Hint`。
    ///
    /// `Hint`（刚进页面还没选版本）只是**一行**提示（≈30px），让它吃预算会把列表从 **7 整行**
    /// 砍到 **4 整行**、下半块留 ≈160px 空画布（r1 复审 P2-1 实测：main 在 700×760 的
    /// `Hint` 态是 7 整行——不是 8，第 8 条墨迹其实是 `Pick a version, then install` 那句
    /// hint，横向范围对不上版本号；r2 修回 7 整行）。所以 `Hint` **在任何档位都钉死**。
    pub(crate) fn of(tier: VersionRailTier, state: &PanelState<'_>) -> Self {
        match (tier, state) {
            // M227：chips 档（窄档）单独一支 —— 同一份「先保预算」思路，但钉死区只留实例名。
            (VersionRailTier::Chips, PanelState::Options(_) | PanelState::Install(_)) => {
                Self::Narrow
            }
            // M222 D30 r1/r2：**带加载器版本列表**的选项区在任何档位都太高。三栏档修前
            // 面板 204→372px，版本列表被压到 2.4 行（110px）且底缘切在第三行图标上。
            // 现在走 Shared：面板先吃 `panel_budget`（按菜单行数推，M238），列表按**整行**取
            // （`whole_rows`，标定 44/97）⇒ 1180×760 列表 **132px = 3 整行**、面板 330px、
            // 滚动视口 98px（首屏 76 整块在屏）；900 档 264px = 6 整行、视口 106px。
            // 实例名与版本菜单都在面板钉死区（`options::section`）。
            (_, PanelState::Options(options)) if options.has_loader_versions() => Self::Shared,
            _ => Self::Pinned,
        }
    }

    /// 列表视口该拿多少高（`None` = 面板按内容高钉死，列表吃剩下的 `Fill`）。
    ///
    /// `column_height` 两个分支的入参口径一致：都传
    /// [`frame::body_height`](crate::pages::frame::body_height) 的返回值
    /// （chips 档上方那两行 chip 由 [`narrow_rows`] 自己扣，调用点不用管）。
    ///
    /// `menu_items` 只有 `Shared` 档用得上（面板预算按版本菜单的行数推，见 [`panel_budget`]）；
    /// 其余两支忽略它——调用点统一传 [`menu_items`] 的返回值即可。
    pub(crate) fn list_height(self, column_height: f32, menu_items: usize) -> Option<f32> {
        match self {
            Self::Pinned => None,
            Self::Shared => Some(whole_rows(column_height, menu_items)),
            Self::Narrow => Some(narrow_rows(column_height)),
        }
    }
}

/// 版本行的**实测节距**（D30 三审：`04` 图行内下载图标的行首 265/309/353/397/441/485，
/// 节距恰 **44.0**；首行选中高亮矩形 253–296 也是 44px）。
///
/// 列表是 `Fill + scrollable` 时，视口高度不是节距整数倍就会切在某一行中间——D30 r2 实测
/// `01` 是 245px（5.3 行），英文态露出 6px 竖干、中文态露出「预览版」上半截。`Shared` 与
/// `Narrow` 两支都按整行取高，底缘落在行边界上。
pub(crate) const ROW_PITCH: f32 = 44.0;

/// 三栏档「列表以外、列内要扣掉的固定占位」（**列顶 156 → 列表顶 253**，D30 三审实测 = 97
/// = 搜索条 56 + 间距 8 + 分隔线 1 + 间距 8 + 计数头 16 + 间距 8）。
///
/// 1180×760：正文列 580，面板按内容要 [`panel_budget`] ⇒ 列表 **132px = 3 整行**（M238 前是
/// 176px = 4 整行，代价见 [`panel_budget`]）。
/// **改搜索条 / 表头高度时要同步这个数。**
pub(crate) const LIST_CHROME: f32 = 97.0;

/// 面板自身的上下内边距之和（`install::panel_with` 的 `Padding::from([Space::X3, 0])` = 12 + 12）。
pub(crate) const PANEL_PAD_V: f32 = 24.0;

/// 面板滚动区的**首屏**：head 40 + 间距 4 + 六枚 loader chip 32 = **76**。
///
/// 「首屏整块在屏」是本档的硬下限：低于它，chips 就被视口裁成一条缝（M238 实测 736×760
/// 修前只剩 **10px**），而用户看不出「下面还有六枚 chip」——那正是 M227 在 chips 档消灭过的形态。
pub(crate) const FIRST_SCREEN: f32 = 76.0;

/// 选项区里「滚动区 ↔ 钉死区」的间距（`options::section` 的 `Space::X2`）。
pub(crate) const SECTION_GAP: f32 = 8.0;

/// `Shared` 档钉死区里**除版本菜单以外**的固定占位 = 说明/标签 16 + 间距 8 + 间距 8 + 实例名 56。
pub(crate) const PINNED_CHROME: f32 = 88.0;

/// 列表与面板之间那一层固定占位（`rows::content` 的 `tail`：间距 8 + 分隔线 1 + 间距 12）。
pub(crate) const TAIL_CHROME: f32 = 21.0;

/// 正文列高度 → 列表视口的**整行高度**（给三栏档的 `Shared` 分支用）。
///
/// 分高规则是「**面板按内容要够，剩下的按整行给列表**」——[`panel_budget`] 算的就是「够」
/// （首屏 76 + 钉死区 + 面板内外边距），公式本身从 D30 起没变，变的是分子。
///
/// | 正文列 | 菜单行数 | 列表 | 面板 | 滚动视口 |
/// |---|---|---|---|---|
/// | 580（760 窗） | 2 行（≥2 条版本） | **132 = 3 整行** | 330 | **98** |
/// | 580 | 1 行（只有 1 条版本） | 176 = 4 整行 | 286 | 102 |
/// | 720（900 窗） | 2 行 | 264 = 6 整行 | 338 | 106 |
/// | 720 | 1 行 | 308 = 7 整行 | 294 | 110 |
///
/// M238 之前这里钉死 `PANEL_BUDGET = 300`（不含 [`TAIL_CHROME`]）：760/900 档面板实得
/// 286/294px，滚动视口被挤到 **54/62px**，六枚 chip 只露出 10px。预算必须**按内容推**，
/// 不能是一个跟菜单行数无关的常数。
#[must_use]
pub(crate) fn whole_rows(column_height: f32, menu_items: usize) -> f32 {
    let room = (column_height - LIST_CHROME - panel_budget(menu_items)).max(ROW_PITCH);
    (room / ROW_PITCH).floor() * ROW_PITCH
}

/// 三栏档面板「首屏整块 + 钉死区完整」要多少高（含列表↔面板那层 [`TAIL_CHROME`]）。
///
/// = 内边距 24 + 首屏 76 + 间距 8 + 钉死区（88 + 菜单高）+ 列表↔面板 21。
///
/// - 菜单 2 行（`≥2` 条加载器版本，`version_menu_height` 封顶两行）⇒ **329**；
/// - 菜单 1 行（只有一条）⇒ **281**。
///
/// 为什么把 [`TAIL_CHROME`] 算进来：它在**同一列**里吃高度，但不在面板盒子里。M238 前那个
/// 300 没算它，于是「预算 300」实际只换来 **286px** 的面板（300 − 21 那层，再算上整行取整
/// 多出的 7px），比面板需要的 308 少 22px——那 22px 就是从滚动视口里借的。
#[must_use]
pub(crate) fn panel_budget(menu_items: usize) -> f32 {
    PANEL_PAD_V
        + FIRST_SCREEN
        + SECTION_GAP
        + PINNED_CHROME
        + crate::pages::download::options::version_menu_height(menu_items)
        + TAIL_CHROME
}

/// 面板里那张版本菜单有几条（决定菜单高、进而决定 [`panel_budget`]）。
///
/// `0` = 这一态不画菜单（`Hint` / `Install` / 原版 / 不兼容 / 取不到版本表）；`Shared` 档只在
/// `has_loader_versions()` 成立时才走，所以那里恒 ≥ 1。
pub(crate) fn menu_items(state: &PanelState<'_>) -> usize {
    match state {
        PanelState::Options(options) => options.loader_version_count(),
        PanelState::Hint | PanelState::Install(_) => 0,
    }
}

/// chips 档（可用宽 < 600）里 `rows::content` **上方**那一坨的实测高（M227）。
///
/// 分类 tabs 48 + 间距 8 + 主版本 chip 条 48 + 间距 8 + 分隔线 1 + 间距 8 = **121**
/// （720×760 实测：正文列顶 156 → `rows::content` 顶 277）。
/// **改 `rails::stacked_chips` 的高度时同步这个数**（`Element` 没有几何内省，只能实测 + 常量）。
pub(crate) const CHIPS_STACK: f32 = 121.0;

/// chips 档里 `rows::content` 自身的固定占位（M227 实测）。
///
/// 列表之前：搜索条 56 + 间距 8 + 分隔线 1 + 间距 8 + 计数头 17 + 间距 8 = 98
/// （实测内容顶 277 → 列表顶 375）；列表之后还要给尾部：间距 8 + 分隔线 1 + 间距 12 = 21
/// （实测列表底 551 → 分隔线 559 → 面板顶 572）⇒ **119**。
pub(crate) const CHIPS_LIST_CHROME: f32 = 119.0;

/// chips 档里选项区**至少要拿到**的高度（M227 的窄档预算）。
///
/// 窄档的取舍：钉死区只留实例名，其余全进滚动区（`options::section` 的 `Narrow` 一支）。
/// 预算 = 上下内边距 12×2 + 首屏内容 104 + 滚动区↔钉死区间距 8 + 实例名 56 = **192**；
/// 其中首屏 104 = head 40 + 间距 4 + 六枚 chip 32 + 间距 8 + 说明/标签行 20。
///
/// 为什么菜单在窄档**不钉**（三栏档是钉的）：700/720×760 的 `rows::content` 只有 **459**，
/// 若钉死区还要含菜单（说明 20 + 8 + 菜单 64~112 + 8 + 实例名 56 + 内边距 24 = 180~228），
/// 再加首屏 head+chips 76 就是 256~304，列表只剩 ≤1 整行 —— 而 M125 的像素守门
/// （`tests/options_shot/chips.rs` ②）要求 **≥3 整行 = 132**。两者在 459 里互斥，
/// 所以窄档把菜单放进滚动区（滚得到、滚动条可见），钉死区只留实例名。
///
/// 五审实测的坏形态（本预算要消灭的）：滚动区视口被钉死区吃成 **0**（五审记的「24px」其实是
/// 面板内边距 12 + 间距 8 + 说明行顶端），head / 六枚 chip / 说明一个像素都不在屏；
/// 实例名输入框被挤成 **35px**（五审记 36），值墨迹的降部被自己的底边框裁掉。
#[doc(alias = "窄档预算")]
pub(crate) const CHIPS_PANEL_BUDGET: f32 = 192.0;

/// 正文列高度 → chips 档列表视口的**整行高度**（M227）。
///
/// 720×760：580 − 121（[`CHIPS_STACK`]）− 119（[`CHIPS_LIST_CHROME`]）− 192（[`CHIPS_PANEL_BUDGET`]）
/// = 148 ⇒ floor(148/44)×44 = **132px = 3 整行**；面板拿走剩下的 208（滚动区 120 ≥ 首屏 104）。
/// 与 [`whole_rows`] 同一条公式，只是分子换成窄档的三段固定占位。
#[must_use]
pub(crate) fn narrow_rows(column_height: f32) -> f32 {
    let room =
        (column_height - CHIPS_STACK - CHIPS_LIST_CHROME - CHIPS_PANEL_BUDGET).max(ROW_PITCH);
    (room / ROW_PITCH).floor() * ROW_PITCH
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 窄档预算的算术（M227）：760 档恰好 3 整行、面板剩下的高度够首屏 + 实例名。
    ///
    /// 抓得住：把预算改小（列表涨到 4 整行 ⇒ 滚动区 < 首屏 104，说明行被切）或改大
    /// （列表掉到 2 整行 ⇒ M125 的像素守门 ② 红）。
    #[test]
    fn the_chips_budget_keeps_three_whole_rows_at_760() {
        let body = 580.0; // 760 − 标题栏 48 − 页头 84 − 上下页边距 48
        assert_eq!(narrow_rows(body), 132.0, "720×760 的 chips 档列表 = 3 整行");
        let panel = body - CHIPS_STACK - CHIPS_LIST_CHROME - narrow_rows(body);
        assert_eq!(panel, 208.0, "面板拿剩下的那块");
        // 滚动区 = 面板 − 上下内边距 24 − 实例名 56 − 滚动区↔钉死区间距 8
        assert!(
            panel - 24.0 - 56.0 - 8.0 >= 104.0,
            "滚动区 {} < 首屏 104（head 40 + 4 + chips 32 + 8 + 说明 20）",
            panel - 24.0 - 56.0 - 8.0
        );
    }

    /// **M238 的结构判据**：三栏档在 760 / 900 两档正文列下，面板的滚动视口都 ≥ 首屏 76。
    ///
    /// 视口 = 面板高 − [`PANEL_PAD_V`] − [`SECTION_GAP`] − 钉死区（[`PINNED_CHROME`] + 菜单高），
    /// 面板高 = 正文列 − [`LIST_CHROME`] − 列表 − [`TAIL_CHROME`]。这条算术就是渲染层真正的
    /// 分高链路（`rails::three_column` → `rows::content` → `install::panel_with` → `options::section`），
    /// 每个加数都取自生产常量，不是测试里抄的副本。
    ///
    /// 抓得住：把 `panel_budget` 退回「死的 300」（= M238 前的形态）⇒ 760 档视口 54、900 档 62，
    /// 两条都红；把预算改成不含 [`TAIL_CHROME`] ⇒ 两者再各少 21；把 [`FIRST_SCREEN`] 调小
    /// （比如有人以为首屏只需 head + chips = 72）不明显红，但那属于口径调整，不属于回归。
    /// 抓不住：像素上 chips 到底露出几行（那要真图，见 `tests/options_shot/compact.rs`）；
    /// 也不管 chips 档（`narrow_rows` 那一支另有它的钉子）。
    #[test]
    fn the_three_column_panel_keeps_its_first_screen_at_760_and_900() {
        let pinned = |items: usize| {
            PINNED_CHROME + crate::pages::download::options::version_menu_height(items)
        };
        let viewport = |body: f32, items: usize| {
            let list = whole_rows(body, items);
            let panel = body - LIST_CHROME - list - TAIL_CHROME;
            panel - PANEL_PAD_V - SECTION_GAP - pinned(items)
        };

        // 硬判据先跑：**滚动视口 ≥ 首屏**（这是这条测试存在的理由，先红先看）
        for (body, items) in [(580.0, 3), (580.0, 1), (720.0, 3), (720.0, 1)] {
            assert!(
                viewport(body, items) >= FIRST_SCREEN,
                "正文列 {body} / 菜单 {items} 条：滚动视口只有 {}px < 首屏 {FIRST_SCREEN}px ⇒ \
                 六枚 chip 会被裁成一条缝（M238 修前 760 档实测 54px / chips 只露 10px）",
                viewport(body, items)
            );
        }

        // 紧接着钉住具体像素：它们让「视口够」不是靠偶然的取整余量。
        // 760 窗（正文列 580）：2 行菜单 = 3 整行列表；1 行菜单可以把第 4 行留下。
        assert_eq!(
            whole_rows(580.0, 3),
            132.0,
            "≥2 条版本 ⇒ 2 行菜单 ⇒ 3 整行列表"
        );
        assert_eq!(
            whole_rows(580.0, 1),
            176.0,
            "只有 1 条版本 ⇒ 菜单 1 行 ⇒ 4 整行列表"
        );
        assert_eq!(viewport(580.0, 3), 98.0);
        assert_eq!(viewport(580.0, 1), 102.0);

        // 900 窗（正文列 720）
        assert_eq!(whole_rows(720.0, 3), 264.0);
        assert_eq!(whole_rows(720.0, 1), 308.0);
        assert_eq!(viewport(720.0, 3), 106.0);
        assert_eq!(viewport(720.0, 1), 110.0);
    }

    /// 三栏档的预算**跟着菜单行数走**（M238）：菜单多一行，预算多 48，列表少一行。
    ///
    /// 抓得住：`whole_rows` 的 `menu_items` 参数被忽略（两档预算相等）；`panel_budget` 漏掉
    /// [`TAIL_CHROME`]（329 → 308）；`PINNED_CHROME` 写小（那会让视口虚高、真图露馅）。
    #[test]
    fn the_three_column_budget_follows_the_version_menu_height() {
        assert_eq!(panel_budget(1), 281.0, "24 + 76 + 8 + 88 + 64 + 21");
        assert_eq!(panel_budget(2), 329.0, "菜单封顶两行 ⇒ 112");
        assert_eq!(panel_budget(3), panel_budget(2), "3 条以上也还是两行");
        assert_eq!(panel_budget(2) - panel_budget(1), 48.0, "一行的差 = 项高");
    }

    /// `list_height` 把三档的「列表吃多少」收敛到一处（`Pinned` = 不给定高）。
    #[test]
    fn list_height_follows_the_share() {
        assert_eq!(TailShare::Pinned.list_height(580.0, 0), None);
        assert_eq!(TailShare::Shared.list_height(580.0, 3), Some(132.0));
        assert_eq!(TailShare::Narrow.list_height(580.0, 0), Some(132.0));
    }
}
