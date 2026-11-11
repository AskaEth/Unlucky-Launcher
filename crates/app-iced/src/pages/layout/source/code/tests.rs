//! [`code_lines`] 自己的断言（门禁的门禁，T28：守卫守不到自己就等于没有）。
//!
//! 这一族用例的形状统一：**喂一段源码文本 → 断言剥完剩下什么**。它们不读真文件，所以不会因为
//! 别人改页面而假红；真文件上的效果由三处接线守卫自己证。

use super::{code_line, code_lines, has_code_line};

/// 行结构必须保留：只被注释占满的行是空串，代码行原样（去掉行尾空白）。
#[test]
fn it_strips_line_comments_and_keeps_the_line_count() {
    let source = "\
let a = 1; // 尾注释
// 整行注释
/// 文档注释
let b = 2;
/* 单行块注释 */
let c = 3;
";
    let lines = code_lines(source);
    assert_eq!(lines.len(), 6, "行数必须一一对应：{lines:?}");
    assert_eq!(lines[0], "let a = 1;");
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "");
    assert_eq!(lines[3], "let b = 2;");
    assert_eq!(lines[4], "");
    assert_eq!(lines[5], "let c = 3;");
}

/// 跨行 / 嵌套块注释：整段剥掉，注释结束处的代码要接着留下来。
#[test]
fn it_strips_block_comments_across_lines_and_nested() {
    let source = "\
let a = 1;
/* 开头 /* 嵌套 */ 还在注释里
.on_submit(comment_only())   <- 这一行整个在注释里
*/ let b = 2;
let c = /* 行内 */ 3;
";
    let lines = code_lines(source);
    assert_eq!(lines[0], "let a = 1;");
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "", "块注释里的「接线」不许当成代码");
    assert_eq!(lines[3], " let b = 2;", "注释结束后同一行的代码要留下");
    assert_eq!(lines[4], "let c =  3;");
}

/// 字符串字面量里的 `//` 与 `/*` **不剥**（否则会把后半行代码剥掉 ⇒ 假红方向）。
#[test]
fn it_keeps_slashes_inside_string_literals() {
    let source = r##"
let url = "http://example.com/a//b"; let after = 1;
let odd = "a /* not a comment */ b"; let tail = 2;
let raw = r#"a // b"#; let tail2 = 3;
"##;
    let lines = code_lines(source);
    assert!(
        lines[1].contains("http://example.com/a//b"),
        "{:?}",
        lines[1]
    );
    assert!(
        lines[1].contains("let after = 1;"),
        "字符串里的 `//` 不该吃掉后半行：{:?}",
        lines[1]
    );
    assert!(lines[2].contains("let tail = 2;"), "{:?}", lines[2]);
    // 原始字符串**没有特判**：靠其中那个 `"` 偶然按普通字符串处理（模块文档边界第 1 条）。
    assert!(lines[3].contains("a // b"), "{:?}", lines[3]);
    assert!(lines[3].contains("let tail2 = 3;"), "{:?}", lines[3]);
}

/// 已知缺口（模块文档边界第 1 条）：原始字符串含内嵌引号时扫描器会错位，**两个方向都钉住**。
///
/// 不是「希望这样」，是「哪天修了要知道」；也是 `code_text(..).contains(..)` 形式旧断言的已知上限。
#[test]
fn raw_strings_with_inner_quotes_are_a_known_gap() {
    // ① 多剥 ⇒ 假红：`r#"{"url":"http://x"}"#` 那一行会被截断（字符串里的 `//` 当成注释）。
    let json = "let s = r#\"{\"url\":\"http://x\"}\"#; let after = 1;\n";
    assert!(
        !code_lines(json)[0].contains("let after = 1;"),
        "已知缺口（假红方向）：JSON 原始字符串那行的后半行被剥掉了",
    );

    // ② 漏剥 ⇒ 假绿：内嵌引号之后扫描器又开了一个「字符串」，**尾部注释不被剥**，
    //    于是注释里的 `.on_submit(fake())` 会留在「代码」里 —— `code_text(..).contains(..)`
    //    形式的断言会被它骗过（三处接线守卫按行首判，不受影响）。
    let sneaky = "let s = r#\"a \" b\"#; // .on_submit(fake())\n";
    let line = &code_lines(sneaky)[0];
    assert!(
        line.contains(".on_submit(fake())"),
        "已知缺口（假绿方向）：内嵌引号的原始字符串会让尾部注释留下，实际：{line:?}"
    );
    assert!(
        !has_code_line(sneaky, ".on_submit("),
        "行首判不受这条影响：那一行的行首是 `let s = r#\"`，永远匹配不上"
    );
}

/// 字符字面量里的引号不会让扫描器错位（`'"'` 之后还有注释与代码）。
#[test]
fn char_literals_do_not_open_a_string() {
    let source = "\
let quote = '\"'; let after = 1; // 尾注释
let nl = '\\n'; let more = 2;
let lifetime: &'a str = \"x\";
";
    let lines = code_lines(source);
    assert!(lines[0].contains("let after = 1;"), "{:?}", lines[0]);
    assert!(!lines[0].contains("尾注释"), "{:?}", lines[0]);
    assert!(lines[1].contains("let more = 2;"), "{:?}", lines[1]);
    assert!(lines[2].contains("&'a str = \"x\""), "{:?}", lines[2]);
}

/// 行首谓词：注释里的「接线」查不到，代码里的查得到（本模块的全部意义）。
#[test]
fn the_line_predicate_only_sees_code() {
    let commented = "\
let field = text_field(
    ui,
    Field::new(text)
        // .on_submit(|| Message::Instances(commit_rename())),
    ,
    on_input,
);
";
    assert!(
        !has_code_line(commented, ".on_submit("),
        "注释掉的那一行不许被当成交接"
    );

    let blocked = "\
let field = text_field(
    ui,
    Field::new(text)
        /*
        .on_submit(|| Message::Instances(commit_rename())),
        */
    ,
    on_input,
);
";
    assert!(
        !has_code_line(blocked, ".on_submit("),
        "块注释掉的那一行不许被当成交接"
    );

    let wired = "\
let field = text_field(
    ui,
    Field::new(text)
        .on_submit(|| Message::Instances(commit_rename())),
    on_input,
);
";
    let line = code_line(wired, ".on_submit(").expect("接在代码里就该查得到");
    assert!(line.contains("commit_rename()"), "{line}");
}

/// needle 带实参时才分得清「转发」与「写死 None」——这正是 M252 复审的第二格。
#[test]
fn the_predicate_tells_a_wired_argument_from_a_hardcoded_none() {
    let wired = "    .on_submit_maybe(submit)\n";
    let none = "    .on_submit_maybe(None)\n";
    assert!(has_code_line(wired, ".on_submit_maybe("));
    assert!(
        has_code_line(none, ".on_submit_maybe("),
        "两条都「有这一行」"
    );

    let picked = |source: &str| {
        code_line(source, ".on_submit_maybe(")
            .expect("有那一行")
            .contains("None")
    };
    assert!(!picked(wired), "传的是值，不是 None");
    assert!(picked(none), "写死 None 必须被认出来");
}
