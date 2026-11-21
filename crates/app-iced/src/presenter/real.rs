//! 真模型 transport（M384 / D60）：把 `unlucky_agent` 的会话接进 AI 助手栏。
//!
//! ## 它替掉的是什么
//!
//! M119 起 [`super::transport::RigTransport`] 是 [`AgentTransport`] 的**唯一**实现：脚本化回放，
//! 没有模型，工具内核在 GUI 上根本不可达（`DevDocs/artifacts/gapreg/GAPS.md` §2.7 A1/A2）。
//! 本模块把缺的那一段接上：
//!
//! ```text
//! AgentHost ──► Presenter ──► RealTransport（本文件）
//!                                 │ Job{text, base}      经 mpsc 交给后台线程
//!                                 ▼
//!                           worker 线程（活的 Session）
//!                                 ├─ ports：unlucky_cli::commands::agent::ports::build（零改动复用）
//!                                 ├─ registry：unlucky_agent::tools::build_v1_registry（十个工具）
//!                                 ├─ Brain：流式增量 / 工具编排 / 步数上限
//!                                 ├─ JsonlAudit：<data>/agent/audit/audit-<date>.jsonl
//!                                 └─ ConfirmGate：本文件的纯 std future（等 UI 点「允许/拒绝」）
//!                                 │ AgentEvent（agent 侧）
//!                                 ▼
//!                           Mapper（本文件）──► AppEvent（presenter dto）──► EventQueue ──► UI
//! ```
//!
//! ## 三条设计约束
//!
//! 1. **不上引 tokio / 不阻塞 UI**：异步只发生在 worker 线程里，驱动用 core 的进程级阻塞桥
//!    （`unlucky_core::download::blocking::block_on`，与「测试连接」同一个运行时）。确认闸门是
//!    **手写 poll 的纯 std future**——GUI 主线程只需要 `Sender::send` 一下。
//! 2. **presenter 保持零 `rig` / `tokio` / `rmcp`**：装配与翻译都在本 crate（`rail.rs:54` 那条禁令不动）。
//! 3. **失败必须是人话 + 不带密钥**：provider 的错误原文过
//!    [`unlucky_cli::commands::agent::probe::scrub`]（主目录 / Bearer / **本次密钥原文** → 掩码，
//!    截断 400 字），再由 presenter 的 `scrub_text` 过第二道。
//!
//! ## 显式降级（不许静默）
//!
//! 没配好（`ai.enabled=false` / 缺端点 / 缺模型 / Anthropic 缺密钥）时**不接真后端**：
//! [`Plan::resolve`] 返回具体缺哪一项，[`super::agent::AgentHost::from_env`] 据此退回脚本化
//! rig，并挂一条**具名**横幅（[`super::AgentFailure::NotConfigured`]）+ 一行 stderr 日志。
//! 界面侧本来就有四处「模拟助手 · 还没接真实模型」的常驻文案（`simulated = true`），
//! 所以用户看到的是「两条明说的路」，不是一条死路。
//!
//! 判据用 [`unlucky_cli::commands::agent::probe::AiSettings::to_config`]——**与设置页
//! 「测试连接」按钮同一条**，不是「有没有 api_key」：本地 llama.cpp / Ollama 本来就不要密钥
//! （`ProviderKind::requires_api_key` 只对 Anthropic 为真），拿「有 key」当判据会把本地模型用户
//! 挡在门外。
//!
//! ## 两种「非回答」的收场（M388）：超时与停止
//!
//! M384 把真 provider 接进桌面之后，这一轮发出去就**没法停**：provider 的 await 没有时限，
//! 而界面上的「停止」走的是 `AppCommand::StopAgent` → `MockBackend` 的空臂，与 worker 线程
//! 毫无关系。M388 补的两条路：
//!
//! | 档 | 谁判 | 谁收尾 | 产物 |
//! |---|---|---|---|
//! | **超时** | 内核（`unlucky_agent::brain::guard` 的空闲时限） | worker 报事实 → UI 按语言渲染结论 | `ConclusionSlot` + 一条系统条目 + 审计 `timeout` |
//! | **停止** | 用户 | UI **当场**收尾（不等 worker 回声） | 一条系统条目 + 审计 `cancelled` |
//!
//! 取消的三步顺序是承重的：**先静音映射器**（[`Mapper::mute`]）→ 再置取消旗标（内核在 await 处
//! 析构请求 future）→ UI 收尾。只做前一半是「丢弃不是中断」，只做后一半会让最后几段正文冒到
//! 「已取消」下面。**能打断的只有「等 provider」那一段**：内核停在工具执行或等确认时，旗标要等
//! 那一段走完才被看见——那一段是**丢弃**，报告里按这个口径写。
//!
//! ## 已知边界（写进报告「没验的部分」）
//!
//! - **YOLO 是活开关（M389）**：`SessionConfig` 里塞的是「每次判定现读 `launcher.json` 的
//!   `ai.yolo`」的探针（[`Plan::yolo_switch`]），所以设置页改完**同一条会话**的下一次高敏感写
//!   就按新值判。判据是「那一次调用自己的决策时刻」：翻开关不追溯已经决定的那一笔；
//! - **没有真 key ⇒ 真实模型往返未验**：本模块验到的是「配置→provider 构造」「错误路径」
//!   「工具编排与审计」三段，第一段到模型的网络上没有走过；
//! - **单轮并发**：worker 串行处理 Job。GUI 在 `busy` 期间禁用输入，所以正常情况下排不上队；
//!   真并发提交未验（presenter 的 `agent_submit` 会拒）；
//! - **打断的粒度是一个 await**：内核停在同步端口调用（工具执行）或停在确认闸门上时，取消旗标
//!   要等那一段结束才生效。M388 的图只证了「等 provider 时能立刻打断」这一档。

pub(crate) mod confirm;
pub(crate) mod mapper;
pub(crate) mod plan;
pub(crate) mod worker;

pub(crate) use confirm::{ConfirmBridge, DecisionSlot, DecisionState};
pub(crate) use mapper::{Mapper, MapperSink};
pub(crate) use plan::Plan;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use unlucky_agent::brain::rig::RigProvider;
use unlucky_agent::brain::ProviderFactory;
use unlucky_agent::confirm::ConfirmGate;
use unlucky_agent::event::AgentEventSink;
use unlucky_agent::port::Ports;
use unlucky_presenter::agent::{AgentRail, AgentTransport};
use unlucky_presenter::dto::AppEvent;

use unlucky_cli::commands::agent::ports;
use unlucky_cli::commands::agent::probe::Missing;
use unlucky_cli::context::Ctx;

use super::transport::{CancelSlot, ConclusionSlot, EventQueue, FailureSlot};
use super::AgentFailure;
use worker::{run_worker, Worker};

/// 一次提交（交给 worker 线程的最小载荷）。
struct Job {
    /// 已经过 presenter 出站脱敏的正文。
    text: String,
    /// 本轮第一条消息在 rail 上的下标（`rail.messages.len()`，提交那一刻取的）。
    ///
    /// 为什么在提交时算：`MessageDelta` 靠下标寻址（`AgentRail::ingest`），而 worker 线程
    /// 拿不到 rail。下标只在尾部追加时稳定，正好够用。
    base: usize,
}

/// 真模型 transport：前端只碰两个 `Arc<Mutex<..>>` 与一条 mpsc。
pub(crate) struct RealTransport {
    queue: EventQueue,
    failure: FailureSlot,
    jobs: Sender<Job>,
    decision: DecisionSlot,
    /// 事件映射器（「停止」要在这里**静音**：worker 还在 socket 上时，它后面吐的每一段正文
    /// 都属于用户已经停掉的那一轮）。
    mapper: Arc<Mutex<Mapper>>,
    /// 内核的取消旗标槽（会话在 worker 线程里装配，所以是「先开槽、装配后填」）。
    cancel: CancelSlot,
    /// 后端报上来的收场事实（目前只有超时；句子由 UI 按当前语言渲染）。
    conclusion: ConclusionSlot,
}

impl RealTransport {
    /// 按「现在这一份」设置装配真后端。
    ///
    /// # Errors
    /// 配置不完整时返回缺哪一项（调用方据此退回脚本化 rig + 具名横幅）。
    pub(crate) fn open() -> Result<Self, Missing> {
        Ok(Self::from_plan(Plan::resolve()?))
    }

    /// 按一份**已经解析好**的设置快照装配（生产端口 + rig provider）。
    ///
    /// 两个用途：①[`Self::open`] 的正常路径；②用例把 `launcher.json` 钉在临时目录里时，
    /// 走 [`Plan::at`] → 这里，避免测试去碰用户真实数据根里的 AI 四项。
    pub(crate) fn from_plan(plan: Plan) -> Self {
        Self::spawn(plan, real_ports(), RigProvider::factory())
    }

    /// 与 UI 共享的事件队列（形状与 [`super::transport::RigTransport`] 相同）。
    pub(crate) fn queue(&self) -> EventQueue {
        Arc::clone(&self.queue)
    }

    /// 与 UI 共享的失败原因槽。
    pub(crate) fn failure(&self) -> FailureSlot {
        Arc::clone(&self.failure)
    }

    /// 与 UI 共享的「收场事实」槽（超时由 worker 写、UI 取走后按语言渲染）。
    pub(crate) fn conclusion(&self) -> ConclusionSlot {
        Arc::clone(&self.conclusion)
    }

    /// 起 worker 线程。`ports` / `factory` 是注入点：生产是 core 端口 + rig provider，
    /// 测试是临时数据根的端口 + 回放 provider（**不用网络就能跑完整条链路**）。
    fn spawn(plan: Plan, ports: Ports, factory: ProviderFactory) -> Self {
        let queue = EventQueue::default();
        let failure = FailureSlot::default();
        let decision: DecisionSlot = Arc::new(Mutex::new(DecisionState::default()));
        let cancel: CancelSlot = Arc::new(Mutex::new(None));
        let conclusion: ConclusionSlot = Arc::new(Mutex::new(None));
        let (jobs, intake) = std::sync::mpsc::channel::<Job>();
        let scrub = plan.scrubber();
        let mapper = Arc::new(Mutex::new(Mapper::new(queue.clone(), scrub.clone())));
        let gate: Arc<dyn ConfirmGate> = Arc::new(ConfirmBridge {
            queue: queue.clone(),
            decision: Arc::clone(&decision),
            scrub,
        });

        let sink: Arc<dyn AgentEventSink> = Arc::new(MapperSink {
            mapper: Arc::clone(&mapper),
        });
        let ui_mapper = Arc::clone(&mapper);
        let worker_failure = Arc::clone(&failure);
        let worker_cancel = Arc::clone(&cancel);
        let worker_conclusion = Arc::clone(&conclusion);
        // 线程起不来时 `jobs` 立刻断开，`submit` 会返回具名失败（不静默）。
        let spawned = std::thread::Builder::new()
            .name("ull-agent-brain".to_string())
            .spawn(move || {
                run_worker(
                    &plan,
                    &ports,
                    &factory,
                    gate,
                    sink,
                    Worker {
                        mapper,
                        failure: worker_failure,
                        cancel: worker_cancel,
                        conclusion: worker_conclusion,
                    },
                    intake,
                );
            });
        if let Err(error) = spawned {
            eprintln!("[agent] 起不了会话线程：{error}");
        }
        Self {
            queue,
            failure,
            jobs,
            decision,
            mapper: ui_mapper,
            cancel,
            conclusion,
        }
    }
}

impl AgentTransport for RealTransport {
    fn submit(&mut self, text: &str, history: &AgentRail) -> Result<Vec<AppEvent>, String> {
        let job = Job {
            text: text.to_string(),
            base: history.messages.len(),
        };
        match self.jobs.send(job) {
            Ok(()) => Ok(Vec::new()),
            Err(_) => {
                // 会话线程已经没了（起不来 / 崩了）：具名失败，别让输入框空转。
                if let Ok(mut slot) = self.failure.lock() {
                    *slot = Some(AgentFailure::Unreachable);
                }
                Err("assistant worker thread is gone".to_string())
            }
        }
    }

    /// 真后端的事件由 worker 线程推、下一拍从队列回流 ⇒ **异步**后端。
    ///
    /// 这条申报是 M384 r2 复审 P2-1 的承重墙：不申报的话 `agent_submit` 收尾的
    /// `settle_streaming()` 会把 `begin_turn()` 的 `busy` 立刻灭掉，而真后端的第一个 token
    /// 可能要等几百毫秒到几秒——那段时间里发送键可用，再按一次 Enter 就会两轮并行
    /// （第二轮用旧的 `rail.messages.len()` 当 `base`，`MessageDelta` 会指到上一轮的气泡上）。
    fn is_async(&self) -> bool {
        true
    }

    fn confirm(&mut self, request_id: &str, accept: bool) -> Vec<AppEvent> {
        // 事件不由这里产生：裁决交给等着的 future，工具收尾事件由 agent 侧的编排层
        // 经 sink 回流（与 `RigTransport` 的分工相反，因为这里真的有一个在跑的调用）。
        let decided = self
            .decision
            .lock()
            .map(|mut state| state.decide(request_id, accept))
            .unwrap_or(false);
        if !decided {
            eprintln!("[agent] 确认裁决的请求 id 对不上（{request_id}），已忽略");
        }
        Vec::new()
    }

    /// 「停止」（M388）：**先静音映射器，再置取消旗标**。
    ///
    /// 这个顺序是承重的：静音保证「接下来发生什么都进不了会话」，置旗标让内核在 await 处析构请求
    /// future（reqwest 的连接随之关闭）。只做前一半 = 「丢弃不是中断」；只做后一半 = 内核响应
    /// 不及时时，最后几段正文仍会冒到「已取消」下面。
    ///
    /// 返回 `true` 只在**内核真的在跑**时（会话已装配好、旗标槽里有句柄）。内核可能正停在工具执行
    /// 或等确认上——那一段不读旗标，最坏要等它走完；本函数如实说「信号已发」，不说「已停下」。
    fn cancel(&mut self) -> bool {
        let dropped = self
            .mapper
            .lock()
            .map(|mut mapper| mapper.mute())
            .unwrap_or(0);
        let token = self.cancel.lock().ok().and_then(|slot| slot.clone());
        let Some(token) = token else {
            eprintln!("[agent] 停止：内核还没装配好（没有可打断的调用）⇒ 本轮只丢弃");
            return false;
        };
        token.cancel();
        eprintln!(
            "[agent] 停止：已静音映射器（此前已丢弃 {dropped} 条）+ 已置取消旗标\
             （内核在 await 处析构请求 future）"
        );
        true
    }
}

impl std::fmt::Debug for RealTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RealTransport")
            .field("pending_jobs", &self.jobs)
            .finish_non_exhaustive()
    }
}

/// 生产端口：数据根由 cli 的 `Ctx` 解析，端口实现复用 `ull agent serve` 那一套（零改动）。
fn real_ports() -> Ports {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    match Ctx::resolve_in(&unlucky_cli::cli::Globals::default(), exe_dir.clone()) {
        Ok(ctx) => ports::build(&ctx),
        Err(error) => {
            // 数据根都解析不出来（极端情况）：拿一份只读的兜底端口，至少让工具不是"消失"。
            eprintln!("[agent] 数据根解析失败（{error}），端口退回空数据根");
            ports::build(
                &Ctx::resolve_in(
                    &unlucky_cli::cli::Globals {
                        data_dir: Some(std::env::temp_dir()),
                        ..unlucky_cli::cli::Globals::default()
                    },
                    exe_dir,
                )
                .expect("临时数据根必须能解析"),
            )
        }
    }
}

#[cfg(test)]
mod tests;
