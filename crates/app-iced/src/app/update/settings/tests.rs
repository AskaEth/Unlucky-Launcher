//! 设置页分组消息分发的**守门测试**（M101 `SETTINGSMSG`）。
//!
//! 判据与 `app/update/page_messages.rs` 里那条 `the_page_match_has_no_wildcard_fallback` 同源：
//! 兜底会把「加了变体忘了接线」从**编译错误**降级成「点了没反应」。两级 dispatch 各守一条：
//! 那一层守 `pages::Message`，本文件守 `GroupMessage` 与四个 handler。
//!
//! ## 判据（r2 复审 P2-3 之后收窄过一次）
//!
//! 上一版的判据是「除已知前缀外，任何含 `=>` 的行都算兜底」——**过宽**：往 handler 里加一个
//! **没有兜底臂**的嵌套 match（`match true { true => …, false => … }`），或者 rustfmt 把长
//! or-pattern 折行成 `| X =>`，都会让它假红。四条填页线一写真实逻辑就撞，守门反而推高成本。
//!
//! 现在的判据是**结构化的**：只找 `match message {` 这一层（用大括号深度计数把嵌套 match 排除
//! 在外），**只看深度 1 的臂**——
//!
//! - `app/update/settings.rs` 的 `on_settings`：恰好 **4 条**臂，每条以 `pages::settings::GroupMessage::`
//!   （或短写 `GroupMessage::`）开头；
//! - 四个 handler：每条臂以该组自己的 `pages::settings::<线>::Message::`（或短写 `Message::`）开头，
//!   还没有臂的（空枚举 `match message {}`）一条臂都没有；
//! - 以 `|` 开头的折行 or-pattern 续行放行（它不可能是兜底）。
//!
//! ## 抓得住什么 / 抓不住什么
//!
//! 抓得住：
//! 1. `on_settings` 里加 `_ =>` 或 binding 臂 `other =>`（`GroupMessage` 不再穷尽）；
//! 2. handler 把 `match message {}` 改成 `_ => Task::none()` / `other => { … }`；
//! 3. `on_settings` 的臂数不是 4（删了一条、或把两条合并）；
//! 4. handler 的臂写成别的组的变体路径（前缀不对）。
//!
//! 抓不住（老实列出来）：
//! 1. **嵌套 match 里的兜底**——深度 ≥2 的臂不看。这是刻意的：嵌套 match 的穷尽性由它自己的类型
//!    保证，跟 `GroupMessage` 漏接线无关；代价是没人守「嵌套 match 自己写了个 `_ =>`」；
//! 2. **跨行 pattern 分两种，别当一种**：以 `|` 开头的 or-pattern 续行**放行**（不误红）；
//!    但**花括号形状**的多行 pattern（例如 `Message::SetRuntime {` 折成多行、`=>` 落在后面）
//!    会被**静默跳过**——那一行不带 `=>`，于是既不算臂也不算违规（臂数 0、零违规），是**漏检**
//!    而不是误红。两种都是洞，当前代码风格都不出现；真要用这种写法时把规则补上，别把守门删掉；
//! 3. **字符串/字符字面量里的 `{` `}`** 会扰乱深度计数（本文件与四个 handler 都没有这种写法）；
//! 4. handler 忽略消息（`let _ = message; Task::none()`）、两个 handler 身体写反或写成一样
//!    ——类型对得上就编得过，只有人读能看出来；
//! 5. `match message` 被改名或换变量名时找不到匹配，臂数为 0 —— 这时
//!    `the_group_dispatch_has_exactly_four_arms` 会红（不是静默通过）。
//!
//! 站位：本文件是 `app::update::settings::tests`，`include_str!` 的路径相对本文件所在目录。

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::message::Message;
use crate::app::state::App;
use crate::cli::Options;
use crate::pages;
use crate::theme::Families;

/// `on_settings` 的臂允许的前缀。
const GROUP_ARMS: &[&str] = &["pages::settings::GroupMessage::", "GroupMessage::"];

/// 一行的净大括号增量（`{` 加、`}` 减）。
fn brace_delta(line: &str) -> i32 {
    let open = line.matches('{').count() as i32;
    let close = line.matches('}').count() as i32;
    open - close
}

/// 扫一条 `match message { … }`，只看**本层**（深度 1）的臂。
///
/// 返回 `(本层的合法臂数, 违规行)`：违规 = 本层里含 `=>`、又不是「已知前缀开头」的行
/// （`|` 开头的折行 or-pattern 续行放行）。嵌套 match 的臂在深度 ≥2，整段跳过。
fn dispatch_arms(path: &str, source: &str, allowed: &[&str]) -> (usize, Vec<String>) {
    let mut arms = 0usize;
    let mut offending = Vec::new();
    let mut depth = 0i32;
    let mut inside = false;

    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }

        if !inside {
            if trimmed.contains("match message {") {
                inside = true;
                depth = 1;
            }
            continue;
        }

        if depth == 1 && trimmed.contains("=>") {
            let pattern = trimmed.trim_start_matches('|').trim_start();
            if allowed.iter().any(|prefix| pattern.starts_with(prefix)) {
                arms += 1;
            } else if !trimmed.starts_with('|') {
                offending.push(format!("{path}:{}: {trimmed}", index + 1));
            }
        }

        depth += brace_delta(trimmed);
        if depth <= 0 {
            break;
        }
    }

    (arms, offending)
}

/// 不碰磁盘的 App（mock 后端 + 内存设置桥），与 `app/tests.rs` 的 `headless` 同一副骨架。
fn headless() -> App {
    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    )
}

// --------------------------------------------------------------- 守门本体

/// `on_settings` 与四个 handler 的**本层**臂里不许出现任何形式的兜底（`_ =>` 或 `other =>`）。
#[test]
fn the_group_dispatch_has_no_catch_all_arm() {
    let sources: [(&str, &str, &[&str]); 5] = [
        (
            "app/update/settings.rs",
            include_str!("../settings.rs"),
            GROUP_ARMS,
        ),
        (
            "app/update/settings/java.rs",
            include_str!("java.rs"),
            &["pages::settings::java::Message::", "Message::"],
        ),
        (
            "app/update/settings/download.rs",
            include_str!("download.rs"),
            &["pages::settings::download::Message::", "Message::"],
        ),
        (
            "app/update/settings/language_appearance.rs",
            include_str!("language_appearance.rs"),
            &[
                "pages::settings::language_appearance::Message::",
                "Message::",
            ],
        ),
        (
            "app/update/settings/ai_about.rs",
            include_str!("ai_about.rs"),
            &["pages::settings::ai_about::Message::", "Message::"],
        ),
    ];

    let mut offending = Vec::new();
    for (path, source, allowed) in sources {
        let (_, bad) = dispatch_arms(path, source, allowed);
        offending.extend(bad);
    }

    assert!(
        offending.is_empty(),
        "设置页分组消息的 dispatch 本层出现了兜底臂（{offending:?}）：\
         任何 catch-all / binding 臂都会取消 `GroupMessage` 的穷尽性，\
         新变体漏接线时编译器不再报 E0004"
    );
}

/// `on_settings` 必须**恰好四条**臂，一条一条显式写全（不是「至少」）。
///
/// 抓得住：删掉一条臂、把两条合并、或者把 `match message` 改名（改名后扫不到臂 ⇒ 0 ≠ 4）。
/// 抓不住：臂的顺序（顺序不影响语义）、每条臂接到了哪个 handler（类型对得上就编得过）。
#[test]
fn the_group_dispatch_has_exactly_four_arms() {
    let (arms, _) = dispatch_arms(
        "app/update/settings.rs",
        include_str!("../settings.rs"),
        GROUP_ARMS,
    );

    assert_eq!(
        arms, 4,
        "`on_settings` 应该恰好四条臂（java / download / language_appearance / ai_about），实际 {arms} 条"
    );
}

/// **守门自身不误伤**：嵌套 match 与折行的 or-pattern 都不算兜底（r2 复审 P2-3 的探针）。
///
/// 这条是上一版判据假红的回归测试：复审往 handler 里加 `match true { true => …, false => … }`
/// 就让它 FAILED；现在这类写法必须放行。
#[test]
fn the_guard_ignores_nested_matches_and_folded_or_patterns() {
    let synthetic = "\
impl App {
    fn on_settings_java(&mut self, message: pages::settings::java::Message) -> Task<Message> {
        match message {
            pages::settings::java::Message::Probe => {
                let nested = match true { true => 10u8, false => 11u8 };
                let _ = nested;
                Task::none()
            }
            | pages::settings::java::Message::Retry => Task::none(),
        }
    }
}
";

    let (arms, offending) = dispatch_arms(
        "synthetic",
        synthetic,
        &["pages::settings::java::Message::"],
    );
    assert!(
        offending.is_empty(),
        "正常代码被误判成兜底（这正是 r2 复审 P2-3 的假红）：{offending:?}"
    );
    assert_eq!(arms, 2, "嵌套 match 里的两条臂不算本层臂");
}

/// **守门仍然有牙**：`_ =>` 与 binding 臂 `other =>` 都要被本层判成兜底。
#[test]
fn the_guard_still_catches_both_catch_all_shapes() {
    for arm in [
        "_ => Task::none(),",
        "other => { let _ = other; Task::none() }",
    ] {
        let synthetic = format!(
            "impl App {{\n    fn on_settings_java(&mut self, message: M) -> Task<Message> {{\n        match message {{\n            {arm}\n        }}\n    }}\n}}\n"
        );
        let (_, offending) = dispatch_arms("synthetic", &synthetic, GROUP_ARMS);
        assert_eq!(
            offending.len(),
            1,
            "`{arm}` 是兜底，必须被判红（实际 {offending:?}）"
        );
    }
}

// --------------------------------------------------------------- 活的端到端证据

/// **活的端到端证据**：一条组消息真的从外壳那条既有通道走到该组的 handler（r1 复审要求）。
///
/// 路径：`app::Message::Page` → `on_page` → `pages::Message::Settings` → `on_settings` →
/// `on_settings_java`，最后改到 `App` 的状态。r1 之前这条链只有编译期证据（`GroupMessage` 是
/// uninhabited，运行时是死代码）；现在至少有一支真的跑得起来。
///
/// 抓得住：漏斗某一环接断了（`on_page` 没转交、`on_settings` 转错组、handler 忽略消息）——那时
/// `view_dirty` 不会翻。抓不住：真实用户点击（本条没有 UI 控件，构造点只有测试）。
#[test]
fn a_group_message_reaches_its_handler_through_the_funnel() {
    let mut app = headless();
    app.view_dirty = false;

    // M175：`Ping` 随演示行一起删了，改用一个**真变体**（改内存草稿）——它同样只经这条漏斗
    // 改 `App` 状态，所以证据强度不变，还顺带证明这条臂真的接到了 handler 上。
    let _ = app.update(Message::Page(pages::Message::Settings(
        pages::settings::GroupMessage::Java(pages::settings::java::Message::EditMemory(
            "4096".to_string(),
        )),
    )));

    assert!(
        app.view_dirty,
        "java 线的消息没有走到 `on_settings_java`：消息漏斗在某一环断了"
    );
}

// --------------------------------------------------------------- M174：助手配置的活路径

/// 一个指向临时目录的设置桥（真文件，不是内存态）。
pub(super) fn bridged_app(label: &str) -> (App, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ull-ai-app-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(dir.join("config")).expect("临时目录");
    let path = dir.join("config/launcher.json");
    let _ = std::fs::remove_file(&path);
    let app = App::with_presenter(
        Options::default(),
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::with_path(&path),
    );
    (app, path)
}

fn ai(message: pages::settings::ai_about::Message) -> Message {
    Message::Page(pages::Message::Settings(
        pages::settings::GroupMessage::AiAbout(message),
    ))
}

/// **M174 的活路径**：设置页那四条消息（改端点/模型/密钥）+ 保存，真的写进 `launcher.json`。
///
/// 抓得住：handler 没接 `Save`、`store::save` 没写某一个键、写错键名、密钥没落盘。
/// 抓不住：用户真的点了那颗按钮（那是 iced 事件循环的事），也抓不住「保存后 agent 读到了」
/// （那一半由 `unlucky-cli` 的 `probe_file` 用例证明：它读的就是这个文件）。
#[test]
fn saving_the_ai_form_writes_the_real_settings_file() {
    use pages::settings::ai_about::Message as Ai;

    let (mut app, path) = bridged_app("save");
    let _ = app.update(ai(Ai::PickProvider(
        pages::settings::ai::ProviderId::OpenAiCompatible,
    )));
    let _ = app.update(ai(Ai::EditBaseUrl("http://127.0.0.1:8080/v1".to_string())));
    let _ = app.update(ai(Ai::EditModel("qwen2.5-7b".to_string())));
    let _ = app.update(ai(Ai::EditKey(pages::settings::ai::Secret::new(
        "sk-live-app-path",
    ))));
    let _ = app.update(ai(Ai::Save));

    let text = std::fs::read_to_string(&path).expect("保存之后应当有配置文件");
    assert!(
        text.contains("\"base_url\": \"http://127.0.0.1:8080/v1\""),
        "{text}"
    );
    assert!(text.contains("\"model\": \"qwen2.5-7b\""), "{text}");
    assert!(
        text.contains("\"provider\": \"openai_compatible\""),
        "{text}"
    );
    assert!(text.contains("\"api_key\": \"sk-live-app-path\""), "{text}");

    // 保存之后：草稿清干净、回执是「已保存」、密钥输入框清空（值已进文件，不再留在 UI 状态里）。
    let state = &app.settings.ai_about;
    assert_eq!(state.notice, Some(pages::settings::ai::SaveNotice::Saved));
    assert!(!state.dirty);
    assert!(state.key.is_empty());
    assert_eq!(state.key_chars, "sk-live-app-path".chars().count());
    let printed = format!("{state:?}");
    assert!(
        !printed.contains("sk-live-app-path"),
        "UI 状态泄漏密钥：{printed}"
    );
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// **非法值不保存**：端点没有协议头时，保存被拦下、文件不动、回执是具名拒绝原因。
#[test]
fn refusing_to_save_leaves_the_file_untouched() {
    use pages::settings::ai_about::Message as Ai;

    let (mut app, path) = bridged_app("refuse");
    let _ = app.update(ai(Ai::EditBaseUrl("http://127.0.0.1:8080/v1".to_string())));
    let _ = app.update(ai(Ai::EditModel("qwen".to_string())));
    let _ = app.update(ai(Ai::Save));
    let saved = std::fs::read_to_string(&path).expect("第一次应当写成");

    let _ = app.update(ai(Ai::EditBaseUrl("api.deepseek.com/v1".to_string())));
    let _ = app.update(ai(Ai::Save));
    assert_eq!(
        app.settings.ai_about.notice,
        Some(pages::settings::ai::SaveNotice::Refused(
            pages::settings::ai::state::Refusal::BaseUrl
        )),
        "没有协议头的端点必须被拦下，且要说清原因"
    );
    let after = std::fs::read_to_string(&path).expect("读文件");
    assert_eq!(saved, after, "被拒绝的保存不许改动文件：{after}");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// **未配置时不发请求**（GUI 侧那一半）：缺模型名时点「测试连接」不会起任务、不留 Running 态。
#[test]
fn testing_without_configuration_does_nothing() {
    use pages::settings::ai_about::Message as Ai;

    let (mut app, path) = bridged_app("no-test");
    let _ = app.update(ai(Ai::EditBaseUrl("http://127.0.0.1:8080/v1".to_string())));
    // 模型名空着 ⇒ `can_test()` 为假
    assert!(app.settings.ai_about.missing().is_some());
    let _ = app.update(ai(Ai::Test));
    assert_eq!(
        app.settings.ai_about.probe,
        pages::settings::ai::Probe::Idle,
        "未配置时点测试不该有任何探测态（更不该发请求）"
    );
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// M175 r1 复审的用例（拆出来守 400 行硬线；`gate::every_source_file_stays_under_the_line_limit` 会红）。
#[path = "tests/java.rs"]
mod java;

/// M204：Java 组写盘之后请 host 重发快照（动作键 `JAVA_REFRESH_ACTION`）。
#[path = "tests/java_refresh.rs"]
mod java_refresh;
