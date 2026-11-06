//! **断行工具**：给没有空格的长标识符补上「软断点」。
//!
//! ## 为什么在这里而不是日志页里
//!
//! 这是**排版度量**问题，不是日志页的私事：`layout.rs` 已经是页面级几何的唯一真源
//! （档位 / 版面 / 列宽 / 行宽上限），「一行在哪儿能断」属于同一类判据的第二半——
//! 列宽决定**能放多宽**，断点决定**放不下时从哪儿折**。放在 `layout/` 下面：
//!
//! - 日志页（`pages/logs/**`）与以后任何要排长串的页都能用，不产生第二份实现；
//! - 它是纯函数（`&str → String`），不碰 iced 控件树，能在单测里逐字符钉死。
//!
//! ## 修的是什么（M102 视觉复审的 ③）
//!
//! 崩溃日志的一行摘要与堆栈里有 `graphics.gl_operation_failure:medium(70)` 这种
//! **完全没有空格**的长串：iced 的 `Wrapping::Word` 找不到断点，只能退化成按字形硬断，
//! 于是图里出现了 `graphics.gl_o` / `peration_failure`——单词被从中间劈开。
//!
//! 修法不是改字号也不是截断（截断会让用户拿不回原始堆栈），而是往
//! `.` `_` `:` `/` `\` 这些**标识符/路径分隔符后面**插一个**零宽空格**（U+200B）：
//!
//! - U+200B 是 Unicode 的**软断点**（UAX #14 `ZW`）：排版时可以在这里折行，但不占宽度、
//!   不产生字形，`Wrapping::WordOrGlyph` 会优先在它那里折；
//! - 它只进**显示**用的字符串，不进数据：`Logs::summary()`（剪贴板）与 `Line::text`
//!   （视图模型）仍是原文，`strip` 一下就能还原（见单测）。
//!
//! ## 边界（没做什么）
//!
//! - 只在**分隔符之后**插，不在之前：「`a.b`」断成「`a.` / `b`」比「`a` / `.b`」好读；
//! - 分隔符后面紧跟空白、或结尾，就不插（`Wrapping` 本来就能在空白处折）；
//! - 连续分隔符（`a..b`、`a/_b`）只在最后那个后面插一个，不制造一串空断点；
//! - `-` **不插**：时间戳 `2026-11-02 19:40` 断在日期中间比不断更难看，
//!   而 `-` 两侧几乎都有空白/标点兜底。

/// 零宽空格（U+200B）：可断行、不占宽度的软断点。
pub const SOFT_BREAK: char = '\u{200b}';

/// 软断点允许落在哪些字符**之后**。
///
/// 取标识符 / 路径 / 规则 id 的分隔符：`integrity.unsatisfied_link`、
/// `graphics.gl_operation_failure:medium(70)`、`logs/2026-11-02/client.txt`、
/// `AppData\Roaming\.unlucky\instances\dev-test\natives`（Windows 路径，反斜杠也断）。
pub const BREAK_AFTER: [char; 5] = ['.', '_', ':', '/', '\\'];

/// 给 `text` 补上软断点（返回**显示用**的新串；数据侧原文不动）。
///
/// 幂等：对已经补过断点的串再跑一遍不会插更多（U+200B 不是分隔符，也不是空白以外的可断字符）。
pub fn breakable(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        out.push(ch);

        if !BREAK_AFTER.contains(&ch) {
            continue;
        }

        // 只在「分隔符 + 真内容」之间插：结尾、空白、连续分隔符、已有软断点都不插。
        let worth_breaking = chars.peek().is_some_and(|next| {
            !next.is_whitespace() && *next != SOFT_BREAK && !BREAK_AFTER.contains(next)
        });

        if worth_breaking {
            out.push(SOFT_BREAK);
        }
    }

    out
}

/// 去掉 [`breakable`] 插进去的软断点（还原成数据原文）。
///
/// 只给测试与调试用：页面渲染时不需要还原（原文从未被改写）。
///
/// **限界（r1 复审指出）**：它删的是**所有** U+200B，所以原文自带的零宽空格也会被一并删掉
/// （`strip(breakable(x)) == x` 只在「x 本身不含 U+200B」时成立；实测样例里没有这种串）。
/// 这个函数因此只配「与 [`breakable`] 成对使用」的场景，不能当通用的原文还原器用——
/// 产品侧从不调用它，剪贴板/视图模型拿的本来就是没动过的原文。
pub fn strip(text: &str) -> String {
    text.chars().filter(|ch| *ch != SOFT_BREAK).collect()
}

#[cfg(test)]
mod tests;
