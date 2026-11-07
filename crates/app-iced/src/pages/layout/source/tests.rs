//! `source` 自己的断言：切得准、切得住（门禁的门禁）。
//!
//! 口径与 `tests/no_hardcoded_tokens.rs::the_gate_itself_catches_a_planted_violation` 一致：
//! 护栏工具自己先被一条断言钉住，否则它退化时没人发现。

use super::*;

const SAMPLE: &str = "\
/// doc
pub fn alpha() -> u8 {
    let inner = |x: u8| {
        x
    };
    inner(1)
}

pub fn beta<'a>(value: &'a str) -> &'a str {
    value
}
";

#[test]
fn it_stops_at_the_top_level_closing_brace() {
    let alpha = function_body(SAMPLE, "pub fn alpha");
    assert!(alpha.starts_with("pub fn alpha"));
    assert!(alpha.trim_end().ends_with('}'), "{alpha}");
    // 内层闭包的花括号不会提前截断：`inner(1)` 与它的 `};` 都在
    assert!(alpha.contains("inner(1)"), "{alpha}");
    assert!(!alpha.contains("pub fn beta"), "切到了下一个函数：{alpha}");

    let beta = function_body(SAMPLE, "pub fn beta<'a>(");
    assert!(beta.contains("value"), "{beta}");
    assert!(!beta.contains("inner("), "{beta}");
}

#[test]
fn the_helper_also_works_on_signatures_with_generics_and_multiline_params() {
    let source = "pub fn gamma<'a>(\n    metrics: FrameMetrics,\n    content: u8,\n) -> u8 {\n    content\n}\n";
    let gamma = function_body(source, "pub fn gamma");
    assert!(gamma.contains("metrics: FrameMetrics"), "{gamma}");
    assert!(gamma.trim_end().ends_with('}'), "{gamma}");
}

/// `statement` 停在第一个 `;`：构建链断言靠它精确到一个容器。
#[test]
fn a_statement_stops_at_the_first_semicolon() {
    let source = "\
let mut rows = column(entries)
    .padding(Padding::from([0.0, ui.space(Space::X2)]))
    .width(Length::Fill);
let other = 1;
";
    let chain = statement(source, "let mut rows = column(");
    assert!(chain.starts_with("let mut rows"), "{chain}");
    assert!(chain.ends_with(".width(Length::Fill);"), "{chain}");
    assert!(!chain.contains("let other"), "越过语句边界：{chain}");
}

/// `function_region` 把**本文件内被调用的函数**也算进契约（等价重构不假红）。
#[test]
fn the_region_follows_one_level_of_delegation() {
    let source = "\
pub fn outer(a: u8) -> u8 {
    inner(a) + 1
}

fn inner(a: u8) -> u8 {
    a * 2
}

fn unrelated(a: u8) -> u8 {
    a - 9
}
";
    let region = function_region(source, "pub fn outer(");
    assert!(region.contains("inner(a) + 1"), "{region}");
    assert!(region.contains("a * 2"), "转调目标没进来：{region}");
    assert!(!region.contains("a - 9"), "把无关函数也扫进来了：{region}");
}

/// 转调目标带**泛型签名**时也要跟进去（`frame::body` → `body_impl<'a>` 就是这种形状）。
///
/// 这条是变异 H1 抓出来的：早先的实现拿 `fn {name}(` 当定义 needle，遇到
/// `fn body_impl<'a>(` 找不到 ⇒ 护栏自己 panic ⇒ 对等价重构**假红**。
#[test]
fn the_region_follows_delegation_into_a_generic_function() {
    let source = "\
pub fn wrapper(v: u8) -> u8 {
    helper(v) + 1
}

fn helper<'a>(v: u8) -> u8 {
    v * 3
}
";
    let region = function_region(source, "pub fn wrapper(");
    assert!(region.contains("v * 3"), "泛型签名没跟进去：{region}");
}

#[test]
#[should_panic(expected = "找不到签名")]
fn a_missing_signature_fails_loudly() {
    let _ = function_body(SAMPLE, "pub fn nope");
}

/// 跨文件转调也要跟进去（`frame::body` → `layout::block::body_box` 就是这种形状）。
///
/// 变异 M6（把盒子原样搬进 `layout/block.rs`）在单文件版下假红；多文件版应当放行。
#[test]
fn the_region_follows_delegation_across_files() {
    let frame = "\
pub fn body(a: u8) -> u8 {
    block::body_box(a) + 1
}
";
    let block = "\
pub fn body_box(a: u8) -> u8 {
    a * 7
}
";
    let region = region_across(&[frame, block], "pub fn body(");
    assert!(region.contains("body_box(a) + 1"), "{region}");
    assert!(region.contains("a * 7"), "跨文件转调没跟进去：{region}");
}

/// `call_arguments` 取到的是**实参原文**（嵌套括号完整），这是 M4 那条护栏的判据基础。
#[test]
fn call_arguments_returns_the_nested_argument_text() {
    let source = "\
rows = rows.spacing(ui.space(Space::X1));
rows = rows.spacing(ui.space(step));
column![head, scroller].spacing(ui.space(Space::X2));
";
    let args = call_arguments(source, "rows.spacing(");
    assert_eq!(args, vec!["ui.space(Space::X1)", "ui.space(step)"]);
    assert!(
        args[0].contains("Space::") && !args[1].contains("Space::"),
        "实参文本要能区分「写死令牌」与「经变量中转」：{args:?}"
    );
}

/// `derived_from` 顺绑定链追源头：真源放行、中转一层写死令牌判否（变异 M4 的判据）。
#[test]
fn binding_tracing_separates_the_truth_source_from_a_smuggled_token() {
    let honest = "\
    if let Some(spacing) = ROW_SPACING {
        rows = rows.spacing(ui.space(spacing));
    }
";
    assert!(derived_from(honest, "spacing", "ROW_SPACING"));

    // 经局部变量中转一次（变异 H3 的等价重构形状）：仍要追到 ROW_SPACING
    let via_local = "\
    let spacing = ROW_SPACING;
    if let Some(step) = spacing {
        rows = rows.spacing(ui.space(step));
    }
";
    assert!(derived_from(via_local, "step", "ROW_SPACING"));

    // 把写死令牌挪到另一条 let 再传（变异 M4）：追不到 ROW_SPACING
    let smuggled = "\
    if let Some(spacing) = ROW_SPACING {
        rows = rows.spacing(ui.space(spacing));
    }

    let extra = Space::X1;
    rows = rows.spacing(ui.space(extra));
";
    assert!(!derived_from(smuggled, "extra", "ROW_SPACING"));
    assert!(derived_from(smuggled, "spacing", "ROW_SPACING"));
}

#[test]
#[should_panic(expected = "找不到语句起点")]
fn a_missing_statement_start_fails_loudly() {
    let _ = statement(SAMPLE, "let nope = 1");
}
