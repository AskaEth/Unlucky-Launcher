//! 设置页「AI / About」分组文案：**预留**（页面上还没有这个分组，见 `pages/settings/mod.rs` 的
//! 分组注册表）。
//!
//! 加 key 时把条目写进 `$($acc)*` 后面即可（形状见 [`crate::catalog`] 的「域装配」一节）。
//! 现在没有条目，所以本宏是**空转移器**：只把累积表原样交给下一个域。

macro_rules! settings_ai_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* { $($acc)* } }
    };
}

pub(crate) use settings_ai_entries;
