//! `update` 测试的公共夹具：临时目录、真实摘要、清单 JSON 片段。
//!
//! 清单片段里的摘要**按真实字节算**，不用占位值——「校验和不符」那条用例正是要证明
//! 判据真的在跑，占位值会让它对所有输入都成立。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::download::verify::digest_bytes;
use crate::download::{
    ContentCache, DownloadEngine, EngineConfig, HashAlgo, HealthRegistry, MirrorTable,
    RegionProfile,
};

/// 一次性临时目录（`Drop` 时尽力删除；删不掉也留在系统 temp 下不碍事）。
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ull-update-{}-{tag}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("建临时目录");
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    /// 写一个文件（父目录自动建），返回路径。
    pub(crate) fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("建父目录");
        }
        fs::write(&path, bytes).expect("写文件");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 目录里的文件名（读不到就当空；排序过，断言可写死）。
pub(crate) fn names_in(dir: &Path) -> Vec<String> {
    match fs::read_dir(dir) {
        Ok(entries) => {
            let mut names = entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            names.sort();
            names
        }
        Err(_) => Vec::new(),
    }
}

/// 指定异卷根目录的环境变量（跨卷夹具找不到别的卷时，用它显式指一块）。
pub(crate) const XDEV_ROOT_ENV: &str = "ULL_XDEV_TEST_ROOT";

/// 「我知道本机没有第二块卷」的**显式豁免**：值必须是 `1`（`true`/`yes` 不算，手误不该放行）。
///
/// 没有它、又探不到第二块卷 ⇒ 跨卷守卫**当场红**（见 `install_xdev_cases::xdev_guard`）。
/// 它不会让真跨卷用例变假绿：探到了第二块卷照样真跑（豁免只在探测失败时被咨询）。
pub(crate) const XDEV_WAIVE_ENV: &str = "ULL_XDEV_TEST_WAIVE";

/// 「本机/本作业**必须**有第二块卷」的显式声明：值必须是 `1`（与豁免同给时以它为准，不许自相矛盾）。
pub(crate) const XDEV_REQUIRE_ENV: &str = "ULL_XDEV_TEST_REQUIRE";

/// 一块**与会话临时目录不同卷**的可写目录：跨卷替换用例的现场。
///
/// 「不同卷」不是按平台猜的，是**探测**出来的：往候选目录 rename 一个探针文件，只有真的报
/// `CrossesDevices`（Unix `EXDEV` / Windows `ERROR_NOT_SAME_DEVICE`）才认这块地——否则
/// 用例会在同一卷上跑，等于什么都没验。
///
/// 候选顺序：`ULL_XDEV_TEST_ROOT` → 各盘符（Windows：`<盘>:\Temp`，没有就盘根）/
/// `TMPDIR`、`/dev/shm`、`/run/user/<uid>`、`/tmp`（Unix）。
///
/// 探不到时**怎么处置不是这里决定的**：本函数只回答「有没有」，跳过/失败由
/// `install_xdev_cases::xdev_guard` 裁决（跳过必须是响的）。
pub(crate) struct XdevRoot {
    path: PathBuf,
}

impl XdevRoot {
    /// 探一块异卷目录；探不到返回 `None`（是否允许跳过由 `xdev_guard` 定）。
    pub(crate) fn probe(tag: &str) -> Option<Self> {
        let mut candidates = Vec::new();
        if let Some(root) = std::env::var_os(XDEV_ROOT_ENV) {
            candidates.push(PathBuf::from(root));
        }
        candidates.extend(volume_candidates());
        candidates.into_iter().find_map(|candidate| {
            let path = candidate.join(format!("ull-xdev-{}-{tag}", std::process::id()));
            if !is_other_volume(&candidate) || fs::create_dir_all(&path).is_err() {
                return None;
            }
            Some(Self { path })
        })
    }

    /// 现场根目录。
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// 现场根目录下的子目录（自动建）。
    pub(crate) fn dir(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::create_dir_all(&path).expect("建异卷子目录");
        path
    }
}

impl Drop for XdevRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 可能的异卷候选根（只是候选，能不能用由 [`is_other_volume`] 定）。
fn volume_candidates() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        ('A'..='Z')
            .map(|letter| format!("{letter}:/"))
            .map(PathBuf::from)
            .filter(|root| root.join("Temp").is_dir() || root.is_dir())
            .map(|root| {
                if root.join("Temp").is_dir() {
                    root.join("Temp")
                } else {
                    root
                }
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let mut candidates = Vec::new();
        for key in ["TMPDIR", "TMP", "TEMP"] {
            if let Some(value) = std::env::var_os(key) {
                candidates.push(PathBuf::from(value));
            }
        }
        for root in ["/dev/shm", "/tmp", "/var/tmp"] {
            candidates.push(PathBuf::from(root));
        }
        if let Some(uid) = std::env::var_os("UID") {
            candidates.push(PathBuf::from("/run/user").join(uid));
        }
        candidates
    }
}

/// 这块目录与 [`std::env::temp_dir`] 是否**真的**不同卷（拿一次真实的 rename 试出来）。
fn is_other_volume(dir: &Path) -> bool {
    if dir.as_os_str().is_empty() || fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = std::env::temp_dir().join(format!("ull-xdev-probe-{}", std::process::id()));
    if fs::write(&probe, b"probe").is_err() {
        return false;
    }
    let landed = dir.join(format!("ull-xdev-probe-{}", std::process::id()));
    let cross = matches!(fs::rename(&probe, &landed), Err(ref error) if error.kind() == std::io::ErrorKind::CrossesDevices);
    let _ = fs::remove_file(&probe);
    let _ = fs::remove_file(&landed);
    cross
}

/// 小写十六进制 SHA-256。
pub(crate) fn sha256(bytes: &[u8]) -> String {
    digest_bytes(HashAlgo::Sha256, bytes)
}

/// 清单里的一个产物对象（`sha256` 与 `size` 都按 `payload` 真实值算）。
pub(crate) fn artifact(target: &str, format: &str, url: &str, payload: &[u8]) -> String {
    format!(
        r#"{{"target":"{target}","format":"{format}","url":"{url}","size":{},"sha256":"{}"}}"#,
        payload.len(),
        sha256(payload)
    )
}

/// 清单里的一个发布对象；`extra` 追加其它字段（如 `,"force":true`），注意前导逗号。
pub(crate) fn release(version: &str, extra: &str, artifacts: &[String]) -> String {
    format!(
        r#"{{"version":"{version}"{extra},"artifacts":[{}]}}"#,
        artifacts.join(",")
    )
}

/// 一份渠道清单。
pub(crate) fn manifest(channel: &str, releases: &[String]) -> String {
    format!(
        r#"{{"channel":"{channel}","releases":[{}]}}"#,
        releases.join(",")
    )
}

/// 造一台测试用下载引擎：镜像表用内置清单，健康度只在内存里，缓存落临时目录。
///
/// `region` 决定「官方优先」还是「镜像优先」——gh-proxy 前缀只在该是后一种时生效。
pub(crate) fn engine_for(tmp: &TempDir, region: RegionProfile) -> std::sync::Arc<DownloadEngine> {
    std::sync::Arc::new(DownloadEngine::new(
        EngineConfig {
            // 失败用例不必等三次退避。
            max_attempts_per_source: 1,
            ..EngineConfig::default()
        },
        std::sync::Arc::new(MirrorTable::builtin()),
        std::sync::Arc::new(region),
        std::sync::Arc::new(HealthRegistry::in_memory()),
        std::sync::Arc::new(ContentCache::open(tmp.join("cache"))),
    ))
}

/// 假服务的一条路由应答。
#[derive(Debug, Clone)]
pub(crate) struct FakeResponse {
    pub(crate) status: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
}

impl FakeResponse {
    /// 200 + JSON 体。
    pub(crate) fn json(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "application/json",
            body: body.as_bytes().to_vec(),
        }
    }

    /// 200 + 二进制体（模拟 release 里的产物）。
    pub(crate) fn bin(body: &[u8]) -> Self {
        Self {
            status: 200,
            content_type: "application/octet-stream",
            body: body.to_vec(),
        }
    }

    /// 固定状态码 + 空体（404 / 500 用）。
    pub(crate) fn status(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain",
            body: Vec::new(),
        }
    }
}

/// 进程内极简 HTTP/1.1 假服务：按**完整请求路径**查表应答，未命中的路径一律 404。
///
/// 用它而不是 httpmock：项目里下载测试就是这么干的（零额外依赖），而且能把
/// 「请求真的发到了哪个 URL」记下来给断言用。
pub(crate) struct FakeHttp {
    addr: std::net::SocketAddr,
    hits: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FakeHttp {
    /// 起服务（`routes` 是「路径 → 应答」表）。
    pub(crate) fn start(routes: Vec<(String, FakeResponse)>) -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("绑定本地端口");
        let addr = listener.local_addr().expect("取本地地址");
        let hits = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let worker_hits = std::sync::Arc::clone(&hits);
        let worker_stop = std::sync::Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if worker_stop.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { break };
                let Some(path) = read_request_path(&mut stream) else {
                    continue;
                };
                worker_hits
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(path.clone());
                let response = routes.iter().find(|(route, _)| route == &path).map_or_else(
                    || FakeResponse::status(404),
                    |(_, response)| response.clone(),
                );
                let _ = write_response(&mut stream, &response);
            }
        });

        Self {
            addr,
            hits,
            stop,
            handle: Some(handle),
        }
    }

    /// 站点根地址（可直接喂给 `GitHubReleaseSource::with_base`）。
    pub(crate) fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// 已经收到的请求路径（按到达顺序）。
    pub(crate) fn hits(&self) -> Vec<String> {
        self.hits
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Drop for FakeHttp {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // 自己连一下唤醒阻塞中的 accept，否则 join 会一直等。
        let _ = std::net::TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// 读一个请求的路径（只读头部，够用）。
fn read_request_path(stream: &mut std::net::TcpStream) -> Option<String> {
    use std::io::Read;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buffer).to_string();
    head.lines()
        .next()?
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

fn write_response(
    stream: &mut std::net::TcpStream,
    response: &FakeResponse,
) -> std::io::Result<()> {
    use std::io::Write;
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    }
}
