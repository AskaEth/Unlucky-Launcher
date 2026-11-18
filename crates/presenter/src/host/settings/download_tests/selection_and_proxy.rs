//! **最近两轮接进来的两个键族**的守门：源选择（M294）与代理（M295）。
//!
//! 从 `download_tests.rs` 搬出只为守 400 行硬线（与 `support/server.rs` 拆出同一理由）：那条文件
//! 里这一族是最后两段，搬过来正文一字未改，只是多了一句「搬家原因」。
//!
//! ## 代理两键（M295）：`download.proxy_mode` / `download.proxy_url`
//!
//! 从文件走到**引擎配置**，走的是 GUI 的生产装配点 `host::engine::engine`（内部 `apply_tuning` →
//! `shared::set_tuning` → 按数据根重建）。
//!
//! 抓得住：① `tuning_of` 少读一个键（`proxy_mode` 或 `proxy_url`）⇒ 第一、三条红；② 非 manual 档
//! 也把地址塞进 `Manual` ⇒ 第一条红；③ `manual` + 空地址落成 `Direct`（设置页承诺的是「用系统代理」）
//! ⇒ 第四条红；④ 非法地址让引擎构造 panic ⇒ 最后一段红；⑤ 两行文案与三档语义脱钩 ⇒ 文案断言红。
//!
//! 抓不住：请求是不是**真的**经由代理（那是 core 的 `tests/download/engine_mock/proxy.rs`：本地假
//! 代理 + `.invalid` 目标 + 绝对 URI 请求行）；也抓不住发布版 GUI 的「下载 JDK」页——那条路径仍用
//! `EngineConfig::default()`（见 `docs/download-settings.md` §3.1.1 的 ❌ 一行）。

use super::*;
use unlucky_core::download::proxy::ProxySetting;

/// 写文件 → 走生产装配点 → 返回「引擎拿到的代理档 + 引擎自报的问题」。
fn applied(tag: &str, document: serde_json::Value) -> (ProxySetting, Option<String>) {
    let (dir, env) = env_with(tag, document);
    std::fs::create_dir_all(env.layout.cache()).expect("cache 目录");
    let engine = crate::host::engine::engine(&env);
    let seen = (
        engine.config().proxy.clone(),
        engine.proxy_problem().map(str::to_string),
    );
    std::fs::remove_dir_all(&dir).ok();
    seen
}

#[test]
fn the_proxy_keys_travel_from_the_file_to_the_engine_config() {
    use unlucky_core::download::config::{EngineConfig, KEY_PROXY_MODE, KEY_PROXY_URL};

    // ① none（也是 schema 出厂值）：直连；**非 manual 档的地址被忽略**。
    assert_eq!(
        applied(
            "proxy-none",
            serde_json::json!({ "download": { "proxy_mode": "none", "proxy_url": "http://127.0.0.1:7890" } })
        ),
        (ProxySetting::Direct, None),
        "none 档必须是真直连，且地址不参与"
    );

    // ② system：跟随环境变量（同样是「不调代理 API」那一档，地址仍被忽略）。
    assert_eq!(
        applied(
            "proxy-system",
            serde_json::json!({ "download": { "proxy_mode": "system", "proxy_url": "http://127.0.0.1:7890" } })
        ),
        (ProxySetting::System, None)
    );

    // ③ manual：地址进结构（去空白）。
    assert_eq!(
        applied(
            "proxy-manual",
            serde_json::json!({ "download": { "proxy_mode": "manual", "proxy_url": " http://127.0.0.1:7890 " } })
        ),
        (
            ProxySetting::Manual {
                url: "http://127.0.0.1:7890".to_string()
            },
            None
        )
    );

    // ④ manual + 空地址 ⇒ 设置页那行文案承诺的「用系统代理」。
    assert_eq!(
        applied(
            "proxy-manual-empty",
            serde_json::json!({ "download": { "proxy_mode": "manual", "proxy_url": "" } })
        ),
        (ProxySetting::System, None)
    );

    // ⑤ 非法地址：不 panic、有可读原因、配置不撒谎（仍是用户写的 manual）。
    let (proxy, problem) = applied(
        "proxy-broken",
        serde_json::json!({ "download": { "proxy_mode": "manual", "proxy_url": "http://127.0.0.1:99999" } }),
    );
    assert_eq!(
        proxy,
        ProxySetting::Manual {
            url: "http://127.0.0.1:99999".to_string()
        }
    );
    let problem = problem.expect("解析不了必须留下原因（否则用户只会觉得「填了没用」）");
    assert!(problem.contains("http://127.0.0.1:99999"), "{problem}");

    // 两行文案必须与新语义一致（否则界面又回到「填了不知道算不算数」）。
    let (dir, env) = env_with("proxy-rows", serde_json::json!({}));
    let group_view = group(&env);
    let mode_row = row(&group_view, KEY_PROXY_MODE);
    assert!(
        mode_row
            .description
            .contains("system = 跟随系统环境变量代理")
            && mode_row.description.contains("manual"),
        "代理模式那行必须把三档说清：{}",
        mode_row.description
    );
    let url_row = row(&group_view, KEY_PROXY_URL);
    assert!(
        url_row.description.contains("只在 manual 模式生效"),
        "地址那行必须说清它在别的档被忽略：{}",
        url_row.description
    );
    assert_eq!(
        EngineConfig::default().proxy,
        ProxySetting::System,
        "「没读设置」的档仍是跟随环境变量（auth / JDK 页走它），与本文件的三档翻译是两件事"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// **M294**：两个「源选择」键真的进了 `DownloadTuning`（删掉 `with_source_selection` ⇒ 两断言红）。
#[test]
fn the_two_source_selection_keys_reach_the_tuning() {
    use unlucky_core::download::{MirrorPreference, RegionPreference};
    let (dir, env) = env_with(
        "source-selection",
        serde_json::json!({ "download": { "region": "global", "mirror_preference": "official" } }),
    );
    let tuning = tuning_of(&env);
    assert_eq!(tuning.region, RegionPreference::Global);
    assert_eq!(tuning.mirror_preference, MirrorPreference::Official);
    std::fs::remove_dir_all(&dir).ok();
}

/// **回落必须被用户看见**（r1 复审 p2-2 的 GUI 侧判据）。
///
/// 断的是**最终的文案**（设置页「HTTP 代理」那一行）：出现「已回落到直连」+ 原因，独占一行，且
/// 原因里没有口令；合规地址那一行**没有**这句（否则天天挂一条假警告）。
///
/// 同源证明：这一行的原因与引擎的 `proxy_problem()` 是同一串（宿主调 `ProxySetting::problem`，
/// 引擎走 `apply`，两者共用 `manual_proxy` 那一个判定）。
///
/// 变异：删掉 `group()` 里的 `proxy_note` ⇒ 第一段红（文案里没有「已回落到直连」）；让
/// `describe`/错误文案回到未脱敏的原串 ⇒ 第三段红（`contains(SECRET)`）。
#[test]
fn a_broken_proxy_url_is_shown_as_falling_back_to_direct() {
    use unlucky_core::download::config::KEY_PROXY_URL;

    const SECRET: &str = "PWMUSTNOTLEAK";
    let (dir, env) = env_with(
        "proxy-hint-broken",
        serde_json::json!({ "download": {
            "proxy_mode": "manual",
            "proxy_url": format!("http://user:{SECRET}@127.0.0.1:99999"),
        }}),
    );
    let group_view = group(&env);
    let url_row = row(&group_view, KEY_PROXY_URL);
    assert!(
        url_row.description.contains("\n已回落到直连："),
        "用不了的代理地址必须在设置页说出来（独占一行）：{:?}",
        url_row.description
    );
    assert!(
        url_row.description.contains("99999"),
        "提示里要带上用户写的那个地址（脱敏后）：{}",
        url_row.description
    );
    assert!(
        !url_row.description.contains(SECRET),
        "提示里泄漏了代理口令：{}",
        url_row.description
    );

    // 与引擎同源：宿主这一行说的就是引擎实际记录的那条原因。
    std::fs::create_dir_all(env.layout.cache()).expect("cache 目录");
    let problem = crate::host::engine::engine(&env)
        .proxy_problem()
        .map(str::to_string)
        .expect("引擎侧也该记录这条原因");
    assert!(
        url_row.description.contains(&problem),
        "设置页与引擎说的不是同一件事：{problem}"
    );
    std::fs::remove_dir_all(&dir).ok();

    // 合规地址 ⇒ 不许有这条提示（假警告会让真的那条失效）。
    let (dir, env) = env_with(
        "proxy-hint-ok",
        serde_json::json!({ "download": { "proxy_mode": "manual", "proxy_url": "http://127.0.0.1:7890" } }),
    );
    let ok_group = group(&env);
    let ok_row = row(&ok_group, KEY_PROXY_URL);
    assert!(
        !ok_row.description.contains("已回落到直连"),
        "合规地址不该有回落提示：{}",
        ok_row.description
    );
    std::fs::remove_dir_all(&dir).ok();
}
