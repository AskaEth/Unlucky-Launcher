//! 消息目录：`Key` 枚举 + 中英两套常量表 + 取词 / 占位符渲染 / 缺译诊断。
//!
//! ## 为什么 key 是枚举而不是字符串
//!
//! 字符串 key（`"nav.instances"`）拼错了编译器一声不响，运行时才知道——本仓的说法是「运行时静默回落」。
//! 枚举 key 把这类错误前移到**编译期**：`Key::NavInstance`（真名是 `NavInstances`）直接
//! `error[E0599]`，不可能跑到运行时。
//!
//! **这里没有守卫测试**：曾经写过一个 `compile_fail` 文档测试想守这条，但复审实测它对
//! 「把 `Key` 换成字符串包装类型」这类变异仍然全绿——它守的是「这段代码编译不过」，而不是
//! 「编译期校验还在」。说不清真假的守卫比没有更糟，所以删了；这条纪律只靠枚举这个事实本身。
//!
//! ## 为什么两套译文写在**同一行**
//!
//! ```ignore
//! NavInstances => ("Instances", "实例"),
//! ```
//!
//! 一行就是「一个 key 的全部语言」。于是「加了 key 忘了译文」在结构上不可能发生——这比「运行时发现
//! 缺译再诊断」更强。缺译诊断（[`Catalog::diagnose`]）因此有两个真实用途：
//!
//! 1. **注入目录**：后续 mission 用 [`Catalog::from_entries`] 加第三套（繁体 / 更多语言）时，
//!    表格是手写的，缺条目的风险回来了；
//! 2. **回归守门**：万一有人把这张表改回「两套各写一遍」，[`Catalog::missing`] 的守卫测试立刻红。
//!
//! ## 按域拆开（M99 `SETTINGSFRAME`）
//!
//! 条目表原先整张写在本文件里，而**设置页正文接下来由四条 mission 并行接**（java / download /
//! language+appearance / ai+about），每条都要往这张表里加一批 key——同一张表就是四条线的冲突点。
//! 现在一个域一个文件（`catalog/<域>.rs`），本文件只留：`catalog!` 宏、装配链、[`Catalog`] 与对外 API。
//!
//! | 文件 | 域 | 谁会长它 |
//! |---|---|---|
//! | [`nav`] / [`instances`] / [`loader`] / [`launch`] / [`font`] / [`core_text`] | 页面与运行时文案 | 各自页面 |
//! | [`download`] / [`logs`] | 下载页 / 日志页（M94 / M95） | 那两页 |
//! | `settings/`（`general` / `appearance` / `java` / `download` / `ai`） | 设置页，**一个分组一个文件** | 设置页四条线 |
//! | [`general`] | 设置页 General 分组的**真控件**文案（语言选择器三档 + 主题/启动页/关闭三行） | M106 `I18NFOLLOW`（挂在链尾，既有顺序不动） |
//!
//! ### 域文件的形状（加 key 时照抄）
//!
//! ```ignore
//! macro_rules! <域>_entries {
//!     ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
//!         $next! { $($rest,)* {
//!             $($acc)*
//!             NavInstances => ("Instances", "实例"),
//!         } }
//!     };
//! }
//! pub(super) use <域>_entries;
//! ```
//!
//! 它把本域条目**追加**到累积表 `$acc` 之后，再把「还没装配的域」原样交给 `$next`——最后交给
//! [`finish`]，由它把整张表喂给 [`catalog!`]。**为什么非得这么绕**：`macro_rules!` 不能在
//! 宏参数里展开另一个宏（`catalog! { nav_entries!() }` 只会把 `nav_entries!()` 当成普通 token），
//! 也不能用元变量当宏名（`macro_rules! $name` 非法），所以只能「回调链」式累积。代价是每个域文件
//! 都要重抄上面那 6 行签名——这是 `macro_rules!` 的固有成本，不是可以省掉的样板。
//!
//! ### 拆开之后仍然只有一个真相源
//!
//! 装配顺序写在下面那条调用里，**它等于 `Key` 的声明顺序**；[`golden`] 把拆分前那份
//! `key -> (en, zh-CN)` 全量逐字节钉住（连顺序一起钉），所以「搬丢一条 / 两条互抄 / 顺序被调换」
//! 都会红。
//!
//! ## 缺译时给什么
//!
//! **不回落**（绝不拿另一种语言顶替，用户会看到「一半英文一半中文」的界面而不自知）：
//!
//! - [`Catalog::get`] → `None`（严格调用方用它）；
//! - [`Catalog::text`] → [`MISSING`]（`⟪missing⟫`，UI 上刺眼但一眼可辨）；
//! - [`Catalog::render`] → `⟪missing:KeyName⟫`（带上 key 名，方便照着日志补译）。

use crate::lang::Lang;

/// 缺译占位符（`text()` 的返回；故意刺眼——宁可难看，也不许静默顶替）。
pub const MISSING: &str = "⟪missing⟫";

/// 生成 `Key` 枚举 + 两套常量表（一行 = 一个 key 的全部语言）。
macro_rules! catalog {
    ($($variant:ident => ($en:literal, $zh:literal)),+ $(,)?) => {
        /// 可翻译文案的 key。**不存在时编译失败**（枚举变体名即契约）。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Key {
            $(
                #[doc = concat!("en: `", $en, "` / zh-CN: `", $zh, "`")]
                $variant,
            )+
        }

        impl Key {
            /// 全部 key（缺译诊断与完整性测试的遍历源）。
            pub const ALL: &'static [Key] = &[$(Key::$variant),+];

            /// 变体名（日志 / 缺译诊断里用来指认 key；不是文案）。
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Key::$variant => stringify!($variant),)+
                }
            }
        }

        const EN: &[(Key, &str)] = &[$((Key::$variant, $en)),+];
        const ZH_CN: &[(Key, &str)] = &[$((Key::$variant, $zh)),+];
    };
}

// --------------------------------------------------------------- 域装配

/// 链的终点：把累积起来的所有条目交给 [`catalog!`]，生成 `Key` 与两张表。
macro_rules! finish {
    ({ $($acc:tt)* }) => { catalog! { $($acc)* } };
}

mod account;
mod agent;
mod content;
mod core_text;
mod download;
mod font;
mod font_install;
mod general;
mod instances;
mod launch;
mod loader;
mod logs;
mod nav;
mod settings;
mod update;

/// 拆分前的 `key -> (en, zh-CN)` **逐字快照**（金表）。
///
/// ## 它是什么
///
/// M99 把 `catalog.rs` 的单张条目表按域拆到 `catalog/<域>.rs` 之后，这 72 行就是「拆分前那一份」
/// 的原样导出：由拆分前的 `Catalog::of(En)` / `Catalog::of(ZhCn)` 逐条 `println!` 出来，
/// 再原封不动抄进来（生成方式见 `tests.rs` 里 `split_keeps_every_translation` 的注释）。
/// M108 在它上面改了 3 条（`NavDownload` / `DownloadTitle` / `DownloadSubtitle`）、追加了 1 条
/// （`NavDownloadExpanded`）；M105/M106/M107 各自追加的条目也都在，整表现在是 138 条。
///
/// ## 为什么不是「数条数」
///
/// 条数相等抓不住任何一种真会发生的坏法：某条译文在搬移时被漏掉、另一条被复制两次，条数照旧。
/// 这张表比的是**全量、逐字节、按声明顺序**的三元组，于是下面四种坏法都会红：
///
/// | 坏法 | 红在哪 |
/// |---|---|
/// | 某个 key 整条被搬丢 | 条数不等 / 该行 key 名对不上 |
/// | 两条 key 的译文互抄（同 key 被覆盖到别的译文） | 该行 en 或 zh 不等 |
/// | 某条译文改了一个字符 | 该行不等（`assert_eq!` 带原文） |
/// | 域的顺序被调换（`Key` 的 `Ord` 与导出表顺序都靠它） | 该行 key 名对不上 |
///
/// **抓不住**：译文本身翻得对不对（金表是「拆分前」的快照，它只能证明「没变」）。
///
/// ## M108 更新了三条、新增了一条（版本管理改名）
///
/// 用户令（2026-11-04）把这页的显示名从「下载 / Downloads」改成「版本管理 / Version management」：
/// [`Key::NavDownload`] / [`Key::DownloadTitle`] / [`Key::DownloadSubtitle`]
/// 三条译文按令改掉，并**新增** [`Key::NavDownloadExpanded`]（240 展开态的全标签，
/// 88 轨放不下 18 字符的英文，见 `catalog/nav.rs` 的「两条 key」一节）。
///
/// 其余各行仍逐字节等于拆分前那份快照——也就是说这张表现在证明的是「除这几条按令改过的，
/// 其余一字未动」，不再是「整表一字未动」。
///
/// ## M120 动了实例页那一族（1 条改译文 + 1 条删除 + 28 条新增）
///
/// 实例页的文案以前是英文基线字面量，M120 全搬进 `catalog/instances.rs`。那一族在装配链
/// **第 2 位**（紧跟 nav），所以这批行是**插在中间**的，不是文件末尾追加：
///
/// | 动作 | 条目 |
/// |---|---|
/// | 改译文 | [`Key::InstancesEmptyBody`]（换成详情列真正画的那句） |
/// | 删除 | `InstancesLastPlayed`（M117 加的整句，从未接线；页面是「标签 + 值」的元信息卡） |
/// | 新增 | `InstancesTitle` … `InstancesStageWindow`（页头 / 动作 / 空态 / 元信息标签与值 / 隔离档位 / 运行状态 / 阶段，共 28 条） |
///
/// 位置跟着族走这个事实本身就是判据：金表是 `Key::ALL` 的逐 index 镜像，
/// 「为把新 key 排到文件末尾而去重排别人的族」会让这里整片红。
#[cfg(test)]
mod golden;

#[cfg(test)]
mod tests;

use account::account_entries;
use agent::agent_entries;
use content::content_entries;
use core_text::core_text_entries;
use download::{download_entries, version_entries};
use font::font_entries;
use font_install::font_install_entries;
use general::general_entries;
use instances::instances_entries;
use launch::launch_entries;
use loader::loader_entries;
use logs::logs_entries;
use nav::nav_entries;
use settings::{
    settings_accent_entries, settings_ai_entries, settings_appearance_entries,
    settings_download_entries, settings_general_entries, settings_java_entries,
};
use update::update_entries;

// 装配顺序 = `Key` 的声明顺序。**与拆分前逐条一致**，动它等于动 `Key` 的 `Ord` 与导出表顺序，
// 金表（`catalog/golden.rs`）会红。设置页那五个域按页面上的分组顺序排，中间三个现在是空的。
nav_entries! {
    instances_entries,
    loader_entries,
    launch_entries,
    font_entries,
    settings_general_entries,
    settings_appearance_entries,
    settings_java_entries,
    settings_download_entries,
    settings_ai_entries,
    core_text_entries,
    download_entries,
    logs_entries,
    agent_entries,
    general_entries,
    // M107：版本管理页的侧栏 / 搜索文案**追加在链尾**（金表只追加不重排，理由见
    // `catalog/download.rs` 里 `version_entries` 的文档）。M104 的 `agent_entries` 与 M106 的
    // `general_entries` 已经在 main 上占了链尾，所以本域排在它们**之后**：
    // `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，再 version。
    version_entries,
    // M109：内容库页。与 M107 同一条纪律——本域排在 main 已占住的链尾**之后**，
    // 于是 `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，再 version，最后 content。
    content_entries,
    // M114：账户页。同理**追加在 main 已占住的链尾之后**（先 content 再 account），
    // 金表只追加不重排。
    account_entries,
    // M159：自更新（设置页「更新」分组 + 顶部提示条）。同样只追加在链尾之后。
    update_entries,
    // M168：中文字体下载状态行（设置页 General 组）。同样只追加在链尾之后——
    // `catalog/font.rs` 那四条老文案在链中间，往那里追加会平移其后所有 key（金表整片红）。
    font_install_entries,
    // M250：强调色一族（设置页 Appearance 组的第一个控件）。同理追加在链尾——`appearance`
    // 那个域在链中间，往那里追加会平移其后所有 key（金表整片红）。
    settings_accent_entries,
    finish,
    {}
}

/// 一套语言的目录（不可变；`Copy`，随手传值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Catalog {
    lang: Lang,
    entries: &'static [(Key, &'static str)],
}

impl Catalog {
    /// 内置目录。
    #[must_use]
    pub const fn of(lang: Lang) -> Catalog {
        let entries: &'static [(Key, &'static str)] = match lang {
            Lang::En => EN,
            Lang::ZhCn => ZH_CN,
        };
        Catalog { lang, entries }
    }

    /// 用外部表构造目录（给后续 mission 的第三套语言 / 测试注入用）。
    #[must_use]
    pub const fn from_entries(lang: Lang, entries: &'static [(Key, &'static str)]) -> Catalog {
        Catalog { lang, entries }
    }

    /// 这套目录是哪种语言。
    #[must_use]
    pub const fn lang(&self) -> Lang {
        self.lang
    }

    /// 严格取词：缺译就是 `None`（**不回落**到别的语言）。
    #[must_use]
    pub fn get(&self, key: Key) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|(entry, _)| *entry == key)
            .map(|(_, text)| *text)
    }

    /// 这套目录的全部条目（声明顺序）。给导出 / 校对工具与完整性守门测试用。
    #[must_use]
    pub const fn entries(&self) -> &'static [(Key, &'static str)] {
        self.entries
    }

    /// 取词（UI 便利路径）：缺译返回 [`MISSING`]。
    #[must_use]
    pub fn text(&self, key: Key) -> &'static str {
        self.get(key).unwrap_or(MISSING)
    }

    /// 带变量的句子：把 `{name}` 换掉。
    ///
    /// 参数没给全时**占位符原样留在文案里**（`Launch failed (exit {code})`），不 panic、不猜空串——
    /// 缺参在截图上一眼可见，比静默变成 `exit ` 好查。
    #[must_use]
    pub fn render(&self, key: Key, args: &[(&str, &str)]) -> String {
        let Some(template) = self.get(key) else {
            return format!("⟪missing:{}⟫", key.name());
        };

        let mut out = template.to_owned();
        for (name, value) in args {
            out = out.replace(&format!("{{{name}}}"), value);
        }
        out
    }

    /// 缺译的 key（自带语言里从来是空；注入目录与回归守门才用得上）。
    #[must_use]
    pub fn missing(&self) -> Vec<Key> {
        Key::ALL
            .iter()
            .copied()
            .filter(|key| self.get(*key).is_none())
            .collect()
    }

    /// 人话诊断（`None` = 没有缺译）。一条缺译一行，附带英文原文当参考。
    #[must_use]
    pub fn diagnose(&self) -> Option<String> {
        let missing = self.missing();
        if missing.is_empty() {
            return None;
        }

        let mut out = format!("{} 缺 {} 条译文：", self.lang.tag(), missing.len());
        for key in missing {
            let reference = Catalog::of(Lang::En)
                .get(key)
                .map_or_else(|| "<en 也缺>".to_owned(), |text| format!("\"{text}\""));
            out.push_str(&format!("\n  - {} · en={reference}", key.name()));
        }
        Some(out)
    }
}

/// 抽取文案里的 `{name}` 占位符（守门测试用：同一 key 的两套译文占位符必须一致）。
#[must_use]
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        found.push(&after[..close]);
        rest = &after[close + 1..];
    }
    found
}
