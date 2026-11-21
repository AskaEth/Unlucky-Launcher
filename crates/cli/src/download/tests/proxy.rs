//! **代理两键的 CLI 侧守门（M295）**：`download.proxy_mode` / `download.proxy_url` 从 CLI 自己的
//! `launcher.json` 进四条命令的生产装配点，终端那行生效值也含代理档。
//!
//! 从 `tests.rs` 拆出只为守 400 行硬线（与 core 的 `engine_mock/*.rs` 同一手法）。
//!
//! 抓得住：① `download::tuning` 不读 `KEY_PROXY_MODE` / `KEY_PROXY_URL` ⇒ 三档全退回 `Direct`，
//! 前两条红；② `summary()` 少印代理 ⇒ 那行断言红；③ 非法地址让装配 panic ⇒ 最后一段红；
//! ④ 键名常量与 schema 登记脱钩 ⇒ 末尾那句红。
//!
//! 抓不住：请求是不是**真的**经由代理（core 的 `tests/download/engine_mock/proxy.rs`：本地假代理 +
//! `.invalid` 目标 + 绝对 URI 请求行）；也抓不住真实的系统代理 / PAC / 需要认证的代理
//! （见 `docs/download-settings.md` §7）。

use super::*;

#[test]
fn the_proxy_settings_reach_the_cli_engine_and_the_console_line() {
    use unlucky_core::download::config::{KEY_PROXY_MODE, KEY_PROXY_URL};
    use unlucky_core::download::proxy::ProxySetting;

    let dir = TempDir::new("proxy");
    let ctx = ctx_at(&dir);

    // manual：地址进引擎配置，终端那行写的是同一档。
    write_launcher_config(
        &ctx,
        json!({ "download": { "proxy_mode": "manual", "proxy_url": " http://127.0.0.1:7890 " } }),
    );
    let engine = crate::commands::install::build_engine(&ctx);
    assert_eq!(
        engine.config().proxy,
        ProxySetting::Manual {
            url: "http://127.0.0.1:7890".to_string()
        },
        "manual 档必须落到引擎配置上（去空白）"
    );
    let line =
        crate::download::announce(&crate::download::tuning(&ctx)).expect("改过设置 ⇒ 该有一行");
    assert!(
        line.contains("代理=manual（http://127.0.0.1:7890）"),
        "{line}"
    );

    // system + 一个残留地址：地址被忽略，档位是 system（终端那行也要说清是「跟随环境变量」）。
    write_launcher_config(
        &ctx,
        json!({ "download": { "proxy_mode": "system", "proxy_url": "http://127.0.0.1:7890" } }),
    );
    assert_eq!(
        crate::commands::install::build_engine(&ctx).config().proxy,
        ProxySetting::System
    );
    let line = crate::download::announce(&crate::download::tuning(&ctx)).expect("该有一行");
    assert!(line.contains("代理=system（跟随环境变量）"), "{line}");

    // 非法地址：不 panic；引擎给出可读原因，配置字段仍是用户写的值（回落是运行时行为）。
    write_launcher_config(
        &ctx,
        json!({ "download": { "proxy_mode": "manual", "proxy_url": "http://127.0.0.1:99999" } }),
    );
    let engine = crate::commands::install::build_engine(&ctx);
    let problem = engine
        .proxy_problem()
        .expect("地址解析不了要留下原因（CLI 的那行 stderr 靠它）");
    assert!(problem.contains("http://127.0.0.1:99999"), "{problem}");
    assert_eq!(
        engine.config().proxy,
        ProxySetting::Manual {
            url: "http://127.0.0.1:99999".to_string()
        }
    );

    // 两个键名常量必须真的登记在 schema 里（写错 = 「用户设了没生效」的静默退化）。
    let schema = launcher_schema();
    for key in [KEY_PROXY_MODE, KEY_PROXY_URL] {
        assert!(schema.spec(key).is_some(), "{key} 没登记");
    }
}

/// **口令不进终端**（r1 复审 p2-1 的 CLI 侧判据）：`announce` 那一行就是用户（和 CI 日志）看到的
/// 字符串，所以断言落在**最终的字符串**上，而不是「函数存在」。
///
/// 变异：`ProxySetting::describe` 改回 `format!("manual（{url}）")` ⇒ 第一段红（`contains(SECRET)`）。
#[test]
fn the_console_line_never_carries_the_proxy_password() {
    const SECRET: &str = "PWMUSTNOTLEAK";
    let dir = TempDir::new("proxy-secret");
    let ctx = ctx_at(&dir);
    write_launcher_config(
        &ctx,
        json!({ "download": { "proxy_mode": "manual", "proxy_url": format!("http://user:{SECRET}@127.0.0.1:7890") } }),
    );
    let line =
        crate::download::announce(&crate::download::tuning(&ctx)).expect("改过设置 ⇒ 该有一行");
    assert!(!line.contains(SECRET), "终端那行泄漏了代理口令：{line}");
    assert!(line.contains("user:***@"), "{line}");

    // 非法地址那一行（回落提示）同样不许带口令。
    write_launcher_config(
        &ctx,
        json!({ "download": { "proxy_mode": "manual", "proxy_url": format!("http://user:{SECRET}@127.0.0.1:99999") } }),
    );
    let engine = crate::commands::install::build_engine(&ctx);
    let problem = engine.proxy_problem().expect("端口越界要留下原因");
    assert!(
        !problem.contains(SECRET),
        "回落提示泄漏了代理口令：{problem}"
    );
    assert!(problem.contains("user:***@"), "{problem}");
}
