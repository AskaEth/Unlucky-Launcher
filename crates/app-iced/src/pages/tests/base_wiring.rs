//! 页面层**接线**守门：卡片里的控件有没有把 `frame::BASE` 传下去（M237）。
//!
//! ## 为什么还要一条「数接线点」的判据
//!
//! 像素判据（`pages/settings/tests/java_shot.rs`）只落在**一屏能拍到的**控件上。实例详情页的
//! MODS / SAVES 两张卡在现有夹具下拍不到禁用控件：`ULL_MODS_FIXTURE` 把 `running` 写死成
//! `false`（开关全是启用态），SAVES 的动作行在 1180×760 的折叠线以下。那几处接线于是**没有
//! 任何渲染证据**能守——有人删掉一个 `.base(..)`，图和一整张像素判据都不会变。
//!
//! 这里退一步，只数「每个文件里出现了几次接线调用」。它证明的是**接线还在**，不是「画对了」；
//! 真正的「叠的是哪一层底」由像素判据与 `interaction::disabled::tests` 两层负责。
//!
//! 清单里有两种接线：**禁用态**（`Button::disabled` / `Switch::disabled` / `Chip::disabled`）与
//! **hover / 选中状态层**（`Text`·`Outlined` 按钮、`ListItem` 行、手写 chip 的 `state_over`）。
//! 像素判据只量到禁用态那一档（`pages/settings/tests/java_shot.rs` 的 Java 卡），
//! 所以 `instances.rs`（列表行，复审 P2-1）这种**只有状态层**的接线只能靠这份计数守。
//!
//! ## 抓得住 / 抓不住
//!
//! - 抓得住：删掉 / 注释掉某个调用点的 `.base(..)`、或把整个文件的接线回退（数字掉一即红）。
//! - 抓不住：① 传**错的**底（换成 `Base::CANVAS` 数字不变，只有像素判据能抓）；
//!   ② 把接线搬进辅助函数（那是等价重构，改这里的数字并在提交信息里说明搬去了哪）；
//!   ③ 新加的控件（新调用点没人接线时数字不会变——这条只守**已经签过的**那几处）。
//!
//! ## 数字怎么来的
//!
//! 每个文件里 `.base(` 的出现次数（`grep -o "\.base(" <file> | wc -l`），三个手写样式的文件
//! 数是 `BASE.role()`（它们不走组件的 `.base(..)`，而是把状态层直接叠在 `frame::BASE` 上）。

/// `(文件, 期望的接线次数)`：文件路径相对 `pages/`。
const WIRED: [(&str, usize); 9] = [
    ("instances.rs", 1),
    ("instances/mods.rs", 1),
    ("instances/saves.rs", 2),
    ("instances/launch_args.rs", 5),
    ("logs/diagnostic.rs", 1),
    ("settings/java/view.rs", 4),
    ("settings/java/fields.rs", 7),
    ("settings/java/prefs_view.rs", 8),
    ("settings/java/download_view.rs", 5),
];

/// 手写样式（不走组件 `.base(..)`）的三处：状态层直接叠在 `frame::BASE.role()` 上。
const ROLE_SITES: [(&str, usize); 3] = [
    ("settings/general.rs", 1),
    ("settings/general/theme.rs", 1),
    ("settings/logs_row.rs", 1),
];

/// 数**非注释行**里 `needle` 的出现次数。
///
/// 整行注释（`//` / `///` / `//!`）先剔掉：**把接线注释掉不算接线**，而「注释掉一行」恰恰是
/// 最容易发生的回退。复审 r2 实测过：只按 `source.matches()` 数，注释掉
/// `item.base(frame::BASE)` 之后本判据仍是绿的（注释里还留着 `.base(` 这四个字符）。
fn count(haystack: &str, needle: &str) -> usize {
    haystack
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .map(|line| line.matches(needle).count())
        .sum()
}

#[test]
fn every_card_internal_control_site_still_passes_a_base() {
    let sources: [(&str, &str); 12] = [
        ("instances.rs", include_str!("../instances.rs")),
        ("instances/mods.rs", include_str!("../instances/mods.rs")),
        ("instances/saves.rs", include_str!("../instances/saves.rs")),
        (
            "instances/launch_args.rs",
            include_str!("../instances/launch_args.rs"),
        ),
        ("logs/diagnostic.rs", include_str!("../logs/diagnostic.rs")),
        (
            "settings/java/view.rs",
            include_str!("../settings/java/view.rs"),
        ),
        (
            "settings/java/fields.rs",
            include_str!("../settings/java/fields.rs"),
        ),
        (
            "settings/java/prefs_view.rs",
            include_str!("../settings/java/prefs_view.rs"),
        ),
        (
            "settings/java/download_view.rs",
            include_str!("../settings/java/download_view.rs"),
        ),
        (
            "settings/general.rs",
            include_str!("../settings/general.rs"),
        ),
        (
            "settings/general/theme.rs",
            include_str!("../settings/general/theme.rs"),
        ),
        (
            "settings/logs_row.rs",
            include_str!("../settings/logs_row.rs"),
        ),
    ];

    for (name, want) in WIRED {
        let source = sources
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, source)| *source)
            .unwrap_or_else(|| panic!("{name} 不在本判据的清单里（清单与文件对不上）"));
        let got = count(source, ".base(");
        assert_eq!(
            got, want,
            "{name}: `.base(..)` 出现 {got} 次，期望 {want} 次 —— 卡片内的控件必须把 \
             `frame::BASE` 传下去（`pages/frame/pieces.rs` 的 `BASE`）。\
             如果你是把接线搬进了辅助函数（等价重构），把这里的数字跟着改并说明搬去了哪。"
        );
    }

    for (name, want) in ROLE_SITES {
        let source = sources
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, source)| *source)
            .unwrap_or_else(|| panic!("{name} 不在本判据的清单里（清单与文件对不上）"));
        let got = count(source, "BASE.role()");
        assert_eq!(
            got, want,
            "{name}: `BASE.role()` 出现 {got} 次，期望 {want} 次 —— 手写按钮的 hover / 禁用\
             状态层必须叠在卡片底上（画布底会让它在卡里偏色，M237）"
        );
    }
}
