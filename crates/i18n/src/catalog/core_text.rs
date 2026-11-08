//! core / presenter 只给了中文串、UI 又不许画原文时的兜底文案（口径见 [`crate::bridge`]）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! core_text_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── core / presenter 只给了中文串、UI 又不许画原文时的兜底（见 `bridge`）
            CoreTextUntranslated => ("[system text not translated]", "［未翻译的系统文案］"),
        } }
    };
}

pub(crate) use core_text_entries;
