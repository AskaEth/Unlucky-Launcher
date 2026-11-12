//! 进程级共享引擎：**一个数据根一个 `DownloadEngine`**，跨请求复用。
//!
//! # 为什么要有这一层（M165）
//!
//! 在此之前，宿主侧每处理一次「请求」（一次检索 / 一次详情 / 一次装文件 / 一次拉清单）都会
//! 现建一个引擎（`DownloadEngine::new`）。建的时候什么都会重新来一遍，于是下面这些**跨请求
//! 本该积累的状态每次都清零**：
//!
//! | 引擎里的东西 | 每建一次就丢什么 |
//! |--------------|------------------|
//! | `reqwest::Client`（`http::build_client`） | 整条连接池：每个请求重新 TCP + TLS 握手 |
//! | `Semaphore`（`max_concurrency`） | 限流变成「每请求 64」，N 个并发请求 = 64×N 条在飞 |
//! | `HealthRegistry`（`open_in`） | 从盘上重读 `health.json`；`last_save_ms` 被重置成「现在」⇒ 收尾那次 `flush()` 落在 5s 节流窗口里直接返回，**这个短命引擎记下的失败数一并丢掉**（只有「进/出冷却」会立刻落盘） |
//! | `ContentCache`（`open`） | 从盘上重读并解析整份 `index.json`；`persist()` 用**本请求这一份内存索引**覆写全文件 ⇒ 两个并发请求各自覆写，后写的把先写的登记条目冲掉 |
//! | `MirrorTable::builtin()` | 重新解析一遍内置 `mirrors.json` |
//! | 地区档案 + `Jitter` | 重读 `region.json`；抖动序列重新播种 |
//!
//! 失败分类、镜像回退、`Retry-After` 语义一概不动——复用只改「谁来建、建几次」，不改
//! [`DownloadEngine`] 内部的任何判定。
//!
//! # 边界（谁持有 / 活多久 / 并发多少）
//!
//! * **谁持有**：本模块的进程级登记处（`OnceLock<Mutex<…>>`），键是**数据根的缓存目录**
//!   （`<launcher_home>/cache`）。同一数据根的多次请求拿到同一个 `Arc<DownloadEngine>`；
//!   不同数据根各有一份，缓存/健康度不会串味（有测试钉住这一条）。
//! * **活多久**：跟随进程。登记处**不做淘汰**——淘汰会让后续请求建出第二个引擎，同一个根上
//!   同时存在两条连接池，等于把要修的问题换个地方复现。要让地区判定等「一次性派生值」重算，
//!   用 [`forget`] 显式失效（设置页「重新检测地区」就这么做）。
//! * **`forget` 到什么范围**：只让登记处不再持有那一份。**已经拿到 `Arc` 的人继续用旧的**：
//!   短命请求自然结束（它们的窗口只有一次下载），但**长命持有者必须自己重建**——presenter 的
//!   自更新子系统（`host::update::Factory`）从 `CoreHost::build` 起就把引擎存在字段里，
//!   只 forget 的话它会一直用旧实例（含旧地区档案）直到进程结束。所以「重新检测地区」那条
//!   设置动作除了 [`forget`] 还调 `CoreHost::refresh_update_factory()` 把新引擎接过去
//!   （M165 复审 P2-1：这是本模块文档原先说漏的一半）。
//! * **并发多少**（M193）：容量来自设置 `download.concurrency`（出厂 8、硬上限 64，见 [`set_tuning`]），
//!   而且是**进程级**上限——多请求并发时分摊这批许可，不是「每请求 8」。
//! * **镜像切换 / 失效**：一律走引擎原有的候选规划 + 健康度冷却，本模块不参与判定；唯一可见的
//!   差别是健康度记忆现在真的跨请求累积（这本就是它存在的理由）。
//!
//! # 与 [`super::blocking::block_on`] 的绑定关系
//!
//! `reqwest` 的连接池绑在**驱动连接的运行时**上：引擎复用了而每次 `block_on` 都新起一个运行时，
//! 池里的空闲连接就属于已销毁的运行时，复用会退化成「拿到的都是死连接」。所以引擎复用必须配
//! 运行时常驻（`blocking` 那一头一并改成进程内唯一运行时）。两者是同一件事的两半。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::cache::{now_ms, ContentCache};
use super::config::{publish, wanted, DownloadTuning, EngineConfig};
use super::engine::DownloadEngine;
use super::health::HealthRegistry;
use super::mirrors::MirrorTable;
use super::region::{decide_from, load_cached, store_cached};
use super::region_system::detect_signals;

mod tuned;
pub use tuned::engine_with_tuning;

/// 登记处：缓存根 → 引擎。
fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<DownloadEngine>>> {
    static ENGINES: OnceLock<Mutex<HashMap<PathBuf, Arc<DownloadEngine>>>> = OnceLock::new();
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 「这个根建过几次引擎」的账本（守卫/诊断用，**不**随 [`forget`] 清零）。
///
/// 与 [`registry`] **分两把锁**：记账发生在 `DownloadEngine::new` 里（见 [`record_build`]），
/// 而 [`engine_for`] 是**持着**登记处那把锁去调构造的——合成一把锁就是自己等自己。
fn ledger() -> &'static Mutex<HashMap<PathBuf, usize>> {
    static BUILDS: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();
    BUILDS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缓存根的键：去掉尾部分隔符（`D:/x/cache` 与 `D:/x/cache/` 视作同一根）。
fn key_of(cache_root: &Path) -> PathBuf {
    let text = cache_root.to_string_lossy();
    let trimmed = text.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        cache_root.to_path_buf()
    } else {
        PathBuf::from(trimmed)
    }
}

/// 取该数据根上的共享引擎（第一次调用时构造，之后一律复用）。
///
/// 构造过程只做本地 I/O（读 `region.json` / `health.json` / `index.json`，解析内置
/// `mirrors.json`），**不联网**；因此持锁构造不会把网络等待带进临界区。
#[must_use]
pub fn engine_for(cache_root: impl AsRef<Path>) -> Arc<DownloadEngine> {
    let key = key_of(cache_root.as_ref());
    let mut engines = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(engine) = engines.get(&key) {
        return Arc::clone(engine);
    }
    let engine = Arc::new(build(&key));
    engines.insert(key, Arc::clone(&engine));
    engine
}

/// 让该数据根的共享引擎失效（下次 [`engine_for`] 重新构造）。
///
/// **只让登记处不再持有那一份**：已经拿到 `Arc` 的人继续用旧的。短命请求自然结束；长命持有者
/// （presenter 的自更新 `Factory`）必须自己重建——「重新检测地区」那条设置动作就补调了
/// `CoreHost::refresh_update_factory()`。返回是否真淘汰了一份。
pub fn forget(cache_root: impl AsRef<Path>) -> bool {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&key_of(cache_root.as_ref()))
        .is_some()
}

/// 记一笔「这个缓存根上又造了一个引擎」。
///
/// **唯一调用点是 [`DownloadEngine::new`]**（不是 [`engine_for`]）：记账卡在构造这个卡口上，
/// [`constructions`] 才是「进程里真实构造过几次」，与谁造的无关（M165 复审备注 2）。
pub(crate) fn record_build(cache_root: &Path) {
    *ledger()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(key_of(cache_root))
        .or_default() += 1;
}

/// 该数据根在本进程里**实际构造过几次引擎**（不论谁构造的）：复用成立时停在 `1`（`forget`
/// 之后每重建一次 +1）。只断言「结果一样」测不出复用——引擎建几次都能下同一个文件。
#[must_use]
pub fn constructions(cache_root: impl AsRef<Path>) -> usize {
    ledger()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key_of(cache_root.as_ref()))
        .copied()
        .unwrap_or(0)
}

/// 真实构造（**presenter 侧**唯一一处 `DownloadEngine::new` 的宿主装配；CLI 走 [`engine_with_tuning`]）。
/// 装配参数从该数据根的 [`wanted`] 取，不用裸 `EngineConfig::default()`（M193）：并发 → `Semaphore`
/// 容量、尝试次数 → `attempts_for()` 上限、gh_proxy → 地区档案（`expand()` 只认档案里那个前缀）。
fn build(cache_root: &Path) -> DownloadEngine {
    let now = now_ms();
    let tune = wanted(cache_root);
    let region_path = cache_root.join("region.json");
    let region = load_cached(&region_path, now).unwrap_or_else(|| {
        let profile = decide_from(&detect_signals(), now);
        let _ = store_cached(&region_path, &profile);
        profile
    });
    DownloadEngine::new(
        tune.apply_to(EngineConfig::default()),
        Arc::new(MirrorTable::builtin()),
        Arc::new(tune.apply_gh_proxy(region)),
        Arc::new(HealthRegistry::open_in(cache_root, now)),
        Arc::new(ContentCache::open(cache_root)),
    )
}

/// 换该数据根的下载调优（宿主取引擎前 / 设置变更时调用），返回**是否真的变了**。变了就清掉该根的
/// 登记项，下次 [`engine_for`] 按新参数重建——否则「改了设置要重启才生效」。已拿到 `Arc` 的长命
/// 持有者继续用旧的（与 [`forget`] 同一条边界）；自更新 `Factory` 由 dispatch 侧换引擎。
/// **锁序**：`publish` 的锁先释放再拿登记处的锁，[`build`] 反之——两段不嵌套。
pub fn set_tuning(cache_root: impl AsRef<Path>, next: DownloadTuning) -> bool {
    let key = key_of(cache_root.as_ref());
    if !publish(&key, next) {
        return false;
    }
    let _ = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&key);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例一个独立数据根（登记处与调优都是进程级、按根存，键撞了会互相干扰）。
    fn root(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("ull-shared-{tag}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建缓存根");
        dir
    }

    /// 守卫：同一个根上「多次请求」只构造一个引擎（**断言构造次数，不是只看结果**）。
    ///
    /// 抓得住的变异：把 [`engine_for`] 里的 `engines.get` 命中分支删掉（每次现建），
    /// 本用例立刻红在 `constructions == 1` 与 `Arc::ptr_eq` 两处。
    #[test]
    fn one_engine_per_cache_root() {
        let dir = root("one");
        let first = engine_for(&dir);
        let second = engine_for(&dir);
        assert_eq!(constructions(&dir), 1, "同一根只该构造一次引擎");
        assert!(Arc::ptr_eq(&first, &second), "两次请求必须是同一个引擎");
        // 尾部分隔符不该被当成另一个根。
        let with_sep = PathBuf::from(format!("{}/", dir.display()));
        assert!(
            Arc::ptr_eq(&first, &engine_for(&with_sep)),
            "尾部分隔符不该造成第二个引擎"
        );
        assert_eq!(constructions(&dir), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 两个数据根**不共享**引擎：缓存 / 健康度串味比多建一个引擎严重得多。
    #[test]
    fn different_cache_roots_do_not_share() {
        let a = root("a");
        let b = root("b");
        let engine_a = engine_for(&a);
        let engine_b = engine_for(&b);
        assert!(!Arc::ptr_eq(&engine_a, &engine_b));
        assert_eq!(constructions(&a), 1);
        assert_eq!(constructions(&b), 1);
        assert_eq!(engine_a.cache().root(), a.as_path());
        assert_eq!(engine_b.cache().root(), b.as_path());
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    /// 失效后重建（设置页「重新检测地区」依赖这条语义）。
    #[test]
    fn forget_rebuilds_once() {
        let dir = root("forget");
        let before = engine_for(&dir);
        assert!(forget(&dir), "已存在的引擎应被淘汰");
        assert!(!forget(&dir), "淘汰后不该再有可淘汰的");
        let after = engine_for(&dir);
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(constructions(&dir), 2, "失效一次 = 多构造一次");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 离线回环 HTTP 服务：记下「接受了几条连接 / 处理了几条请求」。
    ///
    /// 只回固定 JSON、只讲 HTTP/1.1 keep-alive，够用来判「两次下载有没有复用同一条连接」。
    fn loopback_server(
        expected_requests: usize,
    ) -> (String, std::sync::mpsc::Receiver<(usize, usize)>) {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
        let addr = listener.local_addr().expect("取端口");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut conns = 0usize;
            let mut requests = 0usize;
            while requests < expected_requests {
                let Ok((mut socket, _)) = listener.accept() else {
                    break;
                };
                conns += 1;
                let _ = socket.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut seen = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    match socket.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => seen.extend_from_slice(&chunk[..read]),
                    }
                    if !seen.windows(4).any(|window| window == b"\r\n\r\n") {
                        continue;
                    }
                    requests += 1;
                    let body = br#"{"ok":true}"#;
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    if socket.write_all(head.as_bytes()).is_err() || socket.write_all(body).is_err()
                    {
                        break;
                    }
                    seen.clear();
                    if requests >= expected_requests {
                        let _ = tx.send((conns, requests));
                        return;
                    }
                }
            }
            let _ = tx.send((conns, requests));
        });
        (format!("http://{addr}/meta.json"), rx)
    }

    /// **复用真的落到连接上**：同一个引擎跨两次 `block_on` 取同一个 host，只应该拨号一次。
    ///
    /// 这条钉住的是「引擎复用」与「运行时常驻」必须成对出现的那半个承诺：只复用引擎、
    /// 每次 `block_on` 新起运行时的话，连接池里的空闲连接属于已销毁的运行时，
    /// 第二次请求会重新 TCP 握手——`conns` 就是 2 而不是 1。
    ///
    /// 离线可跑：服务端是 127.0.0.1 的回环监听器，不碰任何外网站点。
    #[test]
    fn pooled_connection_survives_across_block_on_calls() {
        use super::super::blocking::block_on;
        use super::super::types::{DownloadRequest, ResourceKind};

        let dir = root("pool");
        let (url, counts) = loopback_server(2);
        let engine = engine_for(&dir);
        for seq in 0..2 {
            let request = DownloadRequest::new(
                ResourceKind::Other,
                url.clone(),
                dir.join(format!("meta-{seq}.json")),
            );
            let bytes = block_on(engine.download_bytes(request)).expect("回环请求应成功");
            assert_eq!(bytes, br#"{"ok":true}"#.to_vec());
        }
        let (conns, requests) = counts
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("服务端应报告计数");
        assert_eq!(requests, 2, "两次下载 = 两条请求");
        assert_eq!(conns, 1, "两次下载应复用同一条连接");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **守门①②**：设置说 2 ⇒ 引擎**真实的并发闸门**就是 2（探针读 `Semaphore` 而非配置）；gh-proxy
    /// 三档也真的改变 `plan()` 的候选链；`retry_limit = 0` 落成「只试一次」。
    ///
    /// 变异证明（只改实现侧）：① `build` 换回裸 `EngineConfig::default()` ⇒ 探针 8 红；②
    /// `apply_gh_proxy(region)` 换回 `region` ⇒ `off` 档仍有候选，红；③ 删 `set_tuning` 的
    /// `remove` ⇒ 换参数后交回旧引擎，红。gh-proxy 的端到端（schema 键 → 调优）等 M197 登记该键。
    #[test]
    fn the_setting_and_the_gh_proxy_override_reach_the_engine() {
        use super::super::types::{DownloadRequest, ResourceKind};
        use super::super::{config, RegionProfile};

        let dir = root("tune");
        // 镜像优先的档案：gh-proxy 只在这个模式下才生成候选。
        store_cached(
            &dir.join("region.json"),
            &RegionProfile::mirror_first(now_ms()),
        )
        .expect("地区档案");
        let req = DownloadRequest::new(
            ResourceKind::Other,
            "https://github.com/o/r/releases/download/v1/a.jar",
            dir.join("a.jar"),
        );
        let chain = |dir: &Path| -> Vec<String> {
            engine_for(dir)
                .plan(&req, 0)
                .into_iter()
                .map(|c| c.url)
                .collect()
        };
        let set = |c: Option<i64>, a: Option<i64>, g: &str| {
            set_tuning(&dir, DownloadTuning::from_settings(c, a, Some(g)))
        };

        set(Some(2), Some(1), "off");
        let engine = engine_for(&dir);
        assert_eq!(engine.concurrency_probe(), 2, "闸门必须真是 2 张许可");
        let off = chain(&dir);
        assert!(!off.iter().any(|u| u.contains("gh-proxy")), "{off:?}");

        set(Some(5), None, "https://gh.example/");
        let rebuilt = engine_for(&dir);
        assert!(!Arc::ptr_eq(&engine, &rebuilt), "参数变了必须换引擎");
        assert_eq!(rebuilt.concurrency_probe(), 5, "新闸门吃新值");
        let over = chain(&dir);
        assert!(
            over.iter().any(|u| u.starts_with("https://gh.example/")),
            "{over:?}"
        );

        // P2-1：`retry_limit = 0`（旧配置里真有）不许回落出厂 3。硬上限：塞 9999 也只开 64。
        set(Some(8), Some(0), "");
        let zero = engine_for(&dir);
        assert_eq!(zero.config().max_attempts_per_source, 1);
        assert_eq!(
            zero.attempts_for("official", ResourceKind::Other, now_ms()),
            1
        );
        let wild = DownloadTuning {
            max_concurrency: 9_999,
            ..Default::default()
        };
        set_tuning(&dir, wild);
        assert_eq!(
            engine_for(&dir).concurrency_probe(),
            config::MAX_CONCURRENCY
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
