//! AI 栏折叠档的**落盘**（M452）：`ui.ai.collapsed` 的写端。
//!
//! ## 为什么单独一卷
//!
//! `agent.rs` 贴着 400 行硬线（M452 进来时 396 行），两个调用点各一行，理由得放别处。
//!
//! ## 这一档到底记的是什么
//!
//! 记的是「常驻右栏是收起还是展开」，**不是**三档里的 `Detached`：拆成独立页是「这次我要看
//! 整页会话」，不是「以后都把栏收起来」。所以：
//!
//! | 用户动作 | 盘上写什么 |
//! |---|---|
//! | `Expand`（点图标条 / `Tab` 到展开） | `false` |
//! | `Collapse` / `Esc`（含独立页那支：它先 `dock` 成展开） | `true`（独立页那支是 `false`） |
//! | `Detach` / `Dock` | **不写**（盘上保持上一次的折叠档） |
//!
//! 写走 presenter 的 `SettingsBridge::persist_ai_collapsed`（现读 → 只改这一键 → 落盘 → 刷
//! `mtime`），于是「刚存完 AI 配置就收起右栏」不会把 `ai.*` 连密钥一起抹掉。

use crate::app::state::App;
use crate::shell::agent::RailState;

impl App {
    /// 折叠档变化之后写盘（`ui.ai.collapsed`）。**必须在 `AgentUi` 的档位迁移之后调**：
    /// 它读的是迁移完的 `self.agent.rail`。
    ///
    /// 拆出档（`Detached`）不写盘：见模块文档那张表。内存态桥（数据根解析不出来）返回
    /// `false`，静默——配置写不进去不该把界面卡住（与 `nav` 那条落盘同一口径）。
    pub(super) fn persist_ai_rail(&mut self) {
        if self.agent.rail == RailState::Detached {
            return;
        }
        let collapsed = self.agent.rail == RailState::Collapsed;
        let saved = self.bridge.persist_ai_collapsed(collapsed);
        // 落盘结果留一行：`skipped` 时下一个人要先怀疑桥（数据根 / 只读），不是怀疑折叠。
        crate::app::diag::diag!(
            "[agent] collapsed={collapsed} persist={}",
            if saved { "saved" } else { "skipped" }
        );
    }
}

#[cfg(test)]
mod tests {
    //! 判据是「**换一个桥对象读同一份文件**」（= 重启后的读法）+「换一个 App 启动读回来」，
    //! 不是「bridge 上有这个方法」。

    use std::path::PathBuf;

    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::prefs::keys;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::app::components::motion;
    use crate::app::Message;
    use crate::cli::Options;
    use crate::shell::agent::Message as AgentMessage;
    use crate::theme::Families;

    use super::*;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 一份临时 `launcher.json`（父目录建好、文件删掉 = 首次启动）。
    fn temp_config(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-agent-prefs-{label}-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        let _ = std::fs::remove_file(&path);
        path
    }

    fn app_with(bridge: SettingsBridge) -> App {
        motion::reset();
        App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            bridge,
        )
    }

    /// 首次启动（**没有配置文件**）：右栏保持出厂折叠档，且不因为「读不到」而在盘上写点什么。
    #[test]
    fn a_first_boot_without_the_key_keeps_the_factory_collapsed_rail() {
        let path = temp_config("first");
        let app = app_with(SettingsBridge::with_path(&path));
        assert_eq!(app.agent.rail, RailState::Collapsed, "缺键 ⇒ 出厂折叠档");
        assert!(
            !SettingsBridge::with_path(&path).ui_key_present(keys::AI_COLLAPSED),
            "缺键的首次启动不该顺手把出厂值写成一条显式配置"
        );
        assert!(!app.state.prefs.ai_collapsed, "presenter 的字段照抄出厂值");
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// **展开 → 落盘 → 重启仍是展开**（这是唯一能分辨「读没读」的一档：出厂是折叠）。
    ///
    /// 抓得住：①`Expand` 分支没调 `persist_ai_rail`（盘上没有 `false`）；②`construct.rs` 的
    /// `adopt_ui_prefs` 没读回来（重启又是折叠，第二段红）；③缺键与「显式 false」被当成一回事
    /// （把键删掉跑这条也会红，因为出厂值是 `false` 但界面出厂态是折叠）。
    #[test]
    fn an_expanded_rail_survives_a_restart() {
        let path = temp_config("expand");
        let mut app = app_with(SettingsBridge::with_path(&path));
        let _ = app.update(Message::Agent(AgentMessage::Expand));
        assert_eq!(app.agent.rail, RailState::Expanded);

        // 落盘：换一个桥读同一份文件
        let reopened = SettingsBridge::with_path(&path);
        assert!(
            reopened.ui_key_present(keys::AI_COLLAPSED),
            "展开必须落盘，否则重启就回到出厂折叠档"
        );
        assert!(
            !reopened.load_ui_keys().ai_collapsed,
            "展开档在盘上是 false"
        );

        // 「重启」：新 App + 新桥，读同一份文件
        let restarted = app_with(SettingsBridge::with_path(&path));
        assert_eq!(
            restarted.agent.rail,
            RailState::Expanded,
            "启动必须读回展开档（`adopt_ui_prefs` 的接线）"
        );
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// 收起之后落盘 `true`，且**展开再收起**这一来一回不会把值写反。
    #[test]
    fn a_collapsed_rail_is_written_as_true() {
        let path = temp_config("collapse");
        let mut app = app_with(SettingsBridge::with_path(&path));
        let _ = app.update(Message::Agent(AgentMessage::Expand));
        let _ = app.update(Message::Agent(AgentMessage::Collapse));
        assert_eq!(app.agent.rail, RailState::Collapsed);
        assert!(
            SettingsBridge::with_path(&path).load_ui_keys().ai_collapsed,
            "收起档在盘上是 true"
        );

        // `Esc` 走同一条 `collapse_agent`：从展开再收一次，值不许翻回去
        let _ = app.update(Message::Agent(AgentMessage::Expand));
        let _ = app.update(Message::Agent(AgentMessage::Escape));
        assert_eq!(app.agent.rail, RailState::Collapsed);
        assert!(
            SettingsBridge::with_path(&path).load_ui_keys().ai_collapsed,
            "`Esc` 收起也要落盘"
        );
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// 拆出档（`Detach`）**不是**折叠档：盘上的值保持不动。
    ///
    /// 抓得住：把 `Detached` 当成「展开」写进去（那样「折叠着退出 → 拆一次页 → 重启」会变成展开）。
    #[test]
    fn detaching_does_not_touch_the_disk() {
        let path = temp_config("detach");
        let mut app = app_with(SettingsBridge::with_path(&path));
        let _ = app.update(Message::Agent(AgentMessage::Collapse));
        assert!(SettingsBridge::with_path(&path).load_ui_keys().ai_collapsed);

        let _ = app.update(Message::Agent(AgentMessage::Detach));
        assert_eq!(app.agent.rail, RailState::Detached);
        assert!(
            SettingsBridge::with_path(&path).load_ui_keys().ai_collapsed,
            "拆出页不许把折叠档改成展开"
        );

        // 独立页上按 `Esc`：`collapse_agent` 走 `dock` ⇒ 展开 ⇒ 盘上必须变 false
        let _ = app.update(Message::Agent(AgentMessage::Escape));
        assert_eq!(app.agent.rail, RailState::Expanded);
        assert!(
            !SettingsBridge::with_path(&path).load_ui_keys().ai_collapsed,
            "从独立页收回是展开档，要落盘"
        );
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }
}
