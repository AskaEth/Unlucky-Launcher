//! 加载器文案：品牌名不翻译（Fabric / Forge / Quilt 一律原样），只有「原版」是一个词。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! loader_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 加载器（品牌名不翻译：Fabric / Forge / Quilt 一律原样，只有「原版」是词）
            LoaderVanilla => ("Vanilla", "原版"),
        } }
    };
}

pub(crate) use loader_entries;
