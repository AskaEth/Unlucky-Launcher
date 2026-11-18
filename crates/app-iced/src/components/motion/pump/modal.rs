//! 模态状态机与采样（`pump` 的子模块，只为守 400 行硬线而拆）。
//!
//! | 状态 | 进入条件 | 离开条件 |
//! |---|---|---|
//! | [`Modal::Hidden`] | `configure` / 退场走完 / 被别处硬关 | 开框 |
//! | [`Modal::Opening`] | `observe_modal(true)` | 进场腿走完 ⇒ `Open` |
//! | [`Modal::Open`] | 进场走完 | `begin_modal_close()` |
//! | [`Modal::Closing`] | `begin_modal_close()` | 退场腿走完 ⇒ `Hidden` + 交回一次「已关」 |
//!
//! **`Open` 是"没有轨道"的稳态**：对话框开着可能几分钟，那不是动画，一帧都不该订阅。

use super::super::transition::{modal_spec, ModalFrame};
use super::{leg_fraction, Pump};

/// 模态的状态机。
///
/// 为什么要状态机而不是一条进度：**弹窗开着的时候不能订阅帧**。`Open` 是一个"没有轨道"的
/// 稳态（[`Pump::is_animating`] 为假），只有起手与收尾那两段是动画。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Modal {
    /// 没有弹窗。
    Hidden,
    /// 进场中。
    Opening { start_ms: f64 },
    /// 完全展开（稳态，不占帧）。
    Open,
    /// 退场中。`from` = 起手那一刻的进度：进场途中被关掉时它是**当前进度**，不是 1.0
    /// （否则会先跳到完全展开再往回缩 = 反向硬跳）。
    Closing { start_ms: f64, from: f32 },
}

impl Pump {
    /// 弹窗的**逻辑**开关（每帧 update 侧同步一次）。
    ///
    /// - 开了 → 从 [`Modal::Hidden`] 起跑进场；已经在进/开着就不动。
    /// - 关了（不是走 [`Pump::begin_modal_close`] 的那条路，即框已经被别处直接关掉）→
    ///   立刻落 [`Modal::Hidden`]（没有框可画，也就没有退场可言）。
    ///
    /// 返回 `true` = 起跑了进场轨道。
    pub fn observe_modal(&mut self, open: bool) -> bool {
        if open {
            // 退场途中来了新框：**当场收尾**旧退场（`Hidden` + 待关），再走进场。不收尾的话
            // 新框会按递减的 progress 画（看起来是倒放），而且 `modal_closing()` 仍为真 ⇒
            // `on_dialog` 的闸门会把新框的按钮全吞掉（P2-2 的另两条后果）。
            if matches!(self.modal, Modal::Closing { .. }) {
                self.modal = Modal::Hidden;
                self.close_pending = true;
            }
            if matches!(self.modal, Modal::Hidden) {
                if self.scale.is_off() {
                    self.modal = Modal::Open;
                    return false;
                }
                self.modal = Modal::Opening {
                    start_ms: self.start_now(),
                };
                return true;
            }
            return false;
        }
        if !matches!(self.modal, Modal::Closing { .. }) {
            self.modal = Modal::Hidden;
            self.close_pending = false;
        }
        false
    }

    /// 请求关掉弹窗：有动效就起跑退场（返回 `false`，真正的关闭由 [`Pump::take_modal_closed`]
    /// 交回调用方），关着动效就返回 `true`（**立刻关，不留 0 时长动画**）。
    pub fn begin_modal_close(&mut self) -> bool {
        if self.scale.is_off() || matches!(self.modal, Modal::Hidden) {
            self.modal = Modal::Hidden;
            self.close_pending = false;
            return true;
        }
        // 已经在退场 ⇒ 保持原样（重复的关闭请求不该重算起点）。
        if matches!(self.modal, Modal::Closing { .. }) {
            return false;
        }
        // **起点是当前进度**：进场走到 20% 就关 ⇒ 从 0.20 往回缩；已展开 ⇒ 从 1.0 缩。
        // 不这么做就是「先跳到 1.0 再退」的反向硬跳（复审 r2 的 P2-2）。
        let from = self.modal_frame().progress;
        self.modal = Modal::Closing {
            start_ms: self.start_now(),
            from,
        };
        false
    }

    /// 退场是否已经走完（取走一次，之后返回 `false`）。
    pub fn take_modal_closed(&mut self) -> bool {
        std::mem::take(&mut self.close_pending)
    }

    /// 退场是否正在跑（这段时间里的弹窗消息要被吞掉：框已经在缩小，再点一次「删除」不该再生效）。
    pub fn modal_closing(&self) -> bool {
        matches!(self.modal, Modal::Closing { .. })
    }

    /// 这一帧模态该画成什么样。
    pub fn modal_frame(&self) -> ModalFrame {
        match self.modal {
            Modal::Hidden => ModalFrame::HIDDEN,
            Modal::Open => ModalFrame {
                visible: true,
                progress: 1.0,
            },
            Modal::Opening { start_ms } => {
                let spec = modal_spec();
                let t = leg_fraction(
                    self.elapsed_ms() - start_ms,
                    self.leg_ms(spec.enter.duration),
                );
                ModalFrame {
                    visible: true,
                    progress: spec.enter.easing.sample(t),
                }
            }
            Modal::Closing { start_ms, from } => {
                let spec = modal_spec();
                let t = leg_fraction(
                    self.elapsed_ms() - start_ms,
                    self.leg_ms(spec.exit.duration),
                );
                ModalFrame {
                    visible: true,
                    // 从起手进度**连续**缩到 0：`from * (1 - eased)`。`from = 1.0` 时与改造前
                    // 逐帧等价；进场途中关则是"从半开接着缩"。
                    progress: from * (1.0 - spec.exit.easing.sample(t)),
                }
            }
        }
    }
}
