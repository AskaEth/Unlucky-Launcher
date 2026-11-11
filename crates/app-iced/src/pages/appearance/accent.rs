//! 强调色的**运行期状态**（M250 ACCENTWIRE）：输入原文 → 解析结果 → 生效色。
//!
//! 这一层存在的理由：`appearance.accent_color` 此前是死线——键、输入框、presenter 往返都在，
//! 但造 tokens 的唯一入口恒走 `DesignTokens::brand(board)`，用户填了色界面没变化。接线要落地
//! 的不只是「把 hex 喂进 `Scheme::from_accent`」，还有**失败要可见**：
//!
//! | 输入 | 生效配色 | `error()` | 落盘值 |
//! |---|---|---|---|
//! | 空串 | 出厂品牌板（`DesignTokens::brand`） | `None` | `""` |
//! | `006A5C` | `DesignTokens::with_accent`（只换三族） | `None` | `#` + `006A5C` |
//! | `zzzzzz` / 超长串 | **保持上一次合法的值**（不静默回退成出厂色） | `Some(..)` | 上一次合法的值 |
//! | `#12`（还没打完） | 同上（还没生效） | `None`，`pending` 里挂着 | 上一次合法的值 |
//!
//! 「保持上一次合法值」而不是「回退出厂板」是刻意的：回退会让「填错」和「填对但没生效」在屏幕上
//! 长得一模一样，用户唯一的线索就只剩那条错行——而那正是这里要保证不被丢掉的。
//!
//! ## 何时才算「错」（M250 r1 p2-3）
//!
//! 受控输入框**每敲一个字符**都会调 [`Accent::set`]。手打 `006A5C` 的过程里，前 5 个字符都是
//! `Short`，若立刻亮红就会闪 5 次——用户没打错，只是没打完。所以只有两种情形立刻报错：
//!
//! 1. **不可能再靠打字变合法**：出现非十六进制字符（`Digit`）或超过 6 位（`Long`）——任何继续
//!    输入都只会更错，立刻亮；
//! 2. **提交**（[`Accent::commit`]）：离开这一格、点到别的外观控件、**按回车**（P18，M252 接的
//!    `Change::AccentCommit`）、或配置读回。
//!
//! 配置读回走 [`Accent::new`]（`commit` 语义）：文件里存着坏值必须**当场**看得见，不能等用户来点。
//!
//! 解析本身不在这里：`AccentChoice::parse` 是唯一真相（`unlucky_presenter::prefs`），本文件只负责
//! 把结果接到色值与 UI 上。

use unlucky_design::color::Color;
use unlucky_presenter::prefs::{AccentChoice, AccentError};

/// 出图矩阵的强调色开关（`ULL_APPEARANCE_ACCENT=#RRGGBB`；产品路径读 `launcher.json`）。
///
/// 放在本文件而不是 `state.rs` 的 `ENV_*` 那一族：`state.rs` 正好 400 行（硬线），这一条
/// 属于强调色自己，跟 `Accent` 放一起也更好找。语义与其它 `ULL_APPEARANCE_*` 一致——只影响这次进程。
pub const ENV_ACCENT: &str = "ULL_APPEARANCE_ACCENT";

/// 出图矩阵的第二档（`ULL_APPEARANCE_ACCENT_COMMIT=1`）：把 [`ENV_ACCENT`] 灌进来的值当作
/// **已提交**（[`Accent::commit`]）。
///
/// P18 / M252：「打了一半就放着」的 `#12` 平时**不**亮红，按回车才亮。出图通道没有键盘，所以用这个
/// 开关把那一帧拍下来——它拍的是 `commit()` 之后的**同一个状态**，不是另一条渲染分支。
/// 语义同族：只影响这次进程，产品路径不读它。
pub const ENV_ACCENT_COMMIT: &str = "ULL_APPEARANCE_ACCENT_COMMIT";

/// 强调色的运行期状态（`App` 经 `pages::appearance::State` 持有一份）。
#[derive(Debug, Clone, PartialEq)]
pub struct Accent {
    /// 输入框的原文（受控控件要它；非法输入也必须原样留着，否则用户看不见自己错在哪）。
    text: String,
    /// 上一次**合法**的解析结果（非法输入不改它 —— 生效色因此不会跳回出厂板）。
    choice: AccentChoice,
    /// 解析出来的强调色（`None` = 出厂板，强调色由品牌色板决定）。
    color: Option<Color>,
    /// **现在要显示**的结构化原因（`None` = 不显示）。
    error: Option<AccentError>,
    /// 已经打错、但「还可能被打字修好」的原因（输入中暂不显示，[`Accent::commit`] 才亮出来）。
    pending: Option<AccentError>,
}

impl Accent {
    /// 从配置读一份（启动时调一次）。配置里存着非法值时**当场**亮错行，不静默吞掉。
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut accent = Self {
            text: text.to_owned(),
            choice: AccentChoice::Factory,
            color: None,
            error: None,
            pending: None,
        };
        accent.retry(true);
        accent
    }

    /// 设置页改了一次输入（受控输入框：每敲一个字符都走这里）。
    ///
    /// 「短但还可能是合法前缀」的输入只挂进 `pending`，不亮红（见模块文档 §何时才算「错」）。
    pub fn set(&mut self, text: &str) {
        self.text = text.to_owned();
        self.retry(false);
    }

    /// 配置被外部改写后重置（热切：CLI / AI 改完 1 秒内生效）。
    pub fn reset(&mut self, text: &str) {
        *self = Self::new(text);
    }

    /// 提交：把「输入中暂不报」的错亮出来。离开这一格就该走它（见 [`Accent::pending`]）。
    ///
    /// 三个触发点：点到别的外观控件 / 按回车（P18 `Change::AccentCommit`）/ 配置读回。
    /// 幂等：已经亮着错再提交一次什么都不做。
    pub fn commit(&mut self) {
        if self.error.is_none() {
            self.error = self.pending.take();
        }
    }

    /// 把 `ULL_APPEARANCE_ACCENT` / `ULL_APPEARANCE_ACCENT_COMMIT` 灌进来（不设 = 不动）。
    ///
    /// 第一档 = 「像刚敲进去的」（走输入中规则）；再给它 `…_COMMIT=1` 就等于「刚按下回车」。
    /// 两档合起来才拍得出 P18 的对照：同一条 `#12`，回车前不亮、回车后亮红。
    pub fn apply_env(&mut self) {
        let Ok(value) = std::env::var(ENV_ACCENT) else {
            return;
        };
        self.set(&value);
        if std::env::var_os(ENV_ACCENT_COMMIT).is_some_and(|flag| flag != "0") {
            self.commit();
        }
    }

    /// 输入框原文。
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 要显示的结构化失败原因（`None` = 合法，或还没打完）。
    #[must_use]
    pub fn error(&self) -> Option<AccentError> {
        self.error
    }

    /// 解析出来的强调色（`None` = 出厂板；`rebuild_ui` 与出图判据读它）。
    #[must_use]
    pub fn color(&self) -> Option<Color> {
        self.color
    }

    /// 落盘值：`""` = 出厂，`#RRGGBB` = 自定义。**永远是上一次合法值**——非法手输不写盘。
    #[must_use]
    pub fn persist_value(&self) -> &str {
        self.choice.as_str()
    }

    /// 取证日志里的一个词：`factory` / 规范化的 hex / `invalid(..)` / `pending(..)`。
    ///
    /// 为什么日志要带这个：截图与真机复现只能靠一行文字分辨「强调色没生效」与「强调色生效了但
    /// 图上看不出来」——后者是这条 mission 的验收判据，前者是它的 bug。
    #[must_use]
    pub fn log_token(&self) -> String {
        if let Some(error) = self.error {
            return format!("invalid({error:?})");
        }
        if let Some(error) = self.pending {
            return format!("pending({error:?})");
        }
        if self.choice.as_str().is_empty() {
            "factory".to_owned()
        } else {
            self.persist_value().to_owned()
        }
    }

    /// 按当前 `text` 重算：合法 ⇒ 换色 + 清错；非法 ⇒ **色不动**，按 `show` 决定报不报。
    fn retry(&mut self, show: bool) {
        match AccentChoice::parse(&self.text) {
            Ok(choice) => {
                self.color = Color::from_hex(choice.as_str());
                self.choice = choice;
                self.error = None;
                self.pending = None;
            }
            Err(error) => {
                // 只有「位数不足」还有机会靠继续打字补成合法；非 hex 字符与超 6 位已经回不了头。
                let hopeless = !matches!(error, AccentError::Short { .. });
                if show || hopeless {
                    self.error = Some(error);
                    self.pending = None;
                } else {
                    self.pending = Some(error);
                    self.error = None;
                }
            }
        }
    }
}
