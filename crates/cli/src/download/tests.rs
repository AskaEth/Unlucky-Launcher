//! [`super`] 的单测（M203）：`launcher.json` 的 `download.*` 真的进到四条命令拿到的引擎里
//! （`install` / `java` / `pack` / `update`——`update` 复用 `install` 的装配，自更新只下一个
//! artifact，所以并发在那条路径上不可观测，随设置变的是每源重试次数与 gh-proxy 前缀）。
//!
//! 断言读的是**引擎的真实字段**——`config().max_concurrency` 与 `Semaphore` 的
//! `available_permits()`（`concurrency_probe`）——不是「函数被调用过」：后者在回落写错、
//! 或者命令又自建了一个裸引擎时照样绿。gh-proxy 那一条断言的是**候选链**（真行为），不是配置值。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use unlucky_core::download::config::{
    DownloadTuning, DEFAULT_MAX_ATTEMPTS_PER_SOURCE, DEFAULT_MAX_CONCURRENCY, KEY_CONCURRENCY,
};
use unlucky_core::download::types::{DownloadRequest, ResourceKind};
use unlucky_core::download::DownloadEngine;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::Globals;
use crate::context::Ctx;

/// 一次性临时目录（cli 没有 dev-dependency 的 tempfile；与 `commands/config/tests.rs` 同一手法）。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-cli-download-{tag}-{}-{seq}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 数据根钉在临时目录里：`launcher.json` 落在 `<dir>/.ULL/config/launcher.json`。
fn ctx_at(dir: &TempDir) -> Ctx {
    let globals = Globals {
        data_dir: Some(dir.path().to_path_buf()),
        ..Globals::default()
    };
    Ctx::resolve(&globals).expect("解析数据根")
}

/// 直接写 `launcher.json`——落点用 [`Ctx::setting_path_for`]（`ull config set` 用的那个表达式），
/// **不是**实现读侧用的 `ctx.launcher_config()`。
///
/// 两个表达式今天指向同一份文件（`download.*` 不是数据根键 ⇒ 回落 `launcher_config()`），但写侧
/// 走「用户改配置那条路」的 API，实现哪天改读别的文件时这条用例会红，而不是跟着一起漂移。
fn write_launcher_config(ctx: &Ctx, document: Value) -> PathBuf {
    let path = ctx.setting_path_for(KEY_CONCURRENCY);
    std::fs::create_dir_all(path.parent().expect("配置目录")).expect("建配置目录");
    std::fs::write(&path, serde_json::to_vec(&document).expect("序列化"))
        .expect("写 launcher.json");
    path
}

/// 「引擎真的按这个并发跑」：配置字段 + 闸门许可数，两处都读真实值。
fn assert_concurrency(engine: &DownloadEngine, expected: usize, case: &str) {
    assert_eq!(
        engine.config().max_concurrency,
        expected,
        "{case}：`EngineConfig` 里的并发"
    );
    assert_eq!(
        engine.concurrency_probe(),
        expected,
        "{case}：`Semaphore` 真实许可数"
    );
}

/// **`install` 吃设置**：`download.concurrency` 改了就变；同一次进程里再改一次也立刻变（不是只读一次）。
///
/// 抓得住的变异：`commands::install::build_engine` 换回裸 `EngineConfig::default()` ⇒ 3 / 5 两处红。
/// 抓不住的：真实下载时「只有 3 条连接在飞」的时序（离线验不了，见报告「没验的部分」）。
#[test]
fn install_engine_takes_the_concurrency_from_the_setting_store() {
    let dir = TempDir::new("install");
    let ctx = ctx_at(&dir);
    // 没有 launcher.json = 没配过 ⇒ 出厂值（这也是「回落」的第一档）。
    assert_concurrency(
        &crate::commands::install::build_engine(&ctx),
        DEFAULT_MAX_CONCURRENCY,
        "未配置",
    );

    write_launcher_config(&ctx, json!({ "download": { "concurrency": 3 } }));
    assert_concurrency(
        &crate::commands::install::build_engine(&ctx),
        3,
        "concurrency=3",
    );

    write_launcher_config(&ctx, json!({ "download": { "concurrency": 5 } }));
    assert_concurrency(
        &crate::commands::install::build_engine(&ctx),
        5,
        "concurrency=5",
    );
}

/// **`java` 吃设置**：并发与每个源的尝试次数都进引擎。
///
/// 抓得住的变异：`commands::java::build_engine` 换回裸 `EngineConfig::default()` ⇒ 探针停在 8、次数停在 3，红。
#[test]
fn java_engine_takes_the_concurrency_and_retry_from_the_setting_store() {
    let dir = TempDir::new("java");
    let ctx = ctx_at(&dir);
    write_launcher_config(
        &ctx,
        json!({ "download": { "concurrency": 3, "retry_limit": 4 } }),
    );

    let engine = crate::commands::java::build_engine(&ctx);
    assert_concurrency(&engine, 3, "concurrency=3");
    assert_eq!(
        engine.config().max_attempts_per_source,
        4,
        "retry_limit=4 要真的落到引擎上（出厂是 3）"
    );
}

/// **`pack` 吃设置**（四条命令各有一条独立断言：把任意一处改回裸引擎，只有那一条红）。
///
/// 抓得住的变异：`commands::pack::build_engine` 换回裸 `EngineConfig::default()` ⇒ 2 处红。
#[test]
fn pack_engine_takes_the_concurrency_from_the_setting_store() {
    let dir = TempDir::new("pack");
    let ctx = ctx_at(&dir);
    write_launcher_config(&ctx, json!({ "download": { "concurrency": 2 } }));
    assert_concurrency(
        &crate::commands::pack::build_engine(&ctx),
        2,
        "concurrency=2",
    );
}

/// **`update` 吃设置**（第 4 条路径，`commands/update.rs::engine`）：自更新也只拿出厂值是不行的。
///
/// 这条路径上「并发」实际不可观测（自更新只下**一个** artifact，闸门开 1 还是 8 都只有一条连接），
/// 但引擎参数是同一处装配出来的，用并发打点最省事；真正随设置变的是每源重试次数与 gh-proxy 前缀。
/// 抓得住的变异：`commands::update::engine` 换回裸 `EngineConfig::default()` ⇒ 4 ≠ 8 处红，另五条绿。
#[test]
fn update_engine_takes_the_concurrency_from_the_setting_store() {
    let dir = TempDir::new("update");
    let ctx = ctx_at(&dir);
    write_launcher_config(&ctx, json!({ "download": { "concurrency": 4 } }));
    assert_concurrency(&crate::commands::update::engine(&ctx), 4, "concurrency=4");
}

/// **`gh_proxy` 的 CLI 侧闭环**：手写 `"off"` ⇒ 候选链里真的没有 gh-proxy（读的是 `plan()`，不是配置值）。
///
/// 两段式：先在镜像优先的地区档案下确认**出厂**（空串 = 跟随内置）确实生成 gh-proxy 候选，再把它
/// 改成 `off` 确认候选消失——只断言后者的话，「本来就没有候选」会让用例白绿。
/// 抓得住的变异：`download::tuning` 不读 `KEY_GH_PROXY`（传 `None`）⇒ 第二段仍有 gh-proxy 候选，红；
/// 把空串当成关（`gh_proxy_override("")` 返回 `Off`）⇒ 第一段没有候选，红（回落用例的
/// `tuning == default` 同时红——两处都该红，实测如此）。
/// 抓不住的：真实网络下这个前缀能不能通（离线只验候选链的**生成**）。
#[test]
fn the_gh_proxy_setting_reaches_the_candidate_chain() {
    use unlucky_core::download::cache::now_ms;
    use unlucky_core::download::region::{store_cached, RegionProfile};

    let dir = TempDir::new("ghproxy");
    let ctx = ctx_at(&dir);
    // gh-proxy 只在「镜像优先」的地区档案下生成候选：先把该数据根的 region.json 钉住。
    let cache_root = ctx.layout().cache();
    store_cached(
        &cache_root.join("region.json"),
        &RegionProfile::mirror_first(now_ms()),
    )
    .expect("地区档案");
    let url = "https://github.com/o/r/releases/download/v1/a.jar";
    let chain = |ctx: &Ctx| -> Vec<String> {
        let request =
            DownloadRequest::new(ResourceKind::Other, url, ctx.layout().cache().join("a.jar"));
        crate::download::engine(ctx)
            .plan(&request, 0)
            .into_iter()
            .map(|candidate| candidate.url)
            .collect()
    };
    let has_gh_proxy = |chain: &[String]| chain.iter().any(|url| url.contains("gh-proxy"));

    // 出厂（未配置）：空串 = 跟随内置默认 ⇒ 有 gh-proxy 候选。
    assert!(has_gh_proxy(&chain(&ctx)), "出厂该套内置前缀");
    // 写 off：候选里不该再有 gh-proxy（其余候选仍在）。
    write_launcher_config(&ctx, json!({ "download": { "gh_proxy": "off" } }));
    let off = chain(&ctx);
    assert!(!has_gh_proxy(&off), "off 档还有 gh-proxy 候选：{off:?}");
    assert!(!off.is_empty(), "关掉加速不等于没有候选：{off:?}");
}

/// **反向断言**：键缺失 / 越界 / 负数 / 类型不对 ⇒ 回落出厂值，而且回落是**静默的**。
///
/// 「静默」在这里是结构性的：`crate::download::engine` 的返回类型没有错误通道，坏设置只进
/// `SettingStore::problems()`，调用方（安装流程）连一个 `?` 都不用加——`ull install` 不会因为一条
/// 写坏的设置而中断，只是按出厂值跑。
///
/// 抓得住的变异：删掉 `DownloadTuning::from_settings` 的区间过滤 ⇒ 9_999 那条把许可数变成 64，红；
/// 把 CLI 的读数改成 `expect`/报错 ⇒ 坏值文件这几档 panic 或编译不过，红。
/// 抓不住的：`problems()` 的内容与措辞（那是 core 的 `setting` 模块，本用例只借它证明文件真的坏）。
#[test]
fn missing_or_illegal_values_fall_back_to_the_factory_defaults_silently() {
    let dir = TempDir::new("fallback");
    let ctx = ctx_at(&dir);
    assert_eq!(
        super::tuning(&ctx),
        DownloadTuning::default(),
        "没有 launcher.json 就该是出厂调优"
    );

    for (case, document) in [
        (
            "越界",
            json!({ "download": { "concurrency": 9_999, "retry_limit": 99 } }),
        ),
        (
            "负数",
            json!({ "download": { "concurrency": -4, "retry_limit": -1 } }),
        ),
        (
            "类型不对",
            json!({ "download": { "concurrency": "八", "retry_limit": "很多" } }),
        ),
    ] {
        let written = write_launcher_config(&ctx, document);
        // 先证明这份文件**真的**是坏的（否则「回落」只是键不存在，用例就成了空断言）。
        let store = SettingStore::load(&written, launcher_schema());
        assert!(
            !store.problems().is_empty(),
            "{case}：这份文件该被判为有问题"
        );

        assert_eq!(
            super::tuning(&ctx),
            DownloadTuning::default(),
            "{case}：坏值必须回落出厂调优"
        );
        let engine = crate::commands::install::build_engine(&ctx);
        assert_concurrency(&engine, DEFAULT_MAX_CONCURRENCY, case);
        assert_eq!(
            engine.config().max_attempts_per_source,
            DEFAULT_MAX_ATTEMPTS_PER_SOURCE,
            "{case}：尝试次数也回落出厂值"
        );
    }
}

/// **M292 / P21 判据的 CLI 侧闭合**：三个超时键从 `launcher.json` 进到**生产路径**的引擎配置，而且
/// 改过设置时终端上那行说明写的是**生效值**。
///
/// 为什么断言 [`super::announce`]：台账原文的判据是「改完能在 `--dry-run` 或日志里看到生效值」。
/// CLI 没有打印调优的 `--dry-run`（那几处 `--dry-run` 只出计划清单 / 版本串），所以这一行走
/// stderr，而它的内容由 `announce` 产出——**断言这个函数就是断言终端上那行**（不是另拼一份旁证）。
/// 走 stderr 而不是 `logs/launcher.log` 的理由见 `announce` 的文档：那个日志的目录是进程级全局，
/// 在装配点调 `set_dir` 会让一个读路径改全局状态（M292 第一版就是这么写的，`commands::sessions`
/// 的 launcher.log 用例当场红了）。
///
/// 抓得住：① `download::tuning` 少读一个键 ⇒ 引擎字段与那行说明同时红；② `announce` 对出厂值也返回
/// `Some` ⇒ 第二段红（每个 `ull install` 都多一行噪声）；③ 把汇总写成常量 ⇒ 说明里的数字对不上。
/// 抓不住：真机慢链路上「预算到底够不够」（离线环境没有真链路，见 M292 报告的未验项）。
#[test]
fn the_timeout_settings_reach_the_engine_and_the_console_line() {
    use std::time::Duration;

    let dir = TempDir::new("timeouts");
    let ctx = ctx_at(&dir);
    write_launcher_config(
        &ctx,
        json!({ "download": {
            "throughput_floor_kib": 16,
            "stall_timeout_secs": 300,
            "response_timeout_secs": 180,
        }}),
    );

    // 出厂档不该打那一行（否则每个命令都多一行噪声）。
    assert!(
        super::announce(&DownloadTuning::default()).is_none(),
        "出厂值不该打说明"
    );

    // 生产路径：`ull install` 取引擎的那个入口（四条命令共用这一处装配）。
    let engine = crate::commands::install::build_engine(&ctx);
    assert_eq!(
        engine.config().min_avg_throughput,
        16 * 1024,
        "throughput_floor_kib=16 ⇒ 引擎里是 16 KiB/s（字节/秒）"
    );
    assert_eq!(
        engine.config().chunk_stall_timeout,
        Duration::from_secs(300)
    );
    assert_eq!(engine.config().response_timeout, Duration::from_secs(180));

    // 终端上那行（同一份 `summary()`，同一个 `announce`）。
    let line = super::announce(&super::tuning(&ctx)).expect("改过设置 ⇒ 该有一行说明");
    assert!(line.contains("吞吐下限=16KiB/s"), "{line}");
    assert!(line.contains("无进展窗口=300s"), "{line}");
    assert!(line.contains("单次请求上界=180s"), "{line}");

    // 不设 ⇒ 引擎回到出厂值，终端也不再打说明（「不设 = 当前行为」的 CLI 侧证据）。
    write_launcher_config(&ctx, json!({ "download": {} }));
    let engine = crate::commands::install::build_engine(&ctx);
    assert_eq!(
        engine.config().response_timeout,
        Duration::from_secs(60),
        "未设置 ⇒ 出厂 60s"
    );
    assert!(
        super::announce(&super::tuning(&ctx)).is_none(),
        "全出厂值不该打说明"
    );

    // 非法值：三个键一起写成 0 / 越界 ⇒ 回落出厂，且说明行不出现（不是「打一行假的」）。
    write_launcher_config(
        &ctx,
        json!({ "download": {
            "throughput_floor_kib": 0,
            "stall_timeout_secs": 0,
            "response_timeout_secs": 99_999,
        }}),
    );
    assert_eq!(super::tuning(&ctx), DownloadTuning::default());
    assert!(super::announce(&super::tuning(&ctx)).is_none());
}

/// 一个 URL 在 CLI 装配出来的引擎里的**源名顺序**（`plan()` 是引擎真正的选源决策点）。
fn plan_sources(engine: &DownloadEngine) -> Vec<String> {
    let url = "https://libraries.minecraft.net/com/google/guava/guava/32.0.0/guava-32.0.0.jar";
    let dest = std::env::temp_dir().join("ull-cli-plan-guava.jar");
    let request = DownloadRequest::new(ResourceKind::Library, url.to_string(), dest);
    engine
        .plan(&request, 0)
        .into_iter()
        .map(|c| c.source)
        .collect()
}

/// **M294 r1（P2-1）**：两键进 CLI 四条命令的生产装配点，终端那行说明也含它们。
///
/// 判据是**候选顺序**：两段都让设置与探测结论相反，没接上时必红（变异：`download-settings.md` M15）。
#[test]
fn the_source_selection_settings_reach_the_cli_engine() {
    use unlucky_core::download::region::{store_cached, RegionProfile};

    let dir = TempDir::new("source-selection");
    let ctx = ctx_at(&dir);
    let cache = ctx.layout().cache();
    std::fs::create_dir_all(&cache).expect("建缓存根");
    let now = unlucky_core::download::cache::now_ms();
    let region_json = cache.join("region.json");

    // 探测=镜像优先 + 设置 official ⇒ 官方第一条（未接线时是 bmclapi）。
    store_cached(&region_json, &RegionProfile::mirror_first(now)).expect("档案");
    let mirror = json!({ "download": { "mirror_preference": "official" } });
    write_launcher_config(&ctx, mirror);
    let engine = crate::commands::install::build_engine(&ctx);
    assert_eq!(plan_sources(&engine), ["official", "bmclapi"], "官方第一条");
    let line = super::announce(&super::tuning(&ctx)).expect("该有一行");
    assert!(line.contains("镜像偏好=official"), "{line}");
    assert!(line.contains("地区=auto"), "{line}");

    // 探测=直连 + 设置 cn ⇒ 镜像第一条（未接线时是 official）。
    store_cached(&region_json, &RegionProfile::direct(now)).expect("档案");
    write_launcher_config(&ctx, json!({ "download": { "region": "cn" } }));
    let engine = crate::commands::install::build_engine(&ctx);
    assert_eq!(plan_sources(&engine), ["bmclapi", "official"], "镜像第一条");
    let line = super::announce(&super::tuning(&ctx)).expect("该有一行");
    assert!(line.contains("地区=cn"), "{line}");
    assert!(line.contains("镜像偏好=auto"), "{line}");
}

/// 代理两键的 CLI 侧守门（M295）：单开一个文件——本文件已贴着 400 行硬线。
#[path = "tests/proxy.rs"]
mod proxy;
