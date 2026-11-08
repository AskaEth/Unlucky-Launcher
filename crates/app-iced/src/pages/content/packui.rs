//! 整合包**导入 / 导出**区（M113 `PACKUI`）：内容库页里「把我的包装进来」与「把我的实例打出去」。
//!
//! ## 为什么长在内容库页上
//!
//! 用户令：「下载页只负责下载整合包和Mod/光影」+「支持当前主流的整合包格式导入导出的功能」。
//! 下载页（+内容库）是「拿内容进来」的地方，所以：
//!
//! - **导入**（本地包 → 实例）：入口在这里，和「从站点拿整合包」摆在同一页；
//! - **导出**（实例 → 包）：同一块，选目标实例 + 格式，预检之后再写盘。
//!
//! ## 三种格式都是 core 的既有能力，本模块只做界面
//!
//! | 格式 | 导入 | 导出 | 界面上的如实说明 |
//! |---|---|---|---|
//! | CurseForge `manifest.json` | 是（`files[]` 远程补全**未接通**） | 是 | 缺 CF 客户端 ⇒ 那些条目只能记成「没搞到」，逐条列出，**绝不猜 URL** |
//! | Modrinth `.mrpack` | 是 | 是 | —— |
//! | MultiMC / Prism 实例目录 | 是 | **否**（core 有意省略） | 文案明写「只能导入、不能导出」 |
//!
//! `.disabled` 是**路径后缀**（不是 `env.client=optional`）——这条在导出预检的逐条原因里出现，
//! 界面上不解释「可选依赖」，因为那不是同一件事。
//!
//! ## 文件分工
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`message`] | 本区的意图（挂在 `content::Message::Pack` 下） |
//! | [`model`] | 视图模型（快照 → 要画的东西；纯决策可单测） |
//! | [`view`] | 画法（进度**整块**复用下载页的安装面板） |
//! | `demo` | 出图夹具（`ULL_CONTENT_PACK` 选态） |
//!
//! ## M160 新增的一屏
//!
//! `model/pick.rs` + `view/pick.rs` 是**导入前的冲突清单**：分页（每页 `PAGE_SIZE` 条）、
//! 三个入口、计数摘要、确认/取消。它出现的前提是 core 的预检给出了至少一条冲突——
//! 没有冲突时页面连这一屏都不画（直接往下导）。
//!
//! ## 本版明确的边界（报告里同样登记）
//!
//! - **没有系统文件选择框**：依赖树里没有 `rfd` 一类的对话框依赖，加它要动根 `Cargo.toml`、
//!   过许可门禁、还要给 Linux 选后端（GTK / xdg-portal），直接影响「单文件产物」。所以入口是
//!   **拖拽（真能用）+ 路径输入框**，并且把原因写在界面上（[`model::Pack::no_dialog_note`]）。
//! - **冲突是逐文件决策了（M160）**：点「导入」先跑一次**只读预检**（core 的 `scan_pack`），
//!   没有冲突就直接导（不打扰用户）；有冲突就画那一屏清单——分页（[`model::PAGE_SIZE`] 条一页）、
//!   「全部保留 / 全部覆盖 / 逐项」三个入口、计数摘要、确认与取消。**缺省永远是保留**：
//!   没点过的文件一个都不会被包内内容替换。仍然不支持的是「两者都留（改名）」——那要改落盘
//!   布局，不在本轮。

mod demo;
mod message;
mod model;
mod view;

#[cfg(test)]
mod tests;

pub use message::Message as PackMessage;
pub use model::{request_export, Pack, PackPageState};

/// 区入口（内容库页的正列调它）。
pub(crate) use view::section;

/// 出图夹具：按 `ULL_CONTENT_PACK` 选态（`importing` / `partial` / `conflict` / `export`）。
pub(crate) fn demo_pack(lang: unlucky_i18n::Lang) -> (Pack, PackPageState) {
    demo::pack(lang)
}
