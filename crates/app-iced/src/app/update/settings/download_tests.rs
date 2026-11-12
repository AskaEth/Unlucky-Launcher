//! Downloads 组的**写回闭环**（M201）：`控件 → Message → AppCommand::SetSetting → 真 host 写盘
//! → CoreEvent::SettingsChanged → App 的快照与页面模型`。
//!
//! 为什么这条断言必须在 app 层：链子的两头都是「有状态的东西」——中间断在任何一环，页面层测试
//! 都看不见（它只拿到一份已经画好的模型）。要问的是「用户点了以后，**盘上和 App 认的值**变了吗」，
//! 而不是「按钮的 `on_press` 里有没有构造消息」。
//!
//! 两个后端都测，因为它们证明的事不同：
//!
//! | 用例 | 后端 | 证明 |
//! |---|---|---|
//! | 前四条 | 假后端（按 host 形状回快照） | 搬运：消息 → 命令、命令 → 快照、快照 → 模型 |
//! | 最后一条 | **真 `CoreHost` + 临时数据根** | 整条链：命令 → `settings::set` → `launcher.json` 真的变了 |

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dto::{
    AppCommand, AppEvent, Backend, CoreEvent, SettingControl, SettingGroupView, SettingRowView,
};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::cli::Options;
use crate::pages;
use crate::pages::settings::download;
use crate::theme::Families;

use super::*;

/// 这一组要断言的那个键（与 presenter 快照里的键同名）。
const CONCURRENCY: &str = "download.concurrency";
/// 一个文本键（代理地址）。
const PROXY_URL: &str = "download.proxy_url";

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 一份只有下载组的快照：并发 = `concurrency`，代理 = `proxy`。
///
/// 行形状照 presenter 的 `host::settings::download::group` 抄（choice + 档位选项，text + 原文），
/// 只保留本文件断言要用的那两行——这里守的是 app 侧的搬运，不是 presenter 的行文案。
fn snapshot(concurrency: &str, proxy: &str) -> Vec<SettingGroupView> {
    let options = ["4", "8", "16"];
    let selected = i32::try_from(
        options
            .iter()
            .position(|item| *item == concurrency)
            .unwrap_or(1),
    )
    .expect("装得下");
    vec![SettingGroupView::new("download", "下载与镜像（D7）")
        .with(SettingRowView::choice(
            CONCURRENCY,
            "并发数",
            "同时下载的文件数",
            &options,
            selected,
        ))
        .with(SettingRowView::text(
            PROXY_URL,
            "HTTP 代理",
            "manual 模式生效",
            proxy,
        ))]
}

/// 记下令、并按 host 的形状回一条 `SettingsChanged` 的后端。
struct Hostish {
    commands: Rc<RefCell<Vec<AppCommand>>>,
    queued: Vec<AppEvent>,
    /// 回事件时用的快照（`SetSetting` 之后换成「值真的变了」的那一份）。
    state: RefCell<(String, String)>,
}

impl Backend for Hostish {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut self.queued)
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.commands.borrow_mut().push(command.clone());
        if let AppCommand::SetSetting { key, value } = command {
            let mut state = self.state.borrow_mut();
            if key == CONCURRENCY {
                state.0 = value;
            } else if key == PROXY_URL {
                state.1 = value;
            }
            // host 写盘之后重新发一份快照（`dispatch.rs` 的 `SetSetting` 分支就这一行）。
            self.queued
                .push(AppEvent::Core(CoreEvent::SettingsChanged(snapshot(
                    &state.0, &state.1,
                ))));
        }
    }
}

/// 一个装着 [`Hostish`] 的 App + 那本命令账。
fn app() -> (App, Rc<RefCell<Vec<AppCommand>>>) {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let backend = Hostish {
        commands: Rc::clone(&commands),
        queued: Vec::new(),
        state: RefCell::new(("8".to_string(), "http://127.0.0.1:7890".to_string())),
    };
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(backend),
        SettingsBridge::memory_only(),
    );
    // 首屏：收一份 host 的快照（产品路径在 `on_boot` 的 `RefreshAll` 之后就是这样）。
    app.state
        .apply(AppEvent::Core(CoreEvent::SettingsChanged(snapshot(
            "8",
            "http://127.0.0.1:7890",
        ))));
    app.refresh_view();
    (app, commands)
}

/// 一条设置页消息走完漏斗。
fn send(app: &mut App, message: pages::settings::download::Message) {
    let message = pages::Message::Settings(pages::settings::GroupMessage::Download(message));
    let _ = app.on_page(message);
}

/// **主用例**：点一个档位 ⇒ 命令发出去了 ⇒ host 回的快照进了 App ⇒ 页面模型跟着换。
///
/// 抓得住：
/// 1. handler 不派发 `AppCommand::SetSetting`（写回那一行被删）——这时假后端收不到命令、
///    也回不出事件，快照停在旧值，红（变异验证见报告）；
/// 2. `pages::Model` 不从快照取设置（`view::model` 那行 `settings:` 被删）——`app.view.settings`
///    是空的，`current_value` 返回 `None`，红；
/// 3. App 侧**乐观回显**（点完就地改本地值、不等事件）——把假后端的回事件删掉后第 2 段仍会绿，
///    所以本用例的「红」只证明闭合回路；它抓不住「有没有多存一份本地值」这一半。
///
/// 抓不住：真实点击（`Chip` 的 `on_press` 是否被真的按下）、host 是否真的落盘（那是最后一条的活）。
#[test]
fn changing_a_choice_value_lands_in_the_app_snapshot_and_the_page_model() {
    let (mut app, commands) = app();
    assert_eq!(
        download::current_value(app.settings_snapshot(), CONCURRENCY).as_deref(),
        Some("8"),
        "开局认的是 host 给的那份"
    );

    send(
        &mut app,
        pages::settings::download::Message::Set {
            key: CONCURRENCY.to_string(),
            value: "4".to_string(),
        },
    );

    assert!(
        commands.borrow().iter().any(|command| matches!(
            command,
            AppCommand::SetSetting { key, value } if key == CONCURRENCY && value == "4"
        )),
        "写回必须走已有的 `AppCommand::SetSetting`（实际下发：{:?}）",
        commands.borrow()
    );
    assert_eq!(
        download::current_value(app.settings_snapshot(), CONCURRENCY).as_deref(),
        Some("4"),
        "App 的设置快照必须跟着 host 回来那份变（不是本地 State 变）"
    );
    assert_eq!(
        download::current_value(&app.view.settings, CONCURRENCY).as_deref(),
        Some("4"),
        "页面模型（这一帧要画的值）也必须是 4：控件画的是 `Model.settings`"
    );
}

/// 文本行：`Edit` 只改草稿（**不发指令**），`Commit` 才带着草稿去写盘。
///
/// 抓得住：`Edit` 也派发（每敲一个键写一次盘）、`Commit` 用快照旧值而不是草稿（用户输入丢失）。
/// 抓不住：输入框本身的按键路径（`text_field` 的 `on_input` 是组件既有实现）。
#[test]
fn typing_a_text_value_only_commits_on_demand() {
    let (mut app, commands) = app();

    send(
        &mut app,
        pages::settings::download::Message::Edit {
            key: PROXY_URL.to_string(),
            value: "http://127.0.0.1:1080".to_string(),
        },
    );
    assert!(
        commands.borrow().is_empty(),
        "输入过程中不许写盘（半截值会落进 launcher.json）"
    );

    send(
        &mut app,
        pages::settings::download::Message::Commit {
            key: PROXY_URL.to_string(),
        },
    );
    assert!(
        commands.borrow().iter().any(|command| matches!(
            command,
            AppCommand::SetSetting { key, value } if key == PROXY_URL && value == "http://127.0.0.1:1080"
        )),
        "提交时带的必须是**草稿**里的那一串（不是快照里的旧值）：{:?}",
        commands.borrow()
    );
    assert_eq!(
        download::current_value(app.settings_snapshot(), PROXY_URL).as_deref(),
        Some("http://127.0.0.1:1080"),
        "提交后快照换成 host 回来的新值"
    );
}

/// 动作行（「重新检测地区」）：派发的是那一行的键，值留空（presenter 那条分支先认键、不看值）。
///
/// 抓得住：动作按钮被写成「什么都不发」、或者把键发成按钮文案。
#[test]
fn an_action_row_dispatches_its_key() {
    let (mut app, commands) = app();
    let key = "download.action.detect_region".to_string();

    send(
        &mut app,
        pages::settings::download::Message::Action { key: key.clone() },
    );

    assert!(
        commands.borrow().iter().any(|command| matches!(
            command,
            AppCommand::SetSetting { key: sent, .. } if sent == &key
        )),
        "动作行必须走「按 schema 键写设置」那一条既有通路：{:?}",
        commands.borrow()
    );
}

/// 快照里没有本组时，页面**不许**自己编一行（`controls_of` 空 = 退回演示行）。
///
/// 抓得住：把演示行当成真值画（M193 用户的投诉形状）。
/// 抓不住：`settings/mod.rs` 那条退回路径本身（那是 `demo_card` 的事）。
#[test]
fn an_empty_snapshot_yields_no_controls() {
    let (mut app, _commands) = app();
    app.state.settings_groups.clear();
    app.refresh_view();

    assert!(download::controls_of(app.settings_snapshot()).is_empty());
    // 有快照时，本组的控件种类就是 presenter 给的那几行（这里两份：choice + text）。
    let kinds = download::controls_of(&snapshot("8", ""));
    assert_eq!(
        kinds,
        vec![SettingControl::Choice, SettingControl::Text],
        "控件种类照快照的 `control` 字段画，不许自己另定"
    );
}

/// **真 host 端到端**：点「16」⇒ 「设置页里改了就生效」这句话的正面证据。
///
/// 链子全长：`Message::Set` → `AppCommand::SetSetting` → 真 `CoreHost` 的 `dispatch.rs` 分支
/// （`settings::set` 校验 + 落盘 + `apply_tuning` + 重新发 `SettingsChanged`）→ `AppState` →
/// `pages::Model::settings`。断言落在**磁盘上那串字节**上，不是任何中间变量。
///
/// 抓得住：
/// 1. 只改本地、不派发（`Message::Set` 的分支被删）⇒ `launcher.json` 里没有 16，红；
/// 2. 派发了但 handler 用了错误的键/值 ⇒ 文件里那个键还是出厂 8，红；
/// 3. 快照没跟着 host 的回流走（`view::model` 那行 `settings:` 被删）⇒ 页面模型空，红；
/// 4. 写盘校验被绕过（`settings::set` 不再 `save()`）⇒ 文件不存在/没变，红。
///
/// 抓不住：真实鼠标点击（本用例从 `Message` 起跑，`Chip::filter` 的 `on_press` 不在其中）、
/// 引擎是否立刻吃到新值（`apply_tuning` 与 `concurrency_probe` 由 presenter 的
/// `the_concurrency_setting_reaches_the_real_download_engine` 守）、以及界面像素（截图的事）。
#[test]
fn a_choice_change_reaches_the_real_settings_file() {
    let dir = std::env::temp_dir().join(format!("ull-m201-writeback-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ULL/config")).expect("config 目录");
    let config = dir.join(".ULL/config/launcher.json");

    let env = unlucky_presenter::host::HostEnv::at_root(&dir);
    let mut app = App::with_presenter(
        Options {
            data_dir: Some(dir.clone()),
            ..Options::default()
        },
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(unlucky_presenter::host::CoreHost::new(env)),
        // 设置桥也指向同一个根：不碰用户的真配置。
        SettingsBridge::with_path(unlucky_presenter::backend::config_path(&dir)),
    );
    // 首屏：产品路径在 `on_boot` 里派的就是这一条（真 host 会发 `SettingsChanged`）。
    app.dispatch(AppCommand::RefreshAll);
    assert_eq!(
        download::current_value(app.settings_snapshot(), CONCURRENCY).as_deref(),
        Some("8"),
        "临时根里没有这个键 ⇒ 快照给的是出厂 8（也是引擎的生效值）"
    );

    send(
        &mut app,
        pages::settings::download::Message::Set {
            key: CONCURRENCY.to_string(),
            value: "16".to_string(),
        },
    );

    let written = std::fs::read_to_string(&config).expect("写回之后 launcher.json 必须存在");
    assert!(
        written.contains("\"concurrency\": 16"),
        "磁盘上必须真的有 16（这才是「设置页里改了就生效」）：{written}"
    );
    assert_eq!(
        download::current_value(app.settings_snapshot(), CONCURRENCY).as_deref(),
        Some("16"),
        "host 把新快照发回来了，App 认的也是 16"
    );
    assert_eq!(
        download::current_value(&app.view.settings, CONCURRENCY).as_deref(),
        Some("16"),
        "页面模型也换成了 16（下一帧控件画的就是它）"
    );

    std::fs::remove_dir_all(&dir).ok();
}
