//! 导航文案：四个一级页。AI 独立页没有导航高亮项（`Route::Agent.nav()` 是 `None`），所以没有 key。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节：本宏把条目**追加**到 `$acc` 之后，
//! 再把「还没装配的域」原样交给 `$next`。条目顺序 = `Key` 的声明顺序（金表 `catalog::golden` 守门）。
//!
//! **显示名与标识符不同名**（M108）：`NavDownload` 这条说的是「版本管理」，而它指向的仍是
//! `Route::Download` / `NavSection::Download`。用户令（2026-11-04）只改**显示层**：全仓重命名
//! 标识符会与在途的多条线全面撞车，收益只在观感，所以标识符改名后置。
//!
//! ## 为什么「版本管理」有两条 key（M108）
//!
//! D33 定死了导航两态：**88 收起轨 ⇄ 240 展开抽屉**。88px 里塞得下中文「版本管理」（4 字 ≈48px），
//! 但塞不下英文 `Version management`（18 字符 ≈101px > 88）——那会变成截断或两行，两个都是视觉缺陷
//! （两行还要把项高从 52 抬到 68，等于为一个词改全局导航节奏）。
//!
//! 所以按**容器宽度**分两条：
//!
//! | key | 用在哪 | en | zh-CN |
//! |---|---|---|---|
//! | [`NavDownload`] | 88 收起轨（`shell/nav.rs` / `pages/nav.rs`） | `Versions` | 版本管理 |
//! | [`NavDownloadExpanded`] | 240 展开抽屉（D33 的展开态） | `Version management` | 版本管理 |
//!
//! 中文两条**故意同值**：48px 放得下就没有理由为「对称」砍短（那是视觉缺陷，不是风格）。
//! 英文短名选 `Versions` 而不是 `Versions list` / `Downloads`：单一词最省宽度，且不与「下载」
//! （真在传文件的动作、设置页的下载分组）混淆。切换点只有 [`crate::catalog`] 的调用方一处，
//! 见 `pages/nav.rs` 的 `label_key`。

macro_rules! nav_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 导航轨（`pages::Route` 的四项）
            NavInstances => ("Instances", "实例"),
            NavDownload => ("Versions", "版本管理"),
            NavDownloadExpanded => ("Version management", "版本管理"),
            NavLogs => ("Logs", "日志"),
            NavSettings => ("Settings", "设置"),
        } }
    };
}

pub(crate) use nav_entries;
