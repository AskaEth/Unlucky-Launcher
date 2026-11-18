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
//! - 抓得住：删掉 / 注释掉某个调用点的 `.base(..)`、或把整个文件的接线回退（数字掉一即红）；
//!   在**没登记过的文件**里新写一处接线（M338 起：目录里每个文件都要对上账）。
//! - 抓不住：① 传**错的**底（换成 `Base::CANVAS` 数字不变，只有像素判据能抓）；
//!   ② 把接线搬进辅助函数（那是等价重构，改这里的数字并在提交信息里说明搬去了哪）；
//!   ③ 新加的控件（新调用点没人接线时数字不会变——这条只守**已经签过的**那几处）。
//!
//! ## 数字怎么来的
//!
//! 每个文件里 `.base(` 的出现次数（非注释行，`grep -o "\.base(" <file> | wc -l` 的口径），
//! 三个手写样式的文件数是 `BASE.role()`（它们不走组件的 `.base(..)`，而是把状态层直接叠在
//! `frame::BASE` 上）。
//!
//! ## M338：参加者从目录派生（不再手写）
//!
//! 原状：这张表要写**两遍**文件清单——一遍是 [`WIRED`] / [`ROLE_SITES`]（名字 + 次数），
//! 一遍是 `sources` 那 13 条 `include_str!`（名字 + 全文），两边靠人肉对齐，
//! 漏一处就是「清单与文件对不上」的 panic 或者一条静默放行。
//!
//! 现在：全文从 [`scan::sources_under`] 派生（扫 `pages/**/*.rs` 的非测试文件），
//! **没登记的文件期望 0 次**——于是「谁在没人管的文件里接线」立刻红。
//! M338 第一次跑就翻出 3 处从没登记过的真接线（`instances/saves/rows.rs` 2 处 `.base(..)`、
//! `settings/frame.rs` 与 `appearance/section/mod.rs` 各 1 处 `BASE.role()`），已按实测数登记——
//! 这正是手写清单年代「表里没有 = 无声通过」的那条债。

use super::scan;

/// `(文件, 期望的接线次数)`：文件路径相对 `pages/`。
///
/// M312：MODS 区的画法从 `instances/mods.rs` 搬到了 `instances/mods/rows.rs`（那边加完
/// 重复 / 前置会越过 400 行硬线）。接线**跟着搬**：`mods.rs`（模型层）1 → 0，
/// `mods/rows.rs` 新增 2（搬来的那一处开关 + 新增的「去模组库找」文字按钮）。
/// 只把旧的一减、不给新文件记一笔，就等于允许「接线搬走后谁都没接手」。
///
/// M338：`instances/saves/rows.rs` 补登记 2 —— 它是目录派生翻出来的（改名前这份表里没有它，
/// 那边的行内输入框与「重命名」按钮一直在接线，却从来没被数过）。
const WIRED: [(&str, usize); 11] = [
    ("instances.rs", 1),
    ("instances/mods.rs", 0),
    ("instances/mods/rows.rs", 2),
    ("instances/saves.rs", 2),
    ("instances/saves/rows.rs", 2),
    ("instances/launch_args.rs", 5),
    ("logs/diagnostic.rs", 1),
    ("settings/java/view.rs", 4),
    ("settings/java/fields.rs", 7),
    ("settings/java/prefs_view.rs", 8),
    ("settings/java/download_view.rs", 5),
];

/// 手写样式（不走组件 `.base(..)`）的位置：状态层直接叠在 `frame::BASE.role()` 上。
///
/// M338：`settings/frame.rs` 与 `appearance/section/mod.rs` 同样是目录派生翻出来的。
const ROLE_SITES: [(&str, usize); 5] = [
    ("settings/general/language.rs", 1),
    ("settings/general/theme.rs", 1),
    ("settings/logs_row.rs", 1),
    ("settings/frame.rs", 1),
    ("appearance/section/mod.rs", 1),
];

/// M338 实测：`pages/**` 下的非测试 `.rs` 有 164 个（`scan::sources_under("")` 数出来的）。
///
/// 下界而不是等号：加文件不该假红；**清空 / 扫不到**必须红（空集合上的全称断言恒真）。
const KNOWN_PAGE_SOURCES: usize = 164;

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
    let sources = scan::sources_under("");
    assert!(
        sources.len() >= KNOWN_PAGE_SOURCES,
        "只扫到 {} 个页面源文件（已知至少 {KNOWN_PAGE_SOURCES} 个）：目录派生读不到源码树了，\
         这条判据会变成空转",
        sources.len()
    );

    // ① 目录里**每个**文件都要对上账：登记过的按登记的数目，没登记的必须是 0 次。
    //
    // 原状：只有 `sources` 那 13 条清单被读进来 ⇒ 没人登记的文件里接线、或者接错，一声不响。
    // 现在：新文件里出现一处 `.base(..)` 就红（「谁接线了没登记」），
    // 登记过的文件少一处也红（「接线掉了」）。
    for file in &sources {
        let registered = WIRED.iter().find(|(name, _)| *name == file.name);
        let got = count(&file.text, ".base(");

        match registered {
            Some((name, want)) => assert_eq!(
                got, *want,
                "{name}: `.base(..)` 出现 {got} 次，期望 {want} 次 —— 卡片内的控件必须把 \
                 `frame::BASE` 传下去（`pages/frame/pieces.rs` 的 `BASE`）。\
                 如果你是把接线搬进了辅助函数（等价重构），把这里的数字跟着改并说明搬去了哪。"
            ),
            None => assert_eq!(
                got, 0,
                "{}: 这个文件里出现 {got} 次 `.base(..)`，但接线表（本文件的 `WIRED`）里没有它 —— \
                 目录派生之后「新文件」默认期望 0 次接线。确实接对了就登记它的次数并写明这是哪一处的底；\
                 接错了就把 `frame::BASE` 换成卡片底。",
                file.name
            ),
        }
    }

    // ② 登记过的文件**必须还在**：搬走 / 改名而忘了回来改这张表 ⇒ 红（`scan::only` 自带人话）。
    //    这一条是清单时代的「清单与文件对不上」panic 的原样保留。
    for (name, _) in WIRED {
        scan::only(&sources, name);
    }

    // ③ 手写样式（`BASE.role()`）同理。
    for file in &sources {
        let registered = ROLE_SITES.iter().find(|(name, _)| *name == file.name);
        let got = count(&file.text, "BASE.role()");

        match registered {
            Some((name, want)) => assert_eq!(
                got, *want,
                "{name}: `BASE.role()` 出现 {got} 次，期望 {want} 次 —— 手写按钮的 hover / 禁用\
                 状态层必须叠在卡片底上（画布底会让它在卡里偏色，M237）"
            ),
            None => assert_eq!(
                got, 0,
                "{}: 这个文件里出现 {got} 次 `BASE.role()`，但接线表（本文件的 `ROLE_SITES`）里没有它 —— \
                 手写状态层叠在卡片底上就该登记；叠错了就换成卡片底。",
                file.name
            ),
        }
    }

    for (name, _) in ROLE_SITES {
        scan::only(&sources, name);
    }
}
