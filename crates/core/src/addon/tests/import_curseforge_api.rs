//! M317：CurseForge 客户端接进导入链（**补全那一半**，全离线）。
//!
//! 本卷管「有 Key 时补得出来」：两次 GET（文件 → 名字/地址/摘要/长度，项目 → `classId`），以及
//! 站点没给 `downloadUrl` 时走 `/download-url` 兜底。导入链上的降级（缺 Key / 401 / 429 / 一条
//! 不挡另一条 / Key 不泄漏）在 `import_curseforge_key.rs`，替身在
//! `import_curseforge_support.rs`。

use crate::addon::curseforge::RetryPolicy;
use crate::addon::import::fetch::curse_meta;
use crate::addon::import::FetchErrorKind;
use crate::download::blocking::block_on;

use super::import_curseforge_support::{
    file_json, key, Scripted, CURSE_BYTES, FILE_URL, JEI, MOD_JSON, TEST_KEY,
};
use super::support::sha1;

/// 「缺 Key」是一个**独立档**：可容忍（那一条记缺、整包照装）、字符串 id 稳定、
/// 与 `Unsupported`（这条路走不通）分得开。
///
/// 抓得住：有人把它并回 `Unsupported`（上层就会说「这条真装不了」，用户再也不会去填 Key）⇒ 红；
/// 有人把它移出 `is_tolerable`（缺 Key 会让整个导入失败）⇒ 红。
#[test]
fn missing_key_is_a_named_kind_not_unsupported() {
    assert_eq!(
        FetchErrorKind::NotConfigured.as_str(),
        "fetch_not_configured"
    );
    assert!(FetchErrorKind::NotConfigured.is_tolerable());
    assert!(FetchErrorKind::NotConfigured.is_not_configured());
    assert!(!FetchErrorKind::Unsupported.is_not_configured());
    assert!(FetchErrorKind::Unsupported.is_tolerable());
}

/// 配了 Key ⇒ 两次 GET 换出一条完整元数据（文件名 / 地址 / 官方 sha1 / 字节数 / 项目类别）。
///
/// 抓得住：只留文件那次 GET（`class_id` 变 `None`，导入侧会把条目记成缺）⇒ `class_id` 断言红；
/// 摘要取错算法（md5 当 sha1）⇒ `sha1` 断言红；`fileLength` 为 0 时当成「期望 0 字节」⇒ `size` 红。
#[test]
fn with_a_key_the_client_completes_one_entry() {
    let transport = Scripted::new()
        .route("/mods/238222/files/5102025", 200, file_json())
        .route("/mods/238222", 200, MOD_JSON);
    let key = key();
    let meta = block_on(curse_meta(
        &transport,
        Some(&key),
        RetryPolicy::disabled(),
        JEI.0,
        JEI.1,
    ))
    .expect("有 Key 就必须补得出来");

    assert_eq!(meta.file_name, "jei-1.20.1-forge-15.2.0.27.jar");
    assert_eq!(meta.url.as_deref(), Some(FILE_URL));
    assert_eq!(meta.sha1.as_deref(), Some(sha1(CURSE_BYTES).as_str()));
    assert_eq!(meta.size, Some(CURSE_BYTES.len() as u64));
    assert_eq!(meta.class_id, Some(6), "目录来自项目的 classId");

    // Key 只出现在请求头里（两条请求都带），不出现在 URL 里。
    let requests = transport.requests();
    assert_eq!(requests.len(), 2, "文件 + 项目各一次：{requests:?}");
    for (url, headers) in &requests {
        assert!(
            !url.contains(TEST_KEY),
            "Key 绝不许进 URL（那是日志与代理都会记的地方）：{url}"
        );
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "x-api-key" && value == TEST_KEY),
            "每条请求都要带 Key：{headers:?}"
        );
    }
}

/// `downloadUrl` 为 `null`（作者关了 API 分发）时走 `/download-url` 端点，不猜 CDN 路径。
#[test]
fn a_file_without_a_site_url_asks_the_resolver() {
    let file_without_url = r#"{"data":{"id":4455667,"modId":238222,"fileName":"jei.jar","fileLength":18,"downloadUrl":null,"hashes":[]}}"#;
    let transport = Scripted::new()
        .route(
            "/files/4455667/download-url",
            200,
            r#"{"data":"https://edge.forgecdn.net/resolved/jei.jar"}"#,
        )
        .route("/mods/238222/files/5102025", 200, file_without_url)
        .route("/mods/238222", 200, MOD_JSON);
    let key = key();
    let meta = block_on(curse_meta(
        &transport,
        Some(&key),
        RetryPolicy::disabled(),
        JEI.0,
        JEI.1,
    ))
    .expect("兜底端点给了地址");
    assert_eq!(
        meta.url.as_deref(),
        Some("https://edge.forgecdn.net/resolved/jei.jar")
    );
}
