//! iced 侧的 presenter 接线层（M119）。
//!
//! ## 为什么单独一个目录
//!
//! presenter 已经把「AI 会话怎么攒、未读怎么算、YOLO 怎么裁决」做完了（M103），壳层只该
//! **读视图模型 + 把点击翻成四个方法**。这份「只做翻译」的胶水需要地方放，而且要能和 presenter
//! 自己的契约分开审：所以是独立目录，不塞进 `app/` 的 update 分支里。
//!
//! | 文件 | 职责 | 不负责 |
//! |---|---|---|
//! | [`agent`] | [`AgentHost`]：presenter 句柄（四契约）+ 事件队列 + 具名错误 | 画、算脱敏 |
//! | [`transport`] | [`AgentTransport`](unlucky_presenter::agent::AgentTransport) 的 iced 侧实现（脚本化 rig + 三种失败注入） | 真实模型调用 |
//! | `rig`（仅测试） | 账户轨的 `Backend` 实现：`AppCommand` → host 形状的 `CoreEvent` | 真 AccountStore 落盘 |
//!
//! ## 逻辑路径与物理路径
//!
//! 物理目录是 `crates/app-iced/src/presenter/`，逻辑名字是 `crate::app::state::presenter`——由
//! `app/state.rs` 用 `#[path = "../presenter/mod.rs"]` 挂载。理由与 `shell::agent`
//! （`crates/app-iced/src/pages/agent/` → `crate::shell::agent`）完全相同：本 mission 的 scope
//! 不含 `lib.rs` 与 `app/mod.rs`，用 `#[path]` 挂载既不碰共用声明文件，也不产生第二份实现。
//!
//! ## 线程/通道边界（本轮只验同步形状，异步未验）
//!
//! ```text
//! on_agent(Submit) ──► AgentHost::submit ──► Presenter::agent_submit ──► Transport::submit
//!                                                                          │ 事件进队列
//!   右栏（读 rag.agent_rail()）◄── AgentHost::apply ◄── Message::Agent(Events) ◄── App::drain_agent
//! ```
//!
//! 队列是 `Arc<Mutex<VecDeque<..>>>`：**transport 侧只入队、UI 侧只出队**，中间不共享状态。
//! 真实异步后端落地时换掉的是「谁往队列里 push」，这条形状不动。本轮 transport 是同步实现
//! （`submit` 当场把整批事件入队），所以「跨线程时序 / 乱序 / 并发提交」**没有验过**。

pub(crate) mod agent;
// 账户轨的 UI 侧 rig（`AppCommand` → host 形状的 `CoreEvent`）。只在测试里存在：截图通道的
// 账户矩阵仍走 presenter 自带的 `MockBackend`（`ULL_ACCOUNT_STATE`）。
#[cfg(test)]
pub(crate) mod rig;
pub(crate) mod transport;

pub(crate) use agent::{AgentFailure, AgentHost};
