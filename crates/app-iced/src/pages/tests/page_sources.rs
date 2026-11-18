//! **页面源表**（M338）：单列页的正文必须经 `frame::body` 交出，不许退回 `frame::single_column`
//! （= 424 居中，视觉复审 ① 的老写法）。
//!
//! ## 债：谁参加当年靠手写
//!
//! 这条判据原本在 `pages/tests.rs` 里挂一串 4 条 `include_str!` 元组
//! （`download.rs` / `logs.rs` / `instances.rs` / `instances/detail.rs`）。它抓得住 r2 的变异 G
//! （把页面里的调用点改回 `frame::single_column`），但**只抓表里那 4 个**：新加的页面源文件
//! 不在这张表里 ⇒ 它退回居中 424 也不会有任何测试变红。
//!
//! ## 现在：参加者从目录派生
//!
//! 扫 `pages/**/*.rs`（非测试文件，见 [`scan`]），断言内容一字未改：
//!
//! 1. **不许退回 `frame::single_column`**：目录里任何一个非测试文件出现它就红，
//!    唯一合法出口是设置页（`settings/**`：supporting-pane 退化到单列时用，见 `frame.rs` 的
//!    `pub fn single_column`——它是那条判据的例外，不是遗漏）；
//! 2. **已知的单列正文必须走 `frame::body`**：`download.rs` / `logs.rs` / `instances.rs` /
//!    `instances/detail.rs` 逐个查（原样）。名字从派生结果里取（[`scan::only`]，文件搬走 / 改名
//!    ⇒ panic 红），所以这张名单**不会腐烂**；
//! 3. **空集保护**：扫到的文件数 ≥ 164（M338 实测），走 `frame::body` 的文件数 ≥ 4。
//!
//! **抓得住**：新增 / 搬家后的任何页面源文件退回居中写法（①，比原状更强）；上面四个已知单列正文
//! 任意一处删掉 `frame::body`（②，逐文件）；删到只剩空集（③）。
//!
//! **抓不住**（两条，都写明，不假装）：
//!
//! - **整页装配（`frame::page`）绕过 `frame::body` 的文件**：`account.rs` 与 `content/view.rs`
//!   现在就属于这一类（前者把正文直接交给 `frame::page`，后者自带页内三栏，见 `content.rs` 的
//!   偏离 #2）。它们不在本条断言的射程内：那要一条**新**判据，不是本条的内容。
//! - **新加的单列页**：目录里没有任何字段能说明「这一页的正文是单列正文」（`content.rs` 也是
//!   单列版面但不走 `frame::body`，`settings` 合法用 `single_column`），所以 ② 的名单仍是
//!   人维护的——新页要自己进来加一行。它不是原状那种「静默腐烂」：名单里每一条都有存在性检查，
//!    文件一搬走就 panic；而「有人把这四个里的某个 body 删掉、同时在别处新增一处」这种守恒漏网，
//!    由 ② 的**逐文件**断言直接堵死（不是靠数目）。③ 的下界只负责「扫不到 / 被清空」。

use crate::pages::layout::source;

use super::scan;

/// M338 实测：`pages/**` 下的非测试 `.rs` 有 164 个（`scan::sources_under("")`）。
///
/// 下界而不是等号：加文件不该假红；**清空 / 扫不到**必须红（空集合上的全称断言恒真）。
const KNOWN_PAGE_SOURCES: usize = 164;

/// M338 实测：走 `frame::body` 的页面源文件有 4 个（下面的 [`SINGLE_COLUMN_BODIES`]）。
const KNOWN_BODY_SOURCES: usize = 4;

/// 已知的**单列正文**文件（原 `pages/tests.rs` 那张表的 4 条，一字未改）。
///
/// 这四条不是「参加者的来源」，而是「哪几页的正文必须走 `frame::body`」这个**产品事实**——
/// 目录派生给不出它（见模块文档的「抓不住」第 2 条）。取用一律经 [`scan::only`]：文件搬走就 panic。
const SINGLE_COLUMN_BODIES: [&str; 4] = [
    "download.rs",
    "logs.rs",
    "instances.rs",
    "instances/detail.rs",
];

#[test]
fn single_column_pages_call_the_body_box_not_the_centred_one() {
    let sources = scan::sources_under("");

    // 空集保护：扫到的文件数不许掉到已知数以下（否则下面每一条都在空转）。
    assert!(
        sources.len() >= KNOWN_PAGE_SOURCES,
        "只扫到 {} 个页面源文件（已知至少 {KNOWN_PAGE_SOURCES} 个）：目录派生读不到源码树了，\
         这条判据会变成空转",
        sources.len()
    );

    // ① 目录里**任何**非测试文件都不许退回居中写法（例外只剩定义点与设置页）。
    let centred: Vec<&str> = sources
        .iter()
        .filter(|file| file.text.contains("frame::single_column("))
        .filter(|file| !file.name.starts_with("settings/"))
        .map(|file| file.name.as_str())
        .collect();
    assert!(
        centred.is_empty(),
        "这些文件退回了 `frame::single_column`（= 424 居中，视觉复审 ① 的原样）：{centred:?}\n\
         单列页的正文必须走 `frame::body`（贴左 + 块宽封顶 1440）；只有设置页在 supporting-pane \
         退化成单列时合法保留 `single_column`"
    );

    // ② 已知的单列正文逐个查（断言内容 = 原状那 4 条；名字从派生结果里取 ⇒ 搬家会自己红）。
    for name in SINGLE_COLUMN_BODIES {
        let file = scan::only(&sources, name);
        assert!(
            file.text.contains("frame::body("),
            "{name} 没有调用 frame::body —— 正文块不再从块宽/块偏移建盒"
        );
    }

    // ③ 下界：走 `frame::body` 的文件数不许掉（多一层防「清空 / 扫不到」于空转）。
    let body: Vec<&scan::Source> = sources
        .iter()
        .filter(|file| file.text.contains("frame::body("))
        .collect();
    assert!(
        body.len() >= KNOWN_BODY_SOURCES,
        "只有 {} 个页面源文件走 `frame::body`（已知至少 {KNOWN_BODY_SOURCES} 个）：{:?}\n\
         删掉某页的 `frame::body` 等于让那一页退出本判据——正文块会退回不贴左 / 容器自撑宽",
        body.len(),
        body.iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>()
    );

    // ④ 实例页窄窗那一屏的唯一入口也得走 body（原样保留，文件按名字从派生结果里取）。
    let detail = scan::only(&sources, "instances/detail.rs");
    let narrow = source::function_body(&detail.text, "fn narrow_detail");
    assert!(
        narrow.contains("frame::body("),
        "narrow_detail 没走 frame::body：\n{narrow}"
    );
}

/// **M357 / P1-1 的接线**：提示条的高度必须**灌进页面几何**，且 `body_height` 的 chrome 里
/// 必须有它那一项。
///
/// 为什么是源码断言：这两处是纯接线（`body()` 里的一次 `.with_banner(..)` 与 `chrome` 里的一个
/// `+`），`Element` 没有几何内省，渲染出来也不比基线差——它只在**矮窗**上改变版面决议，
/// 所以没有任何像素判据会红。断掉接线 = 整条 P1-1 静默失效（几何恒 0，与修前逐像素相同）。
///
/// **抓得住**：`.with_banner(model.update.banner_reserve())` 从 `pages/mod.rs` 消失（装配点漏了）、
/// 改挂到别的字段上（字符串对不上）、`body_height` 的 chrome 里 `metrics.banner` 被摘掉。
/// **抓不住**：`banner_reserve` 本身算错（那是 `pages/update/tests.rs` 的公式断言）。
#[test]
fn the_page_assembler_passes_the_banner_into_the_frame_metrics() {
    let sources = scan::sources_under("");
    let assembler = scan::only(&sources, "mod.rs");
    assert!(
        assembler
            .text
            .contains(".with_banner(model.update.banner_reserve())"),
        "pages/mod.rs 的 body() 没把提示条高度灌进 FrameMetrics：P1-1 的整条链就断了\
         （body_height 永远算 0，与修前逐像素相同，没有任何图会红）"
    );

    let frame = scan::source_at("pages/frame.rs");
    assert!(
        frame.text.contains("+ metrics.banner"),
        "frame::body_height 的 chrome 里少了提示条那一项：几何灌进来也没人读"
    );
}
