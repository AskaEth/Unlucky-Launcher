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

    let _ = app.update(Message::Page(pages::Message::Settings(
        pages::settings::GroupMessage::Java(pages::settings::java::Message::Ping),
    )));

    assert!(
        app.view_dirty,
        "java 线的消息没有走到 `on_settings_java`：消息漏斗在某一环断了"
    );
}
