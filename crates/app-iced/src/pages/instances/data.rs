//! 实例页的**演示数据**（骨架期没有业务状态机，也没有业务数据）。
//!
//! 12 条，**故意比可视区高**：列表列的独立滚动条要能在截图里看见（单测钉住这条）。
//!
//! **这里的数据是夹具（用户数据），不是界面文案**：`1.20.4` / `Fabric 0.15.7` / `4 GiB` /
//! `2 hours ago` 这些是「实例清单里会长成什么样」的样本，M120 没有把它们搬进词表——它们不是
//! 界面说的那句话。界面文案（标题 / 按钮 / 元信息标签 / 空态 / 阶段）走 `catalog/instances.rs`，
//! 见 `text.rs` 与 `app/view/instances.rs`。
//!
//! 这条路径（`pages_demo` harness / 单测）**恒英文**：`pages::demo_model` 走 `Model::new`，
//! `lang` 是 `Lang::En`。产品路径不读这份数据，它画的是 presenter 快照。

/// 演示条目（骨架期没有业务数据；夹具文案，见模块文档）。
pub(crate) struct Entry {
    pub(crate) name: &'static str,
    pub(crate) version: &'static str,
    pub(crate) loader: &'static str,
    pub(crate) memory: &'static str,
    pub(crate) java: &'static str,
    pub(crate) mods: &'static str,
    pub(crate) last_played: &'static str,
}

/// 12 条演示数据：**比可视区高**，这样列表列的独立滚动条在截图里能看见。
pub(crate) const DEMO: [Entry; 12] = [
    Entry {
        name: "Fabulous World",
        version: "1.20.4",
        loader: "Fabric 0.15.7",
        memory: "4 GiB",
        java: "Temurin 21",
        mods: "148 mods",
        last_played: "2 hours ago",
    },
    Entry {
        name: "Skyblock Origins",
        version: "1.21.1",
        loader: "NeoForge 21.1.72",
        memory: "6 GiB",
        java: "Temurin 21",
        mods: "86 mods",
        last_played: "yesterday",
    },
    Entry {
        name: "Vanilla Survival",
        version: "1.21.4",
        loader: "Vanilla",
        memory: "2 GiB",
        java: "Temurin 21",
        mods: "0 mods",
        last_played: "3 days ago",
    },
    Entry {
        name: "Create: Above and Beyond",
        version: "1.18.2",
        loader: "Forge 40.2.0",
        memory: "8 GiB",
        java: "Temurin 17",
        mods: "312 mods",
        last_played: "last week",
    },
    Entry {
        name: "All the Mods 9",
        version: "1.20.1",
        loader: "NeoForge 20.4.237",
        memory: "8 GiB",
        java: "Temurin 17",
        mods: "412 mods",
        last_played: "2 weeks ago",
    },
    Entry {
        name: "Better MC",
        version: "1.20.1",
        loader: "Fabric 0.14.24",
        memory: "6 GiB",
        java: "Temurin 17",
        mods: "264 mods",
        last_played: "last month",
    },
    Entry {
        name: "RLCraft",
        version: "1.12.2",
        loader: "Forge 14.23.5",
        memory: "4 GiB",
        java: "Zulu 8",
        mods: "184 mods",
        last_played: "2 months ago",
    },
    Entry {
        name: "Origins SMP",
        version: "1.20.4",
        loader: "Quilt 0.24.0",
        memory: "4 GiB",
        java: "Temurin 21",
        mods: "57 mods",
        last_played: "3 months ago",
    },
    Entry {
        name: "Redstone Lab",
        version: "1.19.4",
        loader: "Fabric 0.15.2",
        memory: "3 GiB",
        java: "Temurin 17",
        mods: "34 mods",
        last_played: "April 12",
    },
    Entry {
        name: "Pixelmon Reforged",
        version: "1.16.5",
        loader: "Forge 36.2.34",
        memory: "6 GiB",
        java: "Temurin 8",
        mods: "121 mods",
        last_played: "March 3",
    },
    Entry {
        name: "Techopolis 2",
        version: "1.20.1",
        loader: "Forge 47.2.20",
        memory: "8 GiB",
        java: "Temurin 17",
        mods: "198 mods",
        last_played: "January 28",
    },
    Entry {
        name: "Cobblemon",
        version: "1.21.1",
        loader: "Fabric 0.16.9",
        memory: "5 GiB",
        java: "Temurin 21",
        mods: "74 mods",
        last_played: "January 9",
    },
];

/// 演示选中的条目（第一条）。
pub(crate) const SELECTED: usize = 0;

/// 详情正文（**故意写长**：这是行宽上限的视觉证据——1180 宽窗口下也只断在 ~72 字符）。
pub(crate) const NOTE: &str = "This instance keeps its own mods, configs and saves. The launcher resolves the version chain \
     (instance → mod loader → vanilla) at launch time, verifies every jar it downloaded against the mirror \
     manifest, and streams progress back as structured events so the UI never has to guess. Nothing on this \
     page is wired to real data yet: the layout is, the numbers are.";
