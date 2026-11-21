//! 门禁：**选中项的底色只有一个入口**（M299）。
//!
//! ## 守的是什么
//!
//! MD3 的「当前项」在壳里出现十次（菜单项 / 列表行 / 导航项药丸 / 导航轨 / 头像位 / 语言档 /
//! 主题档 / 外观档位 / 分组导航 / 版本选择行）。它们守的是**同一条**行为：底是实色
//! `SecondaryContainer`，状态层叠在它上面。M296 把这条收成一个函数
//! [`selected_fill`](crate::app::components::interaction::selected_fill)，M299 把最后五份私有拷贝
//! （语言档 / 主题档 / 外观档位 / 选中 chip / 外壳药丸 + 头像）也接了上去。
//!
//! 为什么必须有这条门禁：**行为测试抓不住「抄了一份正确的」**。把 `layer_color(..).or_else(||
//! 实色)` 在某个控件里重写一遍，逐值断言全绿（值相同），而下一个 α 改动只会动到其中一半——
//! M296 的变异实验就是这么发现外壳那两处的（改助手 ⇒ 外壳药丸不动）。所以这里量的是**接线**，
//! 不是值。
//!
//! ## 三条判据
//!
//! 1. **每个站点都必须出现 `selected_fill(`**（正例）：删掉调用 = 退回私有拷贝，红；
//! 2. **产品代码里不许出现旧私有拷贝的两种形态**（反例）：`.or_else(|| Some(ui.color(..))` 与
//!    `.unwrap_or_else(|| ui.color(..))`。比对前把空白全部删掉，所以换行 / 缩进 / 加括号都躲不过
//!    （M299 r1 P2-2：上一版是整串字面量，只换排版就能绕过）；
//! 3. **[`KNOWN_COPIES`] 里的豁免必须还活着**（豁免不许挂空条目）。
//!
//! ## 抓得住 / 抓不住（据实写）
//!
//! 抓得住：①某个站点被改成 `Some(ui.color(Role::SecondaryContainer))`（用户点名的那条缺陷）——
//! 反例判据与正例判据至少一条红；②名单外的第十二个站点直接抄私有拷贝（含换行 / 加括号的写法）
//! ——反例判据红（M299 r1 实测过）；③`selected_fill` 被改名 / 移走——全部正例红；
//! ④有人把某处收口了却忘了删豁免——判据 3 红。
//! 抓不住：①**语义上正确但形态不同**的私有拷贝（比如 `match layer_color(..) { Some(c) => c,
//! None => ui.color(..) }`，或先 `let Some(c) = .. else { return ui.color(..) }`）——它不是
//! `or_else` / `unwrap_or_else` 形态，只能靠人读 diff；
//! ②像素（那是 `selection/probe.rs` 与 `DevDocs/artifacts/m299/` 的图）；
//! ③站点内部把 `selected_fill` 的返回值丢了不用（正例只证明「调了」）；
//! ④字符串字面量里恰好含 `//` 的行会被截断（判定只会更松，不会误报）。
//!
//! 这几条盲区写在这里，是因为「门禁绿」最容易被读成「这事有人管」。

use std::path::{Path, PathBuf};

/// 本 crate 的 `src/`。
fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// 递归收集 `.rs` 文件。
fn rust_files(dir: &Path, collected: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("读不动 {}：{error}", dir.display()));

    for entry in entries {
        let path = entry.expect("目录项读失败").path();
        if path.is_dir() {
            rust_files(&path, collected);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            collected.push(path);
        }
    }
}

/// 产品代码（剥掉 `#[cfg(test)]` 项与注释行）：测试里写角色色是**应该的**。
///
/// 为什么不能「从第一个 `#[cfg(test)]` 截到文件尾」：`appearance/section/mod.rs` 在正文中间就有
/// `#[cfg(test)] super::probe::record(..)` 这种取证钩子，截断会把后面的 `choice_style` 整段吃掉
/// （M299 实测：门禁把已经改好的文件判成「没有 selected_fill」）。所以这里数花括号跳**那一个 item**。
fn product_part(text: &str) -> String {
    let mut out = String::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if !line.trim_start().starts_with("#[cfg(test)]") {
            // 注释不算代码：文档里解释「旧写法长什么样」不该被判成旧写法。整行注释与行尾注释
            // 都剥掉（`//` 之后的半行同样不算；字符串里恰好有 `//` 只会让判定更松，不会误报）。
            let code = line.split("//").next().unwrap_or_default();
            out.push_str(code);
            out.push('\n');
            continue;
        }
        // 跳过这个 item：`#[cfg(test)] mod tests;` 是一行声明，带花括号的则数到配平为止。
        let mut depth: i32 = 0;
        let mut opened = false;
        let mut current = line.to_owned();
        loop {
            for ch in current.chars() {
                match ch {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if opened && depth <= 0 {
                break;
            }
            if !opened && current.trim_end().ends_with(';') {
                break;
            }
            match lines.next() {
                Some(next) => current = next.to_owned(),
                None => break,
            }
        }
    }
    out
}

/// 相对 `src/` 的路径 → 源码文本（产品部分）。
fn product_source(relative: &str) -> String {
    let path = src_dir().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不动 {}：{error}", path.display()));
    product_part(&text)
}

/// 旧私有拷贝的**两种**形态（去掉空白后比对：换行 / 缩进 / 括号排版都躲不过）。
///
/// 第一种是「状态层有就给、没有就兜底实色」，第二种是 `selected_fill` 自己的函数体写法
/// （`unwrap_or_else(|| ui.color(container))`）——两者都是「选中项的底」那份拷贝。
const PRIVATE_COPY_NEEDLES: [&str; 2] =
    [".or_else(||Some(ui.color(", ".unwrap_or_else(||ui.color("];

/// 把源码归一成「只留标识符与标点」的形态：删空白，**再删闭包的花括号**。
///
/// 于是下面三种写法归一成同一个字面量：
///
/// ```text
/// .or_else(|| Some(ui.color(..)))
/// .or_else(||    Some(ui.color(..))
/// .or_else(|| { Some(ui.color(..)) })
/// ```
///
/// 删花括号是必要的：M299 r1 P2-2 的绕过写法正是 `or_else(|| { Some(..) })`——只删空白时那个
/// `{` 会把针挡掉。针本身以 `||` 打头并紧贴 `Some(`/`ui.color(`，所以「中间隔着别的语句」不会误命中
/// （`;let y=` 之类的标记仍在）。
fn squeeze(text: &str) -> String {
    text.split_whitespace()
        .collect::<String>()
        .replace(['{', '}'], "")
}

/// 命中哪一个针（没有就 `None`）。
fn private_copy(text: &str) -> Option<&'static str> {
    let squeezed = squeeze(text);
    PRIVATE_COPY_NEEDLES
        .iter()
        .copied()
        .find(|needle| squeezed.contains(needle))
}

/// 这个文件是不是**只**在测试里编译。
///
/// 两种形态：`.../tests/xxx.rs`（父模块 `#[cfg(test)] mod tests;` 声明的目录）与 `xxx/tests.rs`
/// （同名子模块文件）。它们整体是测试代码——扫它们只会把测试助手（比如 `java/marker.rs` 里
/// 「容器为空时按 surface 算对比度」的兜底）当成私有拷贝。
fn is_test_only(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "tests.rs")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

/// 十个「当前项」站点，全部必须经 [`selected_fill`] 拿底。
///
/// 前四行是 M296 收口的，后七行是 M299 收口的（`shell/nav.rs` 与 `shell/nav/account.rs` 各一处）。
const SITES: [&str; 11] = [
    "app/adopt/pick.rs",
    "components/inputs/menu.rs",
    "components/navigation/drawer/item.rs",
    "components/navigation/list_item.rs",
    "components/selection/paint/chip.rs",
    "pages/appearance/section/mod.rs",
    "pages/settings/frame.rs",
    "pages/settings/general/language.rs",
    "pages/settings/general/theme.rs",
    "shell/nav/account.rs",
    "shell/nav.rs",
];

/// 判据 1（正例）：每个站点都调那个唯一入口。
#[test]
fn every_selected_item_goes_through_the_single_entry() {
    let missing: Vec<&str> = SITES
        .iter()
        .copied()
        .filter(|relative| !product_source(relative).contains("selected_fill("))
        .collect();

    assert!(
        missing.is_empty(),
        "这些文件里没有 `selected_fill(`：{missing:?}\n\
         选中项的底只有 `interaction::selected_fill` 一个入口（M296 收口 + M299 收尾）。\
         自己写一遍 `layer_color(..).or_else(|| 实色)` 会让「改助手」只改到一半——\
         这不是风格问题，M296 的变异实验在外壳那两处实测过。"
    );
}

/// 判据 2（反例）：旧私有拷贝的形态在产品代码里一处都不许留。
///
/// ## 为什么比对「去掉空白之后」的形态（M299 r1 P2-2）
///
/// 上一版拿整串 `"or_else(|| Some(ui.color("` 直接 `contains`：复审把同一形态写成
/// `or_else(|| { Some(ui.color(..)) })`（**只换排版**）就绕过去了——语义完全一样，字面量却不同。
/// 现在比对前把源文件里的空白全部删掉（`split_whitespace().collect()`），针也缩成无空白的形态；
/// 换行、缩进、括号里的换行都不再影响判定。
#[test]
fn the_private_copy_idiom_is_gone() {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    assert!(
        files.len() >= 100,
        "只扫到 {} 个源文件：扫描逻辑坏了，这条门禁等于没跑",
        files.len()
    );

    // `interaction/selected.rs` 自己就是那个入口：它的函数体里允许出现兜底。
    // 本文件也豁免：判据的「针」本身就得写成那个形态，否则扫不到。
    let exempt = [
        src_dir()
            .join("components")
            .join("interaction")
            .join("selected.rs"),
        src_dir()
            .join("components")
            .join("selection")
            .join("tests")
            .join("single_entry.rs"),
    ];

    let mut offenders = Vec::new();
    for path in files {
        if exempt.contains(&path) || is_test_only(&path) || is_known_copy(&path) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("读不动 {}：{error}", path.display()));
        if let Some(hit) = private_copy(&product_part(&text)) {
            offenders.push(format!("{}（命中 {hit}）", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "这些文件里还有「状态层缺了就兜底实色」的私有拷贝：{offenders:?}\n\
         选中项的底请改成 `interaction::selected_fill(ui, state, content, container)`——\
         静息它就是实色容器，叠层交给它。"
    );
}

/// 已知的**同一形态、本次不动**的站点：`(相对路径, 为什么留着)`。
///
/// 单独列出来而不是「用更窄的针绕开」：绕开等于假装它不存在。这一处是滑块的 handle（静息底是
/// 实心 `Primary`，hover/focus/pressed 叠状态层 —— 与选中项**同一套语义**，换成
/// `selected_fill(ui, state, OnSurface, Primary)` 是逐位等价的）。
/// `components/inputs/**` 不在 M299 的 scope，已作为 finding 报给塔。
const KNOWN_COPIES: [(&str, &str); 1] = [(
    "components/inputs/slider/paint.rs",
    "滑块 handle 的静息底是实心 Primary，与选中项同一套语义；components/inputs/** 不在 M299 scope，已报 finding",
)];

/// 这个文件是不是 [`KNOWN_COPIES`] 里挂过豁免的那一个。
fn is_known_copy(path: &Path) -> bool {
    let src = src_dir();
    KNOWN_COPIES.iter().any(|(relative, _)| {
        path.strip_prefix(&src)
            .is_ok_and(|tail| tail == Path::new(relative))
    })
}

/// 判据 3（豁免要**活着**）：[`KNOWN_COPIES`] 里的文件必须**仍然**含那个形态。
///
/// 守的是「豁免越挂越多」：谁把那处收口了，这条立刻红，逼他回来删掉豁免行；谁想给自己文件
/// 挂个空豁免，也红。
#[test]
fn every_known_copy_is_still_there() {
    let stale: Vec<&str> = KNOWN_COPIES
        .iter()
        .map(|(relative, _)| *relative)
        .filter(|relative| private_copy(&product_source(relative)).is_none())
        .collect();

    assert!(
        stale.is_empty(),
        "这些豁免已经失效（文件里没有那个形态了）：{stale:?}\n\
         收口了就把它从 `KNOWN_COPIES` 里删掉——豁免表不许留空条目。"
    );
}
