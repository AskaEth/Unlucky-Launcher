//! **源码结构断言**的小工具：把某个函数的实现文本切出来（只给测试用）。
//!
//! ## 为什么要读源码（而不是量 Element）
//!
//! iced 的 `Element` 没有几何内省接口（`size()` 只是 `Size<Length>`，取不到布局结果），
//! 所以「页面把块摆在哪 / 有没有在行之间叠间距」在单测里**量不到**。M102 r1 的教训正是这个：
//! 护栏只喂 `frame_metrics()` 与常量重算，把 `frame::body` 换回「居中 424」的实现，
//! 测试照样全绿——钉住的是公式，不是摆法。
//!
//! 本仓已有同一条纪律的先例：`tests/no_hardcoded_tokens.rs` 直接扫 `src/**`，
//! `pages/download/tests.rs::every_download_key_is_referenced_by_the_page` 用 `include_str!`
//! 扫页面源码找 key 引用。这里只是把「切出某个函数」这件小事做成一个共用工具，
//! 让护栏能断言「盒子是不是照那个数建的」。
//!
//! ## 边界（它抓不住什么）
//!
//! 它读的是**文本**：换个写法（先赋值给局部变量再 `.width(...)`）就可能绕过。
//! 所以它只配一条**明确的契约**用：`frame::body` / `frame::prose` / `list_column` 这几处的
//! 写法本身就是契约的一部分（它们各自只有 5~10 行），不是为了抠实现细节。
//! 真要守像素，还是得靠软件渲染金图（`shot::diff`，绑 Windows + 字体栈，属另一个决定）。
//!
//! ## 「接线在不在」必须走 [`code_line`]（T29，M252 r1 的 P1）
//!
//! 文本判据有个致命退化：把真接线**注释掉**、字符串留在注释里，`contains(..)` 照样绿。
//! 所以断言「某处接了 `X`」时不许直接 `contains`，统一走 [`code_line`] / [`has_code_line`]
//! （注释剥掉、按行首判，边界见 [`code`] 的模块文档）。

mod code;

pub(crate) use code::{code_line, code_lines, code_text, has_code_line};

/// 把 `source` 里第一个 `signature` 起始、到该函数**顶层**右花括号为止的文本切出来。
///
/// 「顶层」= 行首的 `}`（本仓函数都是顶格收尾，内层块都是缩进收尾），所以不会在内层 `}` 提前断。
///
/// # Panics
/// 签名不存在时 panic（护栏自己的前置条件坏了要立刻红，不能静默放行）。
pub(crate) fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("源码里找不到签名 {signature:?}"));

    let rest = &source[start..];
    let end = rest.find("\n}").map_or(rest.len(), |index| index + 2);
    &rest[..end]
}

/// 从 `start` 起到**第一个 `;`** 为止的一条语句（构建链断言用）。
///
/// 例如 `statement(instances.rs, "let mut rows = column(")` 拿到的是
/// `let mut rows = column(...).padding(...).width(Length::Fill);` 这一整条链——
/// 于是「这条链上不许叠间距」可以精确到**这一个**容器，不会连带禁掉旁边 `column![head, scroller]`
/// 上的合法间距。
///
/// # Panics
/// `start` 不存在时 panic（守卫的前置条件坏了要立刻红）。
pub(crate) fn statement<'a>(source: &'a str, start: &str) -> &'a str {
    let from = source
        .find(start)
        .unwrap_or_else(|| panic!("源码里找不到语句起点 {start:?}"));

    let rest = &source[from..];
    let end = rest.find(';').map_or(rest.len(), |index| index + 1);
    &rest[..end]
}

/// `signature` 的函数体 **+ 它内部转调的函数体**（同文件，最多两层）。
///
/// 为什么不只切一个函数：**等价重构不该假红**。把 `frame::body` 拆成
/// `body` → `body_impl` 两层再转调，盒子一个像素都没变，`function_body` 却会拿到三行转调桩、
/// needle 全缺（r2 复审的变异 H1 就是这样假红的）。这里把「被它调用的函数」也算进契约，
/// 于是「拆一层再转调」放行，「换成居中盒子」照样红。
///
/// 跨文件转调用 [`region_across`]。两者都不跟「先赋局部变量再建盒」的写法。
pub(crate) fn function_region(source: &str, signature: &str) -> String {
    region_across(&[source], signature)
}

/// 多文件版的 [`function_region`]：[`expand`] 会在给定的**所有**文件里按名字找转调目标。
///
/// 存在的理由（r4 复审的变异 M6）：把盒子原样搬进 `layout/block.rs::body_box` 再由
/// `frame::body` 转调，是**等价重构**，单文件版会把 needle 全丢掉并假红。
/// 现在把 `frame.rs` 与 `layout/block.rs` 一起交进来，搬运不再被惩罚。
///
/// # Panics
/// `signature` 在任何一个文件里都找不到时 panic（护栏前置条件坏了要立刻红）。
pub(crate) fn region_across(sources: &[&str], signature: &str) -> String {
    let mut region = sources
        .iter()
        .find(|source| source.contains(signature))
        .map(|source| function_body(source, signature).to_owned())
        .unwrap_or_else(|| panic!("这几个文件里都找不到签名 {signature:?}"));

    for _ in 0..2 {
        if !expand(&mut region, sources) {
            break;
        }
    }

    region
}

/// 把 `region` 里出现、且定义在 `sources` 某处的函数体补进来。返回是否有增长。
fn expand(region: &mut String, sources: &[&str]) -> bool {
    let mut grew = false;

    for source in sources {
        let mut cursor = 0;

        while let Some(found) = source[cursor..].find("fn ") {
            let at = cursor + found + "fn ".len();
            cursor = at;

            let name: String = source[at..]
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect();
            if name.is_empty() {
                continue;
            }

            // `defined` 不带 `(`：签名可能带泛型（`fn body_impl<'a>(`），
            // 带括号的 needle 会找不到定义、直接把护栏自己搞 panic。
            let defined = format!("fn {name}");
            if region.contains(&format!("{name}("))
                && !region.contains(&defined)
                && source.contains(&defined)
            {
                region.push('\n');
                region.push_str(function_body(source, &defined));
                grew = true;
            }
        }
    }

    grew
}

/// `haystack` 里每一处 `callee(…)` 调用的**实参文本**（不含括号本身）。
///
/// 括号按深度配对，所以 `ui.space(Space::X1)` 这种嵌套实参会完整取到 `ui.space(Space::X1)`；
/// 找不到闭合括号时取到文本末尾（宁可让断言看见多余文本，也不静默返回空）。
pub(crate) fn call_arguments<'a>(haystack: &'a str, callee: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut from = 0;

    while let Some(index) = haystack[from..].find(callee) {
        let start = from + index + callee.len();
        let mut depth = 1usize;
        let mut end = haystack.len();

        for (offset, ch) in haystack[start..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + offset;
                        break;
                    }
                }
                _ => {}
            }
        }

        found.push(&haystack[start..end]);
        from = end.max(start);
    }

    found
}

/// 顺绑定链往回追：`ident` 是否（直接或经至多 3 跳中转）来自 `root`。
///
/// 认两种绑定形状（本仓就这两种）：
///
/// - `let <ident> = <expr>;`
/// - `if let Some(<ident>) = <expr> {`（`while let` 同理，形状一样）
///
/// 为什么需要它（r4 复审的变异 M4）：把写死的令牌先挪到一条 `let extra = Space::X1;`，
/// 再 `rows = rows.spacing(ui.space(extra));` —— 只看实参文本「有没有 `Space::`」会漏掉它。
/// 顺着绑定追回源头就能区分「真的来自 `ROW_SPACING`（含 `let spacing = ROW_SPACING` 再中转，
/// 变异 H3）」与「中转了一层写死令牌」。
pub(crate) fn derived_from(body: &str, ident: &str, root: &str) -> bool {
    let mut frontier = vec![ident.to_owned()];

    for _ in 0..3 {
        let mut next = Vec::new();

        for name in &frontier {
            for rhs in binding_rhs(body, name) {
                if rhs.contains(root) {
                    return true;
                }

                next.extend(
                    rhs.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                        .filter(|token| {
                            token
                                .chars()
                                .next()
                                .is_some_and(|first| first.is_alphabetic())
                        })
                        .map(str::to_owned),
                );
            }
        }

        if next.is_empty() {
            return false;
        }
        frontier = next;
    }

    false
}

/// `ident` 在 `body` 里的绑定表达式（`let` 与 `Some(ident) =` 两种形状）。
fn binding_rhs<'a>(body: &'a str, ident: &str) -> Vec<&'a str> {
    let mut rhs = Vec::new();

    for pattern in [format!("let {ident} = "), format!("let mut {ident} = ")] {
        let mut from = 0;
        while let Some(index) = body[from..].find(&pattern) {
            let start = from + index + pattern.len();
            let end = body[start..].find(';').map_or(body.len(), |at| start + at);
            rhs.push(&body[start..end]);
            from = end;
        }
    }

    let pattern = format!("Some({ident}) = ");
    let mut from = 0;
    while let Some(index) = body[from..].find(&pattern) {
        let start = from + index + pattern.len();
        let end = body[start..].find('{').map_or(body.len(), |at| start + at);
        rhs.push(&body[start..end]);
        from = end;
    }

    rhs
}

#[cfg(test)]
mod tests;
