//! Elm 三件套：`State` / `Message` / `update` + `view` 接线，以及启动序列。
//!
//! 启动序列：①探测渲染后端并写 `ICED_BACKEND`（必须在 `run()` 之前，M60 Q1）→ ②求值字体栈
//! （D28 前半）→ ③`iced::application(..).run()`（1180×760、无边框自绘标题栏）→ ④首个 `view()`
//! 打耗时；截图模式第 3 帧落盘后退出（无边框窗首帧可能是旧尺寸，M60 坑 #5）。
//!
//! ## 为什么拆成目录（M90）
//!
//! 本文件原来是**单个 400 行的 `app.rs`**：正好卡在「单文件 ≤400 行、动它前必须先拆」的硬门槛上
//! （AGENTS 的文件上限 + 本 mission 的第一条任务）。拆法与职责：
//!
//! | 文件 | 职责（**只做这一件事**） |
//! |---|---|
//! | [`state`] | [`App`] 字段 + 构造（presenter 三件在此持有）+ 主题切换 |
//! | [`message`] | [`Message`] 枚举与「导航项 → 路由」的翻译 |
//! | [`update`] | 全部 `Message` 分支 + 「用户动作 → `AppCommand`」的派发与事件回流 |
//! | [`subscription`] | 帧事件（截图）与 presenter 的 TICK 节拍 |
//! | [`view`] | `view` / `theme` / `title` |
//! | [`boot`] | [`run`]：后端探测 → 字体 → `iced::application(..)` |
//!
//! 拆分的每一步都保持**可观察行为不变**（截图四件套 `--screenshot` / `--screenshot-theme` /
//! `--screen=gallery` / `--diff` 出图逐字节相同），presenter 接线是在拆分**之后**另一步做的。
//!
//! ## 组件库为什么还在本模块下声明
//!
//! `src/lib.rs` 是五条组件线共用的文件，改它必然冲突；`#[path]` 保证物理路径仍是
//! `src/components/`（组件目录自己的归属规则不变），逻辑路径仍是 `crate::app::components`
//! （组件源码里的文档与测试都按这个路径写）。

mod appearance;
mod backend;
mod boot;
mod message;
mod state;
mod subscription;
mod update;
mod view;
mod window_alpha;

#[cfg(test)]
mod tests;

pub(crate) use boot::run;
pub use message::Message;

// 组件库（D39）在本文件声明：`src/lib.rs` 是五条组件线共用的文件，改它必然冲突；
// `#[path]` 保证物理路径仍是 `src/components/`（组件目录自己的归属规则不变）。
//
// M90：可见性从 `mod` 放到 `pub(crate) mod` —— 页面层（`src/pages/**`）要**用**真组件
// （`components::buttons` / `navigation` / `divider`），而 `app` 不是它的父模块。
// 只放开 crate 内可见，组件目录的归属规则一字不改。
#[path = "../components/mod.rs"]
pub(crate) mod components;

/// 验收窗口尺寸（1180×760 @100% DPI —— D30 的硬要求）。
pub const WINDOW_SIZE: iced::Size = iced::Size {
    width: 1180.0,
    height: 760.0,
};

/// 产品名（自绘标题栏的标题；`'static` 好让 view 借用）。
pub const APP_TITLE: &str = "Unlucky Launcher";

/// 窗口最小尺寸（再小骨架会挤坏：88 导航 + 页面留白 + 卡片）。
const MIN_SIZE: iced::Size = iced::Size {
    width: 880.0,
    height: 560.0,
};

/// 截图落盘的帧号（见模块文档）。
const CAPTURE_AT_FRAME: u32 = 3;
