//! M174 的助手分组活路径：**消息 → handler → `SettingStore` → 真文件**。
//!
//! M390 从 `tests.rs` 纯搬移到这里，理由与 `tests/java.rs` 完全一样：`tests.rs` 顶在 400 行
//! 硬线上（`gate::every_source_file_stays_under_the_line_limit`），再挂一个 `mod` 就红。
//! 判据与行为一字未改。

use crate::pages;

use super::{ai, bridged_app};

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
