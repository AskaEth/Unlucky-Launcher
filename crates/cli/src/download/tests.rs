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
