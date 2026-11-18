//! 未登记占位符的可观测性（M244 PLACEHOLDERWARN）。
//!
//! 判据四条：① 已登记键仍正常替换；② 未登记键原样保留且被收集到；③ 同一 key 只报一次；
//! ④ 大小写/空白变体按「不同的键」处理，但日志行给出笔误提示。
//!
//! 日志落盘那一环（`launcher_log::record`）刻意不在单元测试里断言：它是**进程全局**单例
//! （`launcher_log` 自己的用例要排他地改目录），在并行跑的 lib 测试里抢它是给别人制造
//! flake——实测过一次：本模块的几行把 `launcher_log` 的 `缓冲没有上限：205 行` 顶红了。
//! 所以 [`emit`](super::emit) 在测试构建里是空实现，落盘那一环用一次性探针
//! （`--test-threads=1`，跑完即删）手工验过。这里断言的是它的上游——行文本
//! （[`unknown_log_line`]）、「哪些键算新」（[`collect_fresh`]）、「新键怎么交给 sink」
//! （[`PlaceholderMap::report_with`]，用假 sink）——三段合起来就是「记什么、记几次」。

use super::{collect_fresh, near_miss, unknown_log_line, PlaceholderMap};
use crate::launch::command_builder::CommandBuilder;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn map_with_version() -> PlaceholderMap {
    let mut map = PlaceholderMap::new();
    map.insert("version_name", "1.20.1");
    map.insert("natives_directory", "C:/mc/natives");
    map
}

/// ① 正例：已登记键照旧替换（嵌入串里也一样）。
#[test]
fn registered_keys_are_still_substituted() {
    let map = map_with_version();
    let outcome = map.render_tracked("--version ${version_name} -Dnatives=${natives_directory}");
    assert_eq!(outcome.text, "--version 1.20.1 -Dnatives=C:/mc/natives");
    assert!(outcome.unknown_keys.is_empty(), "{outcome:?}");
    assert_eq!(map.render("a${version_name}b"), "a1.20.1b");
}

/// ② 未登记键原样保留，并且**被收集到**（这条是新增断言：改回空实现就红）。
#[test]
fn unknown_keys_are_kept_verbatim_and_collected() {
    let map = map_with_version();
    let outcome = map.render_tracked("--feature ${third_party_flag} ${version_name}");
    assert_eq!(
        outcome.text, "--feature ${third_party_flag} 1.20.1",
        "未登记键必须原样保留"
    );
    assert_eq!(outcome.unknown_keys, strings(&["third_party_flag"]));
    // 走 `render` 的那条路也会把它记进表里。
    assert_eq!(map.render("${third_party_flag}"), "${third_party_flag}");
    assert_eq!(map.unknown_keys(), strings(&["third_party_flag"]));
}

/// ② 的 argv 侧：组装出来的参数串里 `${}` 一个字节都不变（第三方清单兼容性优先）。
#[test]
fn command_builder_keeps_unknown_placeholders_in_argv() {
    let mut builder = CommandBuilder::with_placeholders(map_with_version());
    builder.add("--version");
    builder.add("${version_name}");
    builder.add("${third_party_flag}");
    assert_eq!(
        builder.args(),
        &strings(&["--version", "1.20.1", "${third_party_flag}"])
    );
    assert_eq!(
        builder.placeholders().unknown_keys(),
        strings(&["third_party_flag"])
    );
}

/// ③ 同一 key 只报一次：一次渲染内去重、跨渲染也只在「新见到」时才写日志。
#[test]
fn the_same_key_is_reported_once() {
    let map = PlaceholderMap::new();
    let outcome = map.render_tracked("${b}${a}${b}${a}");
    assert_eq!(outcome.text, "${b}${a}${b}${a}");
    assert_eq!(outcome.unknown_keys, strings(&["b", "a"]), "保序 + 去重");

    // 表级累计：连着渲染两次，清单里还是这两个。
    let map = PlaceholderMap::new();
    assert_eq!(map.render("${b}${a}"), "${b}${a}");
    assert_eq!(map.render("${a}${b}${c}"), "${a}${b}${c}");
    assert_eq!(map.unknown_keys(), strings(&["b", "a", "c"]));

    // 写日志的那一步：`report` 只把新键交给 `emit`（测试构建里它不碰全局日志）。
    let mut seen = Vec::new();
    assert_eq!(
        collect_fresh(&mut seen, &strings(&["a", "a"])),
        strings(&["a"])
    );
    assert!(collect_fresh(&mut seen, &strings(&["a"])).is_empty());
    assert_eq!(
        collect_fresh(&mut seen, &strings(&["b", "a"])),
        strings(&["b"])
    );
}

/// ④ 大小写/空白变体算**不同的键**（替换是精确匹配，合并会掩盖「这个串没被替换」），
/// 但日志行把近似的那一个点出来，方便定位笔误。
#[test]
fn case_and_whitespace_variants_are_distinct_keys_with_a_hint() {
    let map = map_with_version();
    let outcome = map.render_tracked("${version_name}|${Version_Name}|${version_name }");
    assert_eq!(
        outcome.text, "1.20.1|${Version_Name}|${version_name }",
        "变体没被替换 ⇒ 必须原样保留（不能悄悄合并成已登记键）"
    );
    assert_eq!(
        outcome.unknown_keys,
        strings(&["Version_Name", "version_name "]),
        "变体是各自独立的键"
    );
    assert_eq!(near_miss("Version_Name"), Some("version_name"));
    assert_eq!(near_miss(" version_name "), Some("version_name"));
    assert_eq!(near_miss("third_party_flag"), None);

    let hinted = unknown_log_line("Version_Name");
    assert!(hinted.contains("${Version_Name}"), "{hinted}");
    assert!(hinted.contains("${version_name}"), "{hinted}");
    let plain = unknown_log_line("third_party_flag");
    assert!(plain.contains("${third_party_flag}"), "{plain}");
    assert!(!plain.contains("笔误"), "{plain}");
}

/// 替换语义没被顺手改掉的边角：不闭合的 `${` 原样留、单遍不递归。
#[test]
fn rendering_semantics_are_unchanged() {
    let mut map = PlaceholderMap::new();
    map.insert("a", "${b}");
    map.insert("b", "boom");
    assert_eq!(map.render("${a}"), "${b}", "单遍、不递归");
    assert_eq!(map.render("tail ${"), "tail ${");
    assert_eq!(map.render("no placeholders"), "no placeholders");
    assert_eq!(
        PlaceholderMap::new().render_tracked("${}").unknown_keys,
        strings(&[""]),
        "空键也是未登记键（原样保留）"
    );
}

/// ③ 的「真的交给了日志」这一层：`report_with` 只把**本次新**见到的键递给你的 sink。
///
/// 变异证明：把 `report_with` 里的 `for key in &fresh` 换成 `for key in unknown`（每次渲染都
/// 重复写日志）⇒ 本条红（`left: ["a", "a", …]`）——「一个键只报一次、不刷屏」被守住的地方
/// 就在这里，而不是在 `collect_fresh` 的用例里。
#[test]
fn report_emits_only_fresh_keys() {
    let map = PlaceholderMap::new();
    let mut delivered: Vec<String> = Vec::new();
    map.report_with(&strings(&["a", "a"]), |key| delivered.push(key.to_string()));
    assert_eq!(delivered, strings(&["a"]), "一次渲染内的重复只交一次");
    map.report_with(&strings(&["a", "b"]), |key| delivered.push(key.to_string()));
    assert_eq!(
        delivered,
        strings(&["a", "b"]),
        "跨渲染只有新键才交，且保序"
    );
    assert_eq!(
        map.unknown_keys(),
        strings(&["a", "b"]),
        "表级清单与 sink 同一份"
    );

    // 顺序口径与 `collect_fresh` 一致（同一份规则的两处实现不能漂）。
    let mut fresh_seen = Vec::new();
    assert_eq!(
        collect_fresh(&mut fresh_seen, &strings(&["a", "a"])),
        strings(&["a"])
    );
    assert_eq!(
        collect_fresh(&mut fresh_seen, &strings(&["a", "b"])),
        strings(&["b"])
    );
}

/// 日志是**行导向**的：恶意/畸形清单可以在 JSON 里塞换行，一个键不能变成两行假日志。
#[test]
fn log_lines_never_break_into_two_lines() {
    let line = unknown_log_line("evil\n2026-01-01T00:00:00Z [launch] 伪造行\r");
    assert_eq!(line.lines().count(), 1, "{line}");
    assert!(!line.contains('\n') && !line.contains('\r'), "{line}");
    assert!(
        line.contains("evil 2026-01-01T00:00:00Z [launch] 伪造行"),
        "{line}"
    );
}
