//! [`super`] 的单测：翻译表、口令脱敏、非法/不支持地址、两个默认值。
//!
//! 从 `proxy.rs` 拆出只为守 400 行硬线（M295 r1 复审 p2-1 加了一族脱敏用例）。
//! 「请求真的经过代理」的端到端判据不在这里，在 `crates/core/tests/download/engine_mock/proxy.rs`。

use super::super::config::{DownloadTuning, EngineConfig};
use super::super::error::DownloadErrorKind;
use super::{redact_proxy_url, ProxySetting};

/// 口令样本：与本仓 `crates/cli/tests/config_secret_test.rs` 同一个纪律——**任何输出里都不许出现它**。
const SECRET: &str = "PWMUSTNOTLEAK";

/// **两键 → 三档**的唯一翻译规则（表驱动，逐档一条）。
///
/// 抓得住：① `manual` / `system` 拼错或大小写敏感 ⇒ 对应档红；② 非 manual 档还在读 `proxy_url`
/// （`none` + 一个地址仍进 `Manual`）⇒ 第 2、3、4 条红；③ `manual` + 空地址落成 `Direct`
/// （而不是设置页承诺的「用系统代理」）⇒ 第 5、6 条红；④ 地址没 trim、模式没小写归一 ⇒ 第 8 条红。
/// 抓不住：这些档在真客户端上的行为（那是 `engine_mock/proxy.rs` 的活）。
#[test]
fn the_two_keys_translate_to_the_three_modes() {
    let cases: [(Option<&str>, Option<&str>, ProxySetting); 8] = [
        // 键不存在 / 空串 ⇒ none（schema 出厂值）。
        (None, None, ProxySetting::Direct),
        (
            Some(""),
            Some("http://127.0.0.1:7890"),
            ProxySetting::Direct,
        ),
        // 显式 none：地址**忽略**（不进结构）。
        (
            Some("none"),
            Some("http://127.0.0.1:7890"),
            ProxySetting::Direct,
        ),
        // system：地址同样忽略。
        (
            Some("system"),
            Some("http://127.0.0.1:7890"),
            ProxySetting::System,
        ),
        // manual + 空地址 ⇒ 用系统代理（设置页那行文案的承诺）。
        (Some("manual"), Some("  "), ProxySetting::System),
        (Some("manual"), None, ProxySetting::System),
        // 未知值（手改文件）⇒ 回落 none。
        (Some("mars"), None, ProxySetting::Direct),
        // 正常档：大小写/空白不敏感，地址去空白后原样保留。
        (
            Some(" MANUAL "),
            Some(" http://127.0.0.1:7890 "),
            ProxySetting::Manual {
                url: "http://127.0.0.1:7890".to_string(),
            },
        ),
    ];
    for (mode, url, expected) in cases {
        assert_eq!(
            ProxySetting::from_raw(mode, url),
            expected,
            "mode={mode:?} url={url:?}"
        );
    }
    // `as_str` 与 schema 的三个取值逐字相同（诊断行/日志里印的就是它）。
    assert_eq!(ProxySetting::Direct.as_str(), "none");
    assert_eq!(ProxySetting::System.as_str(), "system");
    assert_eq!(
        ProxySetting::Manual {
            url: "http://p:1".to_string()
        }
        .as_str(),
        "manual"
    );
}

/// **口令不许出现在任何对外字符串里**（r1 复审 p2-1 的判据）。
///
/// 覆盖四条真实出口：CLI 的生效值行（`describe`→`summary`）、解析失败文案（`problem` 与 `apply`
/// 同一条）、`Debug`（配置/引擎被打进日志时）、以及脱敏函数本身。
///
/// 变异：`redact_proxy_url` 直接返回 `raw.to_string()` ⇒ 本用例第一段就红（`contains(SECRET)`），
/// 红的样子是 `代理口径的字符串里出现了口令：manual（http://user:PWMUSTNOTLEAK@…）`。
#[test]
fn a_password_in_the_proxy_url_never_reaches_a_user_visible_string() {
    let with_password = format!("http://user:{SECRET}@127.0.0.1:7890");
    let setting = ProxySetting::Manual {
        url: with_password.clone(),
    };

    // ① 生效值那一行（CLI stderr）。
    let line = DownloadTuning {
        proxy: setting.clone(),
        ..DownloadTuning::default()
    }
    .summary();
    assert!(!line.contains(SECRET), "生效值行泄漏口令：{line}");
    assert!(
        line.contains("代理=manual（http://user:***@127.0.0.1:7890）"),
        "用户名要留、口令要打成 `***`（否则用户看不出是哪个账号）：{line}"
    );

    // ② Debug（配置 / 引擎 / `assert_eq!` 的失败信息都会走到这里）。
    let debugged = format!("{setting:?}");
    assert!(!debugged.contains(SECRET), "Debug 泄漏口令：{debugged}");
    assert!(debugged.contains("user:***@"), "{debugged}");

    // ③ 解析失败的文案（`problem()` 与 `apply()` 同源）。
    let broken = ProxySetting::Manual {
        url: format!("http://user:{SECRET}@127.0.0.1:99999"),
    };
    let problem = broken.problem().expect("端口越界必须报错");
    assert!(!problem.contains(SECRET), "回落提示泄漏口令：{problem}");
    assert!(problem.contains("user:***@"), "{problem}");
    let error = broken
        .apply(reqwest::Client::builder())
        .expect_err("端口越界必须是有界错误");
    assert_eq!(error.kind, DownloadErrorKind::InvalidRequest);
    assert!(!error.message.contains(SECRET), "{}", error.message);

    // ④ 函数本身：`redact_proxy_url` 是唯一实现，宿主不许绕开它自己拼地址。
    assert_eq!(
        redact_proxy_url(&with_password),
        "http://user:***@127.0.0.1:7890"
    );
}

/// **没口令的地址逐字不变**（同一族的反向断言：脱敏不许把正常地址改坏）。
///
/// 变异证明：把 `redact_proxy_url` 改成「用 `Url::parse` 再 `to_string`」（补尾斜杠、小写化、
/// 百分号归一）⇒ 第 1、3、4 条红；把「只有用户名、没有口令」也打掉 ⇒ 第 2 条红。
#[test]
fn a_proxy_url_without_a_password_is_left_byte_identical() {
    let untouched = [
        "http://127.0.0.1:7890",
        "http://user@127.0.0.1:7890",
        "127.0.0.1:7890",
        "https://proxy.example:8443/v1",
        "http://127.0.0.1:7890/path@with@at?q=a:b#frag",
    ];
    for url in untouched {
        assert_eq!(redact_proxy_url(url), url, "不许动没口令的地址：{url}");
    }
    // 口令里带 `@` / 路径里带 `:` 这两种「分割点容易被挑错」的形态。
    assert_eq!(
        redact_proxy_url("http://user:p@ss@127.0.0.1:7890"),
        "http://user:***@127.0.0.1:7890"
    );
    assert_eq!(
        redact_proxy_url("http://user:pw@127.0.0.1:7890/a:b"),
        "http://user:***@127.0.0.1:7890/a:b"
    );
}

/// **四类「用不了」都走有界错误，而且每类各有一句中文原因**（r1 p3-1/p3-3 + r2 复审必修）。
///
/// 判据落在**最终文案**上：`problem()` 里出现该类的中文原因，且**没有**把 reqwest/url 的原文
/// 当结论（`builder error`、`invalid port number` 都不许出现在文案里）。
///
/// 为什么强调「自己算」：reqwest 的 `Proxy::all` 对端口越界只会回一句 `builder error`，而
/// `url::ParseError` 的 Display 是英文——两种「摊原文」都不算解释（r2 复审就是这么退回的）。
///
/// 变异：① 删掉 `validate` 里的端口判定 ⇒ 表格第一行红（落到 reqwest 那句 `builder error`）；
/// ② 删掉 scheme 白名单 ⇒ 第二行红（`problem()` 变 `None`）；③ 删掉主机名为空的判定 ⇒ 第三行红。
#[test]
fn unsupported_schemes_and_bad_ports_explain_themselves() {
    let socks = ProxySetting::Manual {
        url: format!("socks5://user:{SECRET}@127.0.0.1:1080"),
    };
    let problem = socks.problem().expect("本构建用不了的 scheme 必须报错");
    assert!(problem.contains("不支持 socks5"), "{problem}");
    assert!(
        problem.contains("只支持 http / https"),
        "文案要说清能用什么：{problem}"
    );
    assert!(!problem.contains(SECRET), "{problem}");
    assert!(socks.apply(reqwest::Client::builder()).is_err());

    // 四类各一句（表驱动）：每行都断「命中该类的中文原因」，并断**没有**摊英文原文。
    let cases: [(&str, &str); 5] = [
        ("http://127.0.0.1:99999", "端口 99999 不是 1–65535"),
        ("http://127.0.0.1:0", "端口 0 不是有效端口"),
        ("http://127.0.0.1:", "端口是空的"),
        ("http://:7890", "缺少主机名"),
        ("http://", "缺少主机名"),
    ];
    for (url, expected) in cases {
        let problem = ProxySetting::Manual {
            url: url.to_string(),
        }
        .problem()
        .unwrap_or_else(|| panic!("{url} 必须被判为用不了"));
        assert!(problem.contains(expected), "{url} ⇒ {problem}");
        assert!(
            !problem.contains("builder error") && !problem.contains("invalid port number"),
            "{url}：不许把 reqwest / url 的原文当结论：{problem}"
        );
    }
    // 空地址：`from_raw` 会把它翻成 `System`（manual + 留空 = 用系统代理），所以这种 `Manual` 只能
    // 手拼出来；手拼出来时同样有一句原因，不会静默通过。
    assert!(ProxySetting::Manual {
        url: "   ".to_string()
    }
    .problem()
    .is_some_and(|problem| problem.contains("地址是空的")));

    // 正常档与合法地址没有问题（否则 GUI 会天天挂一条假警告）。
    assert!(ProxySetting::Direct.problem().is_none());
    assert!(ProxySetting::System.problem().is_none());
    for ok in [
        "http://127.0.0.1:7890",
        "127.0.0.1:7890",
        "https://proxy.example:8443/v1",
        "http://[::1]:8080",
    ] {
        assert!(
            ProxySetting::Manual {
                url: ok.to_string()
            }
            .problem()
            .is_none(),
            "{ok} 是合法写法，不该被拒"
        );
    }
}

/// **两个默认值故意不同**（模块头那段的守卫）：没读设置的子系统保持 M295 之前的行为，
/// 读了设置的装配点与设置页的 `none` chip 对齐。
///
/// 抓得住：有人把 `EngineConfig::default().proxy` 一并改成 `Direct` ⇒ 第一条红（`auth` 与 JDK
/// 下载页会静默失去环境变量代理，那些路径在本仓读不到 `download.*`，改不回来）；把它改成
/// `Manual` 之类 ⇒ 第一条也红。`DownloadTuning::default()` 改成 `System` ⇒ 第二条红
/// （设置页画着 `none`，引擎却在跟随环境变量——本任务要治的原病）。
#[test]
fn the_two_defaults_are_deliberately_different() {
    assert_eq!(
        EngineConfig::default().proxy,
        ProxySetting::System,
        "EngineConfig::default() 是「没读设置」的档：保持 M295 之前的跟随环境变量"
    );
    assert_eq!(
        DownloadTuning::default().proxy,
        ProxySetting::Direct,
        "DownloadTuning::default() 是「读了设置、都是出厂值」的档：与 schema 的 none 对齐"
    );
    // 一行生效值里必须能看出是哪一档（`none` 与 `system` 行为相反，印成同一句话就是新谎）。
    let line = DownloadTuning::default().summary();
    assert!(line.contains("代理=none（直连）"), "{line}");
    let manual = DownloadTuning {
        proxy: ProxySetting::Manual {
            url: "http://127.0.0.1:7890".to_string(),
        },
        ..DownloadTuning::default()
    };
    assert!(
        manual
            .summary()
            .contains("代理=manual（http://127.0.0.1:7890）"),
        "{}",
        manual.summary()
    );
}
