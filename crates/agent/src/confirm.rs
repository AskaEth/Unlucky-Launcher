//! 分级确认协议（02 文档 §3.1「分级确认」+ §4b「YOLO」）。
//!
//! 分工：
//! - **工具**只声明「这次调用需要确认」并给出 diff（[`ConfirmRequest`]），不自己做决定；
//! - **编排层**（[`crate::brain::exec`]）拿 [`YoloSwitch`] 决定是问人、还是走 YOLO；
//! - **UI** 在 P1 通过 [`ConfirmGate`] 接进来（P0 只有事件 + fail-closed 的默认实现）。
//!
//! ## YOLO 的语义边界（M389，承重）
//!
//! **YOLO 只跳过「提问」，不改变「答案」。** 它唯一的作用是让
//! [`YoloSwitch::requires_confirmation`] 对 `High` 返回 `false`——「这一次不问人」。它
//! **不得**把任何人的「拒绝」翻成放行（presenter 的 `agent_confirm` 有此前的反例，M389 一并改了）。
//! 同样不变的还有两条：**能改的键没变少**（白名单照旧、凭据类键永远锁定），
//! **审计照记**（`yolo=true, confirmed=false` 是自动放行，与用户亲手点的允许分开记）。
//!
//! fail-closed 是刻意的：P0 没有确认弹窗，敏感键的写入**一律拒绝**而不是静默放行
//! （research/02 §7 R9「写操作的破坏半径」）。

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 敏感等级。**与 M12 `setting::spec::Sensitivity` 逐字一致**
/// （`DevDocs/SubAgentsTalk/setting-接口约定.md` §8）：
///
/// | 等级 | 语义 |
/// |------|------|
/// | `Low` | 静默生效 |
/// | `Medium` | 静默生效，审计里能查到（每次调用本来都审计，这里只是标记用途） |
/// | `High` | 必须用户确认（YOLO 可跳过确认，但**仍受白名单约束**） |
///
/// 「AI 完全不能写」的键（`ai.api_key` 等凭据）由 M12 的 `writable=false` 表达，
/// 它们**不进白名单**，agent 侧走「不在白名单」的拒绝分支，不需要第四级。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    /// 静默生效。
    #[default]
    Low,
    /// 静默生效但审计标记（下载源、语言、日志级别）。
    Medium,
    /// 必须用户确认（数据目录、Java 路径、代理、AI 端点/YOLO 开关）。
    High,
}

/// 一个待确认的操作：给用户看的措辞 + 结构化细节（UI 画 diff 用）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConfirmRequest {
    /// 请求 id（编排层生成，UI 回填决定时原样带回）。
    pub id: String,
    /// 工具名。
    pub tool: String,
    /// 一句话请求（「AI 请求把 `instance.java.path` 改为 `D:\jdk17`」）。
    pub summary: String,
    /// 结构化细节（`{"path":…, "old":…, "new":…}`）。
    pub details: Value,
    /// 敏感等级。
    pub sensitivity: Sensitivity,
}

/// 用户决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmDecision {
    /// 允许这一次。
    Approved,
    /// 拒绝（工具调用返回「用户拒绝」给模型）。
    Denied,
    /// 允许，且本会话后续不再询问（UI 的「全部允许」）。
    ///
    /// 编排层当前**只当普通放行**处理（`brain/exec.rs` 不区分两者）；「据此临时开 YOLO」
    /// 是 P1 的行为，落地前不要按已实现来看。
    ApprovedAll,
}

impl ConfirmDecision {
    /// 是否放行（`Approved` 与 `ApprovedAll` 都放行）。
    #[must_use]
    pub fn is_approved(self) -> bool {
        matches!(self, Self::Approved | Self::ApprovedAll)
    }
}

/// 确认闸门：P1 由 app 的 Slint 弹窗实现；测试用 [`ScriptedGate`]。
pub trait ConfirmGate: Send + Sync {
    /// 发起确认并等待决定（UI 未接时由实现方立即给出 fail-closed 结果）。
    fn request(&self, request: ConfirmRequest) -> ConfirmFuture;
}

/// 确认未来的箱子类型。
pub type ConfirmFuture = Pin<Box<dyn Future<Output = ConfirmDecision> + Send + 'static>>;

/// 一律放行（YOLO 单测 / 未来"用户已信任该工具"场景）。
#[derive(Debug, Default, Clone, Copy)]
pub struct AutoApproveGate;

impl ConfirmGate for AutoApproveGate {
    fn request(&self, _request: ConfirmRequest) -> ConfirmFuture {
        Box::pin(async { ConfirmDecision::Approved })
    }
}

/// 一律拒绝（fail-closed 的极简形态）。
#[derive(Debug, Default, Clone, Copy)]
pub struct AutoDenyGate;

impl ConfirmGate for AutoDenyGate {
    fn request(&self, _request: ConfirmRequest) -> ConfirmFuture {
        Box::pin(async { ConfirmDecision::Denied })
    }
}

/// P0 默认闸门：**没有确认 UI** → 一律拒绝（fail-closed）。
///
/// 请求本身已经由编排层作为 [`crate::event::AgentEvent::ConfirmRequested`] 发出去了，
/// 所以 P0 的行为是「敏感操作既不静默执行，也不卡住对话」：事件流里留下了
/// 「AI 想改 X」的完整记录，P1 接上弹窗后同一个事件直接变成可点的确认框。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoUiGate;

impl ConfirmGate for NoUiGate {
    fn request(&self, _request: ConfirmRequest) -> ConfirmFuture {
        Box::pin(async { ConfirmDecision::Denied })
    }
}

/// 脚本化闸门：按顺序返回预设决定，用完之后走 `default`（测试用）。
pub struct ScriptedGate {
    decisions: Mutex<VecDeque<ConfirmDecision>>,
    default: ConfirmDecision,
}

impl ScriptedGate {
    /// 依次给出 `decisions`，耗尽后返回 `default`。
    #[must_use]
    pub fn new(
        decisions: impl IntoIterator<Item = ConfirmDecision>,
        default: ConfirmDecision,
    ) -> Self {
        Self {
            decisions: Mutex::new(decisions.into_iter().collect()),
            default,
        }
    }
}

impl ConfirmGate for ScriptedGate {
    fn request(&self, _request: ConfirmRequest) -> ConfirmFuture {
        let decision = self
            .decisions
            .lock()
            .ok()
            .and_then(|mut q| q.pop_front())
            .unwrap_or(self.default);
        Box::pin(async move { decision })
    }
}

/// 确认策略的**装配期输入**（设置里那个 `ai.yolo` 的值）。
///
/// M389 起它**不再是运行期判据**：判据是 [`ConfirmPolicy::switch`] 造出来的 [`YoloSwitch`]，
/// 生产装配点（GUI / MCP）给的是「现读设置文件」的活开关。
///
/// 保留成 `{ pub yolo: bool }` 是有意的：`crates/agent/tests/support` 用结构体字面量装配夹具
/// （那个文件不在本 mission 的 scope），而 `switch()` 让这个旧形状变成 [`YoloSwitch::fixed`]。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfirmPolicy {
    /// 自动批准全部工具调用（设置 → AI → YOLO，默认关）。
    pub yolo: bool,
}

/// YOLO 的**活开关**（M389）：判定时**现问一次**，而不是装配期抄一份。
///
/// ## 为什么是「现问文件」而不是共享的 `Arc<AtomicBool>`
///
/// 真相在 `config/launcher.json` 里，而写它的有**四个**互不相识的写者：设置页的 `ai.yolo` 行、
/// `ull config set ai.yolo …`、外部编辑器、以及 AI 自己的 `set_config("ai.yolo")`。
/// 一个「UI 推一下」的句柄只能在其中一条路径上保鲜，其余几条照样让开关说假话；MCP server
/// 更是另一个进程，句柄共享不到。**文件是这些写者唯一的汇合点**，所以判据也落在文件上。
///
/// ## 语义边界（承重）
///
/// **YOLO 只跳过「提问」，不改变「答案」**：它只决定 `requires_confirmation` 返回什么
/// （要不要问人），**不会**把任何人的「拒绝」翻成放行。
///
/// ## 线程安全
///
/// 只读探针，**没有共享可变状态**：worker 线程（工具编排）与 UI 线程（写设置）各自独立，
/// 写盘走 `SettingStore::save` 的原子替换，读者看不到半个文件。
#[derive(Clone)]
pub struct YoloSwitch {
    probe: Arc<dyn Fn() -> bool + Send + Sync>,
    /// 是否「现问」（只给 `Debug` 用：`Debug` 不许顺手读盘）。
    live: bool,
}

impl YoloSwitch {
    /// 固定值（测试、以及「一次运行一个值」的场景）。**不改就是快照**，与 M389 之前等价。
    #[must_use]
    pub fn fixed(yolo: bool) -> Self {
        Self {
            probe: Arc::new(move || yolo),
            live: false,
        }
    }

    /// 现读探针：每次判定都调一次 `read`。**读不到必须返回 `false`（fail-closed）**——
    /// 配置文件损坏/被锁时按「关」处理（会弹确认），绝不拿一个旧的 `true` 兜底。
    #[must_use]
    pub fn live(read: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            probe: Arc::new(read),
            live: true,
        }
    }

    /// 此刻该不该自动放行（这一次调用就是一次读）。
    #[must_use]
    pub fn get(&self) -> bool {
        (self.probe)()
    }

    /// 该等级是否需要**询问**用户：只有 `High` 会问；YOLO 下连 `High` 也不问
    /// （02 文档 §4b：YOLO 跳过分级确认，但**仍受白名单约束**——能改的键没变少）。
    #[must_use]
    pub fn requires_confirmation(&self, sensitivity: Sensitivity) -> bool {
        !self.get() && sensitivity == Sensitivity::High
    }

    /// 该等级是否可以直接执行（不需要询问）。
    #[must_use]
    pub fn auto_approves(&self, sensitivity: Sensitivity) -> bool {
        !self.requires_confirmation(sensitivity)
    }
}

impl Default for YoloSwitch {
    fn default() -> Self {
        Self::fixed(false)
    }
}

impl fmt::Debug for YoloSwitch {
    /// **不调用探针**：`Debug` 出现在日志里，读盘不该是它的副作用。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.live {
            "YoloSwitch::live(<现读>)"
        } else {
            "YoloSwitch::fixed"
        })
    }
}

/// 拒绝时的引导口径：**内置对话**有弹窗可点，**外部 MCP** 没有 UI——这条差别只在
/// 「拒绝文案」上，策略本身完全相同（同一条 `ConfirmPolicy`、同一份审计）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyGuidance {
    /// 内置对话：弹窗拒绝（或 P0 的 fail-closed）。
    InApp,
    /// 外部 MCP 客户端：引导用户去设置页确认，或打开 YOLO。
    External,
}

impl DenyGuidance {
    /// 拒绝后回给模型/用户的一句话（区分「谁拒的」与「下一步怎么办」）。
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::InApp => "用户拒绝了这次操作，未做任何修改",
            Self::External => {
                "这次操作需要用户确认，但当前是外部 MCP 客户端（没有确认弹窗），已按默认策略拒绝。\
                 请让用户在启动器设置页手动修改，或在设置 → AI 里打开 YOLO 模式后重试（审计照记）。"
            }
        }
    }
}

impl ConfirmPolicy {
    /// 把装配期的值变成判据（[`YoloSwitch`]）。
    ///
    /// 生产装配点不要用这个：那边给的是 [`YoloSwitch::live`] 的探针（见 `crates/app-iced` 的
    /// `Plan::yolo_switch` 与 `ull agent serve`）。这里给固定值，是给测试与「一次运行一个值」用。
    #[must_use]
    pub fn switch(self) -> YoloSwitch {
        YoloSwitch::fixed(self.yolo)
    }

    /// 该等级是否需要**询问**用户（等价于 [`ConfirmPolicy::switch`] 再问一次）。
    #[must_use]
    pub fn requires_confirmation(self, sensitivity: Sensitivity) -> bool {
        self.switch().requires_confirmation(sensitivity)
    }

    /// 该等级是否可以直接执行（不需要询问）。
    #[must_use]
    pub fn auto_approves(self, sensitivity: Sensitivity) -> bool {
        !self.requires_confirmation(sensitivity)
    }
}

impl From<ConfirmPolicy> for YoloSwitch {
    fn from(policy: ConfirmPolicy) -> Self {
        policy.switch()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn low_and_medium_keys_never_ask() {
        let policy = ConfirmPolicy { yolo: false };
        assert!(!policy.requires_confirmation(Sensitivity::Low));
        assert!(!policy.requires_confirmation(Sensitivity::Medium));
        assert!(policy.auto_approves(Sensitivity::Low));
        assert!(policy.auto_approves(Sensitivity::Medium));
    }

    #[test]
    fn high_keys_ask_unless_yolo() {
        assert!(ConfirmPolicy { yolo: false }.requires_confirmation(Sensitivity::High));
        assert!(!ConfirmPolicy { yolo: false }.auto_approves(Sensitivity::High));
        assert!(!ConfirmPolicy { yolo: true }.requires_confirmation(Sensitivity::High));
        assert!(ConfirmPolicy { yolo: true }.auto_approves(Sensitivity::High));
    }

    #[test]
    fn sensitivity_matches_m12_levels() {
        // 序列化名必须与 M12 `setting::spec::Sensitivity` 一致
        assert_eq!(serde_json::to_string(&Sensitivity::Low).unwrap(), "\"low\"");
        assert_eq!(
            serde_json::to_string(&Sensitivity::Medium).unwrap(),
            "\"medium\""
        );
        assert_eq!(
            serde_json::to_string(&Sensitivity::High).unwrap(),
            "\"high\""
        );
    }

    #[test]
    fn a_live_switch_is_re_read_on_every_decision() {
        // M389 的验收内核：**同一个**开关，值一换，下一次判定就跟着换（不是装配期快照）。
        let flag = Arc::new(AtomicBool::new(true));
        let switch = YoloSwitch::live({
            let flag = Arc::clone(&flag);
            move || flag.load(Ordering::Relaxed)
        });

        assert!(
            !switch.requires_confirmation(Sensitivity::High),
            "开着：不问"
        );
        assert!(switch.auto_approves(Sensitivity::High));

        // 运行期关掉（设置页写盘 → 探针下一次读到的就是新值）
        flag.store(false, Ordering::Relaxed);
        assert!(
            switch.requires_confirmation(Sensitivity::High),
            "关掉之后**同一条会话**的高敏感写必须重新问人"
        );
        assert!(!switch.auto_approves(Sensitivity::High));

        // 再打开：又不再问
        flag.store(true, Ordering::Relaxed);
        assert!(!switch.requires_confirmation(Sensitivity::High));
    }

    #[test]
    fn a_live_switch_survives_across_threads() {
        // 判据与审计不在同一线程上：开关必须能跨线程共享（`Send + Sync`）。
        let flag = Arc::new(AtomicBool::new(true));
        let switch = YoloSwitch::live({
            let flag = Arc::clone(&flag);
            move || flag.load(Ordering::Relaxed)
        });
        let worker = {
            let switch = switch.clone();
            std::thread::spawn(move || switch.requires_confirmation(Sensitivity::High))
        };
        assert!(!worker.join().expect("worker 线程"), "YOLO 开着：不问");
        flag.store(false, Ordering::Relaxed);
        assert!(
            switch.requires_confirmation(Sensitivity::High),
            "跨线程现读"
        );
    }
}
