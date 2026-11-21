//! M390：设置页那个 **YOLO 开关**的活证据——拨一下，**同一个进程**里判定点就换答案。
//!
//! ## 这条用例证明什么（一串真东西，不落在任何中间变量上）
//!
//! ```text
//! ai_about::Message::ToggleYolo(true)          ← 开关那颗控件的真消息（`ai::view::yolo_row`）
//!   └─ App::toggle_ai_yolo                     ← 落点（app/update/settings/ai_about.rs）
//!        └─ ai::store::set_yolo                ← 本组唯一写路径（`SettingStore::set` + `save` + 重读）
//!             └─ config/launcher.json          ← 断言落在**文件字节**上
//! Plan::at(同一条设置文件)                       ← 判定点：M389 的「每次裁决现读文件」探针
//!   └─ yolo_now() → ConfirmPolicy → requires_confirmation(High)
//! ```
//!
//! 于是「改开关 ⇒ 下一次高敏感写的行为跟着变」有两个方向都在**同一进程、不重启、不重建 Plan**
//! 的前提下被量到：关 → 问人；开 → 不问；再关 → 又问。
//!
//! ## 抓得住 / 抓不住
//!
//! 抓得住：①开关没接线（`ToggleYolo` 落到 `touch()` 那种草稿臂上 ⇒ 文件不变 ⇒ 红）；
//! ②`set_yolo` 没落盘或写错键名（文件断言红）；③判定点改回「装配期快照」⇒
//! `yolo_now()` 停在旧值 ⇒ 红；④界面不回显落盘后的真值（`app.settings.ai_about.yolo` 红）。
//!
//! 抓不住：真实鼠标点击（本文件从 `Message` 起跑，与 `tests/java_refresh.rs` 同一档）；
//! 「审计照记 `yolo=true`」「凭据键仍写不到」——那两条由 agent 侧守着
//! （`crates/agent/tests/tools_security.rs::yolo_skips_confirm_but_audits_flag` 与
//! `locked_credential_key_is_refused_even_under_yolo`），本文件**不去碰判定链**。

use unlucky_agent::confirm::{ConfirmPolicy, Sensitivity};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use super::{ai, bridged_app, headless};
use crate::app::state::presenter::real::Plan;
use crate::pages;

/// 落一份**能装配出 `Plan`** 的 AI 配置（`Plan::at` 要求 provider 四项齐全）。
///
/// 用 `SettingStore` 而不是走设置页：本用例的起点是「用户已经配好助手」，不是「怎么配」。
fn seed_configured(path: &std::path::Path) {
    let mut store = SettingStore::load(path, launcher_schema());
    store.set("ai.enabled", true.into()).expect("ai.enabled");
    store
        .set("ai.provider", "openai_compatible".to_string().into())
        .expect("ai.provider");
    store
        .set("ai.base_url", "http://127.0.0.1:8080/v1".to_string().into())
        .expect("ai.base_url");
    store
        .set("ai.model", "qwen2.5-7b".to_string().into())
        .expect("ai.model");
    store.set("ai.yolo", false.into()).expect("ai.yolo");
    store.save().expect("落盘");
}

/// 高敏感写要不要问人（`confirm.rs` 里判定链的那一对，逐字取产品代码）。
fn asks(sensitivity: Sensitivity, yolo: bool) -> bool {
    ConfirmPolicy { yolo }.requires_confirmation(sensitivity)
}

/// **主证据**：同一个进程里拨开关，同一条会话（同一个 `Plan`）的下一笔高敏感写换答案。
#[test]
fn flipping_the_switch_changes_the_next_high_write_in_the_same_process() {
    use pages::settings::ai_about::Message as Ai;

    let (mut app, path) = bridged_app("yolo-switch");
    seed_configured(&path);

    // 判定点：**只装配这一次**（= 一条已经跑着的会话）。后面不再 `Plan::at`。
    let plan = Plan::at(&path).expect("合法的 AI 配置应当装配得起来");
    assert!(!plan.yolo_now(), "文件里写着 false");
    assert!(
        asks(Sensitivity::High, plan.yolo_now()),
        "YOLO 关着：高敏感写必须问人"
    );

    // ── 用户拨开开关（真消息，不是直接改文件）──
    let _ = app.update(ai(Ai::ToggleYolo(true)));

    let text = std::fs::read_to_string(&path).expect("拨开关之后必须有配置文件");
    assert!(
        text.contains("\"yolo\": true"),
        "开关必须真的落进磁盘（不是只改内存）：{text}"
    );
    assert!(
        app.settings.ai_about.yolo,
        "界面要换成**落盘后读回来**的那份真值"
    );
    assert!(
        plan.yolo_now(),
        "同一个 Plan（同一条会话）现读就是开着的——判定点没有抄装配期快照"
    );
    assert!(
        !asks(Sensitivity::High, plan.yolo_now()),
        "YOLO 打开：同一笔高敏感写不再问人"
    );

    // ── 再拨回去：同一进程、同一条会话，下一笔又开始问 ──
    let _ = app.update(ai(Ai::ToggleYolo(false)));
    assert!(!plan.yolo_now(), "拨回去必须立刻生效（不重启、不重建会话）");
    assert!(
        asks(Sensitivity::High, plan.yolo_now()),
        "关掉之后高敏感写必须重新问人"
    );

    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// 没有设置文件时不假装：开关不动、回执说清为什么（`SaveNotice::NoStorage`）。
///
/// 抓得住：`toggle_ai_yolo` 在 `bridge.path()` 为 `None` 时仍去改内存（那会让界面显示「已打开」，
/// 而磁盘上什么都没有）——那是本 mission 要反的那种谎。
#[test]
fn without_a_settings_file_the_switch_refuses_and_keeps_its_value() {
    use pages::settings::ai::SaveNotice;
    use pages::settings::ai_about::Message as Ai;

    let mut app = headless();
    let _ = app.update(ai(Ai::ToggleYolo(true)));

    assert_eq!(
        app.settings.ai_about.notice,
        Some(SaveNotice::NoStorage),
        "没有可写的设置文件时必须给具名回执"
    );
    assert!(
        !app.settings.ai_about.yolo,
        "落不了盘就不许把开关画成打开的"
    );
}

/// 拨开关**不许**顺手把别的草稿落盘，也不许把草稿标成已保存。
///
/// 抓得住：把 `ToggleYolo` 接到 `Message::Save` 那条臂上（有人图省事就这么做）——那时
/// 半截的端点草稿会被一起写进文件，而且 `dirty` 被清掉 ⇒ 两条断言都红。
#[test]
fn flipping_the_switch_never_writes_the_rest_of_the_draft() {
    use pages::settings::ai_about::Message as Ai;

    let (mut app, path) = bridged_app("yolo-draft");
    let _ = app.update(ai(Ai::EditBaseUrl("http://127.0.0.1:8080/v1".to_string())));
    let _ = app.update(ai(Ai::EditModel("half-typed".to_string())));

    let _ = app.update(ai(Ai::ToggleYolo(true)));

    let text = std::fs::read_to_string(&path).expect("拨开关应当写出配置文件");
    assert!(text.contains("\"yolo\": true"), "{text}");
    assert!(
        !text.contains("half-typed"),
        "拨开关把没保存的草稿一起落盘了：{text}"
    );
    assert!(
        app.settings.ai_about.dirty,
        "草稿还没保存，拨一下开关不许把它标成已保存"
    );

    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}
