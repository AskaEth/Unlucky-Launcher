//! **剥注释**的行扫描：接线守卫的判据只能看「代码行」，不能看「文本里还有没有那个字符串」。
//!
//! ## 为什么要有这个文件（T29 的第三次复发）
//!
//! 接线守卫是拿源码文本当判据的（`Element` 没有内省接口，见 [`super`] 的模块文档）。用
//! `text.contains(".on_submit(")` 写时，把真接线**注释掉**就整条骗过：字符串还在，功能没了。
//! M252 复审实测了四种注释型变异（行注释 / `/*` 独占一行的块注释 / `.on_submit_maybe(None)`
//! 再加 `let _submit` 绕掉 rustc 的 unused 兜底），**943 条用例一条不红**；同一批「真删除」
//! 变异则每条都红。⇒ 守卫不是没用，是**只认一种改法**。
//!
//! 所以：接线类断言一律先过 [`code_lines`]（注释剥掉、只留代码），再按**行首**判 [`code_line`]。
//!
//! ## 剥什么（三条都要记）
//!
//! | 形态 | 处理 |
//! |---|---|
//! | `// …` / `/// …` / `//! …` | 剥到行尾 |
//! | `/* … */`，**跨行、可嵌套**（Rust 允许嵌套块注释） | 整段剥掉；行结构保留（只被注释占满的行变空串） |
//! | 字符串字面量里的 `//` / `/*`（`"http://x"`、`"a /* b"`） | **不剥**：字面量整段原样搬过去 |
//! | 字符字面量 `'"'` / `'\''` | 整段搬过去，不会误开字符串 |
//!
//! ## 剥不了什么（边界，改守卫前先读这一节）
//!
//! 1. **原始字符串没有被特判**：`r"…"` / `r#"…"#` 里那个 `"` 被当成普通字符串起点，于是
//!    内容（含 `//`）原样保留——对**不含**引号的内容（`r"a // b"`）恰好是对的。含引号时会错位，
//!    而**含引号是常态不是例外**（JSON 字面量必然含 `"`）。实测（`git ls-files` + 逐字面量扫
//!    `r(#+)"`）：`crates/app-iced/src` 下 **9 处** `r#"`、其中 **8 处内容含 `"`**；
//!    全仓 `crates/**.rs` **291 处**。
//!
//!    两个方向都要知道（都由 `code/tests.rs` 的用例钉住）：
//!
//!    | 写法 | 偏差 | 方向 |
//!    |---|---|---|
//!    | `r#"a " b"#; // .on_submit(fake())` | 提前收尾后又开了一个「字符串」⇒ **尾部注释不被剥** | **假绿**（`code_text(..).contains(..)` 形式的断言会被骗） |
//!    | `r#"{"url":"http://x"}"#; // …` | 截断到 `http:` 之后 ⇒ **多剥** | **假红** |
//!
//!    **对本仓的三处接线守卫无影响**：它们判「行首就是那个调用」（[`code_line`]），而行首是
//!    `r#"…` 的行永远匹配不上；受影响的是 `code_text(..).contains(..)` 形式的旧断言
//!    （T29 清单里还有约 90 处）。三处被守卫的文件（`field.rs` / `saves/rows.rs` /
//!    `section/accent.rs`）`r#"` 计数都是 0。
//! 2. **跨行普通字符串**（行尾 `\` 续行）**不识别**：字符串状态每行复位，续行的下一行会当代码。
//!    后果是**假红**（把字符串里的一段当代码看见，反而更啰嗦）。
//! 3. 它**不判语义**：注释剥掉之后，「`.on_submit(..)` 在不在」仍然只是文本事实。等价重构
//!    （先赋值再调用）可能绕过——所以接线守卫的 needle 与写法要一起维护（见各守卫的 docstring）。
//!
//! 除第 1 条里「内嵌引号」那个形态外，其余偏差都是**假红**：宁可假红，也不要「注释掉仍绿」。

/// 逐行的代码文本（注释已剥）。**行数与输入一一对应**：只被注释占满的行变成空串。
///
/// 保留行结构是为了让失败信息能直接对回源码行，也让「行首」这条谓词有意义。
#[must_use]
pub(crate) fn code_lines(source: &str) -> Vec<String> {
    let mut lines = Vec::with_capacity(source.lines().count());
    // 块注释的嵌套深度（0 = 不在注释里）。跨行靠它带着走。
    let mut depth = 0_usize;

    for line in source.lines() {
        let chars: Vec<char> = line.chars().collect();
        let mut code = String::with_capacity(chars.len());
        let mut index = 0;

        while index < chars.len() {
            let ch = chars[index];

            if depth > 0 {
                if ch == '/' && chars.get(index + 1) == Some(&'*') {
                    depth += 1;
                    index += 2;
                } else if ch == '*' && chars.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
                continue;
            }

            if ch == '/' && chars.get(index + 1) == Some(&'/') {
                break;
            }
            if ch == '/' && chars.get(index + 1) == Some(&'*') {
                depth = 1;
                index += 2;
                continue;
            }

            if ch == '"' {
                index = copy_string_literal(&chars, index, &mut code);
                continue;
            }
            if ch == '\'' {
                index = copy_char_literal(&chars, index, &mut code);
                continue;
            }

            code.push(ch);
            index += 1;
        }

        lines.push(code.trim_end().to_owned());
    }

    lines
}

/// 把从 `start`（一个 `"`）起的字符串字面量原样搬进 `code`，返回下一个未处理的下标。
///
/// 「原样」= 里面的 `//`、`/*` 都不当注释（这条正是本模块存在的理由之一）。
/// 行内没有闭合引号时搬到最后（跨行字符串见模块文档第 2 条边界）。
fn copy_string_literal(chars: &[char], start: usize, code: &mut String) -> usize {
    code.push(chars[start]);
    let mut index = start + 1;

    while index < chars.len() {
        let ch = chars[index];
        code.push(ch);
        index += 1;

        match ch {
            // 转义：把被转义的字符一起搬走（`\"` 不会提前收尾）
            '\\' => {
                if index < chars.len() {
                    code.push(chars[index]);
                    index += 1;
                }
            }
            '"' => break,
            _ => {}
        }
    }

    index
}

/// 把从 `start`（一个 `'`）起的**字符字面量**原样搬过去；生命周期（`'a`）只搬那个引号。
///
/// 为什么值得单独判：`'"'` 会让「见到 `"` 就当字符串开头」的扫描器错位，于是整行的注释剥错。
fn copy_char_literal(chars: &[char], start: usize, code: &mut String) -> usize {
    code.push(chars[start]);
    let index = start + 1;

    // `'\n'` / `'\''`：反斜杠 + 一个字符 + 收尾引号
    if chars.get(index) == Some(&'\\') && chars.get(index + 2) == Some(&'\'') {
        for offset in index..index + 3 {
            if let Some(ch) = chars.get(offset) {
                code.push(*ch);
            }
        }
        return index + 3;
    }

    // `'x'`：一个字符 + 收尾引号
    if chars.get(index + 1) == Some(&'\'') {
        if let Some(ch) = chars.get(index) {
            code.push(*ch);
        }
        code.push('\'');
        return index + 2;
    }

    // 生命周期 / 未闭合：只搬引号，余下的照常扫
    index
}

/// `source` 里第一条**去注释后行首是 `prefix`** 的代码行（`None` = 没有这样一行）。
///
/// 接线守卫统一用这个谓词：`code_line(rows_rs, ".on_submit(")` —— 那行被注释掉就查不到。
#[must_use]
pub(crate) fn code_line(source: &str, prefix: &str) -> Option<String> {
    code_lines(source)
        .into_iter()
        .find(|line| line.trim_start().starts_with(prefix))
}

/// [`code_line`] 的布尔形态（全文件扫描那种只关心「有没有」的场合）。
#[must_use]
pub(crate) fn has_code_line(source: &str, prefix: &str) -> bool {
    code_line(source, prefix).is_some()
}

/// 整份源码剥注释后的文本（行结构保留）。
///
/// **T29 的统一收敛口**：仓库里还有一批 `source.contains("…")` 形式的接线断言（见 M252 r1
/// 报告的清单）。它们在问的是同一件事——「这段接线在不在」——只是写法更早。把
/// `source.contains(..)` 换成 `code_text(&source).contains(..)` 是一行改动：判据方向不变，
/// 但「注释掉仍绿」这条退化被堵上。新写的断言优先用 [`code_line`]（能钉「行首是哪个调用」）。
#[must_use]
pub(crate) fn code_text(source: &str) -> String {
    code_lines(source).join("\n")
}

#[cfg(test)]
mod tests;
