//! `download::config` 的单测：**从 `config.rs` 拆出只为守 400 行硬线**（M289 加超时字段前先纯搬移，逐字节未改）。

use super::*;
use crate::download::region::RegionProfile;
use crate::setting::keys::{launcher_schema, launcher_version};

/// **守门③**：schema 的默认值 / 区间 / 键名与 `EngineConfig::default()` / 调优默认值取同一组常量。
/// 变异证明（只改实现侧）：`EngineConfig::default()` 的并发写回 64、schema 默认值写成 16、
/// `MAX_CONCURRENCY` 改了而 schema 仍写 64 ⇒ 各自红。
#[test]
fn the_schema_contract_is_single_sourced() {
    let schema = launcher_schema();
    assert_eq!(schema.version(), launcher_version());
    let cfg = EngineConfig::default();
    let tune = DownloadTuning::default();

    // 并发 1~64（0 张许可没意义）；尝试次数 **0**~10（P2-1：0 = 只试一次，旧配置里真有）。
    let contract: [(&str, i64, i64, usize); 2] = [
        (
            KEY_CONCURRENCY,
            i64::try_from(DEFAULT_MAX_CONCURRENCY).expect("装得下"),
            1,
            MAX_CONCURRENCY,
        ),
        (
            KEY_RETRY_LIMIT,
            i64::from(DEFAULT_MAX_ATTEMPTS_PER_SOURCE),
            0,
            usize::try_from(MAX_ATTEMPTS_PER_SOURCE).expect("装得下"),
        ),
    ];
    for (key, default, min, max) in contract {
        let spec = schema.spec(key).expect("键已登记");
        assert_eq!(spec.default, serde_json::json!(default), "{key}");
        assert_eq!(
            spec.kind,
            crate::setting::spec::ValueKind::Int {
                min,
                max: i64::try_from(max).expect("装得下"),
            },
            "{key}"
        );
    }
    assert_eq!(cfg.max_concurrency, DEFAULT_MAX_CONCURRENCY);
    assert_eq!(tune.max_concurrency, DEFAULT_MAX_CONCURRENCY);
    assert_eq!(cfg.max_attempts_per_source, DEFAULT_MAX_ATTEMPTS_PER_SOURCE);
    assert_eq!(
        tune.max_attempts_per_source,
        DEFAULT_MAX_ATTEMPTS_PER_SOURCE
    );
}

/// `SettingSpec.doc` 是 `&'static str`（`concat!` 插不进常量），描述里的数字只能手写——这条钉住它，
/// 改常量不改文案就红（`catalog.rs` 会把这些 doc 当 JSON schema 的 description 发出去）。
#[test]
fn the_key_docs_quote_the_real_limits() {
    let schema = launcher_schema();
    let doc = |key: &str| schema.spec(key).expect("键已登记").doc;
    assert!(doc(KEY_CONCURRENCY).contains(&format!("1~{MAX_CONCURRENCY}")));
    assert!(doc(KEY_CONCURRENCY).contains(&format!("出厂 {DEFAULT_MAX_CONCURRENCY}")));
    assert!(doc(KEY_RETRY_LIMIT).contains(&format!("出厂 {DEFAULT_MAX_ATTEMPTS_PER_SOURCE}")));
}

/// 未填 / 越界回落默认（不阻断下载）；**`retry_limit: 0` 是合法值**，语义 = 只试一次。
///
/// 变异证明：把并发的区间过滤删掉 ⇒ `0` / `9999` 两条红；把尝试次数的 `.max(1)` 删掉 ⇒
/// `0 → 1` 那条红（引擎会拿到 0 次尝试）；把 `0` 判成越界 ⇒ 同样红。
#[test]
fn missing_or_illegal_values_fall_back_to_the_defaults() {
    assert_eq!(
        DownloadTuning::from_settings(None, None, None),
        DownloadTuning::default()
    );

    let just_over = i64::try_from(MAX_CONCURRENCY).expect("上限装得下") + 1;
    for bad in [0_i64, -1, 9_999, just_over] {
        let tune = DownloadTuning::from_settings(Some(bad), None, None);
        assert_eq!(tune.max_concurrency, DEFAULT_MAX_CONCURRENCY, "{bad}");
    }
    // 0 与 1 都 = 只试一次：旧配置里的 0 不许回落成出厂 3（那正是「界面 0、引擎 3」）。
    for zero_or_one in [0_i64, 1] {
        let tune = DownloadTuning::from_settings(None, Some(zero_or_one), None);
        assert_eq!(tune.max_attempts_per_source, 1, "{zero_or_one}");
    }
    for bad in [-1_i64, 9_999] {
        let tune = DownloadTuning::from_settings(None, Some(bad), None);
        assert_eq!(
            tune.max_attempts_per_source,
            DEFAULT_MAX_ATTEMPTS_PER_SOURCE
        );
    }

    let tune = DownloadTuning::from_settings(Some(2), Some(2), None);
    assert_eq!(tune.max_concurrency, 2);
    assert_eq!(tune.max_attempts_per_source, 2);
}

/// 三态：**空串 ≠ 关**（这条是「设置项说假话」的高发点，M197 的键语义同源）。
#[test]
fn empty_gh_proxy_means_follow_builtin_not_off() {
    assert_eq!(gh_proxy_override(""), GhProxyOverride::Builtin);
    assert_eq!(gh_proxy_override("   "), GhProxyOverride::Builtin);
    for off in [
        "off", "OFF", " none ", "direct", "disable", "disabled", "no",
    ] {
        assert_eq!(gh_proxy_override(off), GhProxyOverride::Off, "{off}");
    }
    assert_eq!(
        gh_proxy_override(" https://gh.example/ "),
        GhProxyOverride::Prefix("https://gh.example/".to_string())
    );
    // 未填（键不存在）也是 Builtin，不是 Off。
    assert_eq!(
        DownloadTuning::from_settings(None, None, None).gh_proxy,
        GhProxyOverride::Builtin
    );
}

/// `apply_gh_proxy` 的三档真的改到档案上、并且**改到候选链上**（`expand()` 读的就是档案里那个
/// 前缀；自定义前缀那一档的链断言在 `shared.rs`）。变异证明：`Off` 分支改成原样返回 ⇒ `off` 红。
#[test]
fn gh_proxy_override_reaches_the_profile_and_the_chain() {
    use crate::download::mirrors::{MirrorTable, SOURCE_GH_PROXY};
    use crate::download::types::ResourceKind;

    let url = "https://github.com/o/r/releases/download/v1/a.jar";
    let table = MirrorTable::builtin();
    let sources = |profile: &RegionProfile| -> Vec<String> {
        table
            .expand(url, ResourceKind::Other, profile)
            .into_iter()
            .map(|candidate| candidate.source)
            .collect()
    };

    let mirror_first = RegionProfile::mirror_first(0);
    let kept = DownloadTuning::default().apply_gh_proxy(mirror_first.clone());
    assert_eq!(kept.gh_proxy, mirror_first.gh_proxy, "Builtin 不该动档案");
    assert!(
        sources(&kept).contains(&SOURCE_GH_PROXY.to_string()),
        "出厂套前缀"
    );

    let off =
        DownloadTuning::from_settings(None, None, Some("off")).apply_gh_proxy(mirror_first.clone());
    assert!(
        !sources(&off).contains(&SOURCE_GH_PROXY.to_string()),
        "关了就没候选"
    );

    let prefix = DownloadTuning::from_settings(None, None, Some("https://gh.example/"))
        .apply_gh_proxy(mirror_first);
    assert_eq!(prefix.gh_proxy.as_deref(), Some("https://gh.example/"));
    assert!(prefix.gh_proxy_enabled(), "前缀生效");
}

/// 每个数据根的调优互不串味（一个根改成 2 不该改另一个根的引擎）。
/// 变异证明：把 `wanted`/`publish` 的键改回单一进程级槽位 ⇒ 第二个根读到 2，红。
#[test]
fn tuning_is_per_cache_root_and_no_change_is_reported_as_such() {
    let a = PathBuf::from("/tmp/ull-tuning-a");
    let b = PathBuf::from("/tmp/ull-tuning-b");
    assert_eq!(wanted(&a), DownloadTuning::default());

    let changed = DownloadTuning::from_settings(Some(2), None, None);
    assert!(publish(&a, changed.clone()), "真变了要报变更");
    assert_eq!(wanted(&a), changed);
    assert!(!publish(&a, changed.clone()), "同值不该报变更");
    assert_eq!(wanted(&b), DownloadTuning::default(), "别的根不受影响");

    assert!(
        publish(&a, DownloadTuning::default()),
        "恢复默认也是一次变更"
    );
    assert_eq!(wanted(&a), DownloadTuning::default());
}

/// M292：三个超时/阈值键的 schema 默认值与范围、`EngineConfig::default()`、`DownloadTuning::default()`
/// 三处同源，且**默认值与 M289 的硬编码逐值相等**（不设 = 当前行为）。
///
/// 变异证明：把 schema 默认值写成 64、把 `EngineConfig::default().response_timeout` 写回 30s、
/// 把 `DEFAULT_THROUGHPUT_FLOOR_KIB` 从 32 改成 16 ⇒ 各自红；宿主常量 `KEY_*` 改名而 schema 没跟
/// ⇒ `spec()` 取不到，第一条红。
#[test]
fn the_timeout_keys_are_single_sourced_and_default_to_todays_values() {
    let schema = launcher_schema();
    let cfg = EngineConfig::default();
    let tune = DownloadTuning::default();

    // (键名, schema 默认值, 下界, 上界, EngineConfig 里的生效值, DownloadTuning 里的生效值)
    let contract: [(&str, i64, i64, i64, Duration, Duration); 2] = [
        (
            KEY_RESPONSE_TIMEOUT,
            i64::try_from(DEFAULT_RESPONSE_TIMEOUT.as_secs()).expect("装得下"),
            i64::from(MIN_RESPONSE_TIMEOUT_SECS),
            i64::from(MAX_RESPONSE_TIMEOUT_SECS),
            cfg.response_timeout,
            tune.response_timeout,
        ),
        (
            KEY_CHUNK_STALL,
            i64::try_from(DEFAULT_CHUNK_STALL.as_secs()).expect("装得下"),
            i64::from(MIN_CHUNK_STALL_SECS),
            i64::from(MAX_CHUNK_STALL_SECS),
            cfg.chunk_stall_timeout,
            tune.chunk_stall_timeout,
        ),
    ];
    for (key, default, min, max, from_cfg, from_tune) in contract {
        let spec = schema.spec(key).expect("键已登记");
        assert_eq!(spec.default, serde_json::json!(default), "{key}");
        assert_eq!(
            spec.kind,
            crate::setting::spec::ValueKind::Int { min, max },
            "{key}"
        );
        assert_eq!(from_cfg, Duration::from_secs(default as u64), "{key}");
        assert_eq!(from_tune, from_cfg, "{key}：调优默认与引擎默认必须一致");
    }

    // 吞吐下限：schema 用 KiB/s，引擎里是字节/秒，两者只差一个 ×1024。
    let floor = schema.spec(KEY_THROUGHPUT_FLOOR).expect("键已登记");
    assert_eq!(
        floor.default,
        serde_json::json!(32),
        "188MB/32KiB/s ≈ 100min"
    );
    assert_eq!(
        floor.kind,
        crate::setting::spec::ValueKind::Int {
            min: i64::from(MIN_THROUGHPUT_FLOOR_KIB),
            max: i64::from(MAX_THROUGHPUT_FLOOR_KIB),
        }
    );
    assert_eq!(
        cfg.min_avg_throughput,
        crate::download::stall::MIN_AVG_THROUGHPUT
    );
    assert_eq!(tune.min_avg_throughput, cfg.min_avg_throughput);
    assert_eq!(
        u64::from(DEFAULT_THROUGHPUT_FLOOR_KIB) * 1024,
        cfg.min_avg_throughput,
        "KiB/s 口径的出厂值与字节/秒的出厂值必须相等"
    );

    // 不设任何键 = 今天的行为（这一条是「默认值逐值相等」的总结论）。
    let untouched =
        DownloadTuning::from_raw(RawTuning::default()).apply_to(EngineConfig::default());
    assert_eq!(untouched.response_timeout, cfg.response_timeout);
    assert_eq!(untouched.chunk_stall_timeout, cfg.chunk_stall_timeout);
    assert_eq!(untouched.min_avg_throughput, cfg.min_avg_throughput);
    assert_eq!(untouched.max_concurrency, cfg.max_concurrency);
    assert_eq!(
        untouched.max_attempts_per_source,
        cfg.max_attempts_per_source
    );
}

/// **键变更真的进到引擎配置**（P21 判据的 core 侧一半）：改设置 ⇒ `apply_to` 后引擎拿到新值 ⇒ 期限跟着变。
///
/// **抓得住**（三键逐一变异过，删掉 `apply_to` 里对应赋值即红）：`response_timeout` ⇒ 278 行
/// `60s/180s`；`chunk_stall_timeout` ⇒ 279 行 `120s/300s`；`min_avg_throughput` ⇒ 280 行
/// `32768/16384`。另外还抓 `from_raw` 的 KiB→字节换算（少乘 1024 ⇒ 第三条红）与 `chunk_job_budget`
/// 忽略下限（改回常量 ⇒ 最后两条期限断言红）。
///
/// **抓不住**：①**宿主有没有调用 `tuning`/`apply_to`** —— 前半段归宿主用例（CLI 读 `engine.config()`、
/// presenter 走 `host::engine::engine`）；②**跑的是不是这棵树的二进制** —— M292 返工就被这一类骗过
/// 一轮：异树/旧目标目录里的二进制正好在 279 行报 `120s/300s`，源码却从一开始就有那行。
#[test]
fn changed_settings_reach_the_engine_config_and_the_budget() {
    let tune = DownloadTuning::from_raw(RawTuning {
        throughput_floor_kib: Some(16),
        stall_timeout_secs: Some(300),
        response_timeout_secs: Some(180),
        ..RawTuning::default()
    });
    assert_eq!(tune.response_timeout, Duration::from_secs(180));
    assert_eq!(tune.chunk_stall_timeout, Duration::from_secs(300));
    assert_eq!(tune.min_avg_throughput, 16 * 1024);

    let cfg = tune.apply_to(EngineConfig::default());
    assert_eq!(cfg.response_timeout, Duration::from_secs(180));
    assert_eq!(cfg.chunk_stall_timeout, Duration::from_secs(300));
    assert_eq!(cfg.min_avg_throughput, 16 * 1024);
    // 没被调优管的字段保持传入值（不是被 `EngineConfig::default()` 冲掉）。
    assert_eq!(cfg.chunk_job_floor, Duration::from_secs(600));
    assert_eq!(cfg.verify_timeout, Duration::from_secs(120));

    // 期限真的跟着变：188MB 在 16 KiB/s 上 ≈ 201 min，出厂值是 100.3 min。
    let total = 188 * 1024 * 1024;
    assert_eq!(
        cfg.chunk_job_budget(Some(total)),
        Duration::from_secs(total / (16 * 1024)),
        "调低吞吐下限 ⇒ 总期限变长（这正是慢链路用户改它的收益）"
    );
    assert!(
        cfg.chunk_job_budget(Some(total)) > EngineConfig::default().chunk_job_budget(Some(total)),
        "与出厂值的期限必须不同，否则这条用例证明不了「改动能生效」"
    );
    // 等响应头那一档是**单次请求**的界：它不参与大文件的期限，改它不会把慢链路判死。
    assert_eq!(cfg.chunk_job_budget(Some(total)), {
        let mut same = cfg.clone();
        same.response_timeout = Duration::from_secs(600);
        same.chunk_job_budget(Some(total))
    });
}

/// **非法值不得穿透**：吞吐下限 `0`（除零）、窗口 `0`、上界 `0`、负数、超上界一律回落出厂值，
/// 而且引擎侧即使真的收到 `0`（手拼 `EngineConfig`）也不会 panic / 除零。
///
/// 变异证明：把 `secs_in_range` 的下界判断删掉 ⇒ `0` 那几条红；把 `min_avg_throughput` 的区间
/// 过滤删掉 ⇒ `0` 与 `999999` 两条红；把 `job_budget` 里的 `min_throughput.max(1)` 删掉 ⇒
/// 最后一条 panic（除零）。
#[test]
fn zero_and_out_of_range_timeouts_fall_back_instead_of_flowing_through() {
    let factory = DownloadTuning::default();
    for bad in [0_i64, -1, -3600, 99_999] {
        let tune = DownloadTuning::from_raw(RawTuning {
            throughput_floor_kib: Some(bad),
            stall_timeout_secs: Some(bad),
            response_timeout_secs: Some(bad),
            ..RawTuning::default()
        });
        assert_eq!(tune, factory, "{bad}：三个键都该回落出厂值，不许穿透");
    }

    // 边界值本身是合法值（区间是闭区间）。
    let edge = DownloadTuning::from_raw(RawTuning {
        throughput_floor_kib: Some(i64::from(MIN_THROUGHPUT_FLOOR_KIB)),
        stall_timeout_secs: Some(i64::from(MIN_CHUNK_STALL_SECS)),
        response_timeout_secs: Some(i64::from(MAX_RESPONSE_TIMEOUT_SECS)),
        ..RawTuning::default()
    });
    assert_eq!(
        edge.min_avg_throughput,
        u64::from(MIN_THROUGHPUT_FLOOR_KIB) * 1024
    );
    assert_eq!(edge.chunk_stall_timeout, Duration::from_secs(30));
    assert_eq!(edge.response_timeout, Duration::from_secs(600));

    // 引擎侧纵深防御：手拼的 `EngineConfig` 带着 0 也不许 panic / 除零。
    let zero = EngineConfig {
        min_avg_throughput: 0,
        ..EngineConfig::default()
    };
    assert_eq!(
        zero.chunk_job_budget(Some(1024)),
        Duration::from_secs(1024),
        "0 按 1 B/s 算（有限值），不是除零也不是无界"
    );
    assert_eq!(zero.chunk_job_budget(Some(0)), zero.chunk_job_floor);
}

/// 描述里的数字（`catalog.rs` 会把它当 JSON Schema 的 `description` 发给 AI）必须是真的。
///
/// 变异证明：把 `MAX_CHUNK_STALL_SECS` 改成 900 而不改文案 ⇒ 第二条红；把出厂 32 改成别的
/// ⇒ 第一条红。
#[test]
fn the_timeout_key_docs_quote_the_real_limits() {
    let schema = launcher_schema();
    let doc = |key: &str| schema.spec(key).expect("键已登记").doc;
    assert!(doc(KEY_THROUGHPUT_FLOOR).contains(&format!(
        "{MIN_THROUGHPUT_FLOOR_KIB}~{}",
        MAX_THROUGHPUT_FLOOR_KIB
    )));
    assert!(doc(KEY_THROUGHPUT_FLOOR).contains(&format!("出厂 {DEFAULT_THROUGHPUT_FLOOR_KIB}")));
    assert!(
        doc(KEY_CHUNK_STALL).contains(&format!("{MIN_CHUNK_STALL_SECS}~{MAX_CHUNK_STALL_SECS}"))
    );
    assert!(doc(KEY_CHUNK_STALL).contains(&format!("出厂 {}", DEFAULT_CHUNK_STALL.as_secs())));
    assert!(doc(KEY_RESPONSE_TIMEOUT).contains(&format!(
        "{MIN_RESPONSE_TIMEOUT_SECS}~{MAX_RESPONSE_TIMEOUT_SECS}"
    )));
    assert!(
        doc(KEY_RESPONSE_TIMEOUT).contains(&format!("出厂 {}", DEFAULT_RESPONSE_TIMEOUT.as_secs()))
    );
}

/// 日志行里能看见「引擎实际拿到了什么」（P21 判据要求的「日志里看到生效值」的 core 侧形状）。
///
/// 变异证明：把 `summary()` 里的 `min_avg_throughput / 1024` 写死成 32 ⇒ 第一条红；返回常数字符串
/// ⇒ 两条都红。
#[test]
fn the_summary_spells_out_the_effective_values() {
    let factory = DownloadTuning::default().summary();
    assert!(factory.contains("吞吐下限=32KiB/s"), "{factory}");
    assert!(factory.contains("无进展窗口=120s"), "{factory}");
    assert!(factory.contains("单次请求上界=60s"), "{factory}");

    let slow = DownloadTuning::from_raw(RawTuning {
        throughput_floor_kib: Some(16),
        stall_timeout_secs: Some(600),
        ..RawTuning::default()
    })
    .summary();
    assert!(slow.contains("吞吐下限=16KiB/s"), "{slow}");
    assert!(slow.contains("无进展窗口=600s"), "{slow}");
}
