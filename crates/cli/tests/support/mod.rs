//! 集成测试共用夹具：临时数据根、命令执行器、最小版本 JSON / 会话索引。
//!
//! 每个测试目标都会整份编译本文件但只用其中一部分，因此这里放开 dead_code。
#![allow(dead_code)]

pub mod update_e2e;
pub mod update_e2e_sign;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 一次性临时目录（`Drop` 时清理）。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// 建一个唯一目录。
    pub fn new(tag: &str) -> Self {
        Self::in_root(&std::env::temp_dir(), tag).expect("创建临时目录")
    }

    /// 建在**另一块卷**上（跨卷验收用）；本机只有一块可写卷时返回 `None`。
    pub fn on_other_volume(tag: &str) -> Option<Self> {
        other_volume_root().and_then(|root| Self::in_root(&root, tag))
    }

    fn in_root(root: &Path, tag: &str) -> Option<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(
            "ull-cli-test-{tag}-{}-{seq}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).ok()?;
        Some(Self { path })
    }

    /// 目录路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 目录的字符串形式（喂给 `--data-dir`）。
    pub fn arg(&self) -> String {
        self.path.display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 指定异卷根目录的环境变量（本机找不到第二块卷时，用它显式指一块）。
pub const XDEV_ROOT_ENV: &str = "ULL_XDEV_TEST_ROOT";

/// 「我知道本机没有第二块卷」的显式豁免（值必须是 `1`）。
///
/// 与 `crates/core/src/update/tests/support.rs` 的同名常量是**同一条政策**（源头在
/// `crates/core/src/update/tests/install_xdev_cases.rs::xdev_guard`，改一处要改两处）：
/// 探不到第二块卷又不豁免 ⇒ 这条 e2e 必须红，不许静静 `return`（M171 r2 P2-1）。
pub const XDEV_WAIVE_ENV: &str = "ULL_XDEV_TEST_WAIVE";

/// 「本机/本作业必须有第二块卷」的显式声明（值必须是 `1`），优先于豁免。
pub const XDEV_REQUIRE_ENV: &str = "ULL_XDEV_TEST_REQUIRE";

/// 跨卷 e2e 探不到第二块卷时的裁决：**显式豁免**才允许跳过（打印一行记录），否则 panic。
///
/// `what` 只用来在输出里点名是哪条用例，裁决规则与 core 侧完全一致（见 [`XDEV_WAIVE_ENV`]）。
pub fn cross_volume_waived(what: &str) {
    let required = std::env::var(XDEV_REQUIRE_ENV).ok();
    let waived = std::env::var(XDEV_WAIVE_ENV).ok();
    let required = required.as_deref().map(str::trim) == Some("1");
    let waived = waived.as_deref().map(str::trim) == Some("1");
    assert!(
        waived && !required,
        "[xdev-guard] {what}：本机没有第二块可写卷，也没有显式豁免——跨卷 e2e 空跑了，\
         所以判红。三种合规做法任选其一：\
         ① ULL_XDEV_TEST_ROOT=<另一块卷上的可写目录>；② ULL_XDEV_TEST_WAIVE=1（值必须是 1）；\
         ③ 认为本机本该有第二块卷就用 ULL_XDEV_TEST_REQUIRE=1 声明。\
         政策与理由见 crates/core/src/update/tests/install_xdev_cases.rs 的模块文档。"
    );
    eprintln!(
        "[xdev-guard] {what}：本机没有第二块可写卷，已显式豁免（{XDEV_WAIVE_ENV}=1），\
         这条 e2e 跳过。"
    );
}

/// 一块与会话临时目录**真的**不同卷的可写根目录（探出来的，不是按平台猜的）。
///
/// 候选：`ULL_XDEV_TEST_ROOT` → Windows 各盘符的 `Temp`（没有就盘根）/ Unix 的 `TMPDIR`、
/// `/dev/shm`、`/tmp`、`/run/user/<uid>`。每个候选都要过「从 temp 目录 rename 进它必须报
/// `CrossesDevices`」这一关，所以返回值**一定**是异卷——同一块卷上的候选会被跳过，
/// 否则跨卷用例会在同卷上跑，等于什么都没验。
pub fn other_volume_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(XDEV_ROOT_ENV) {
        candidates.push(PathBuf::from(root));
    }
    candidates.extend(volume_candidates());
    candidates
        .into_iter()
        .find(|candidate| is_other_volume(candidate))
}

fn volume_candidates() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        ('A'..='Z')
            .map(|letter| PathBuf::from(format!("{letter}:/")))
            .filter(|root| root.is_dir())
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
        let mut candidates: Vec<PathBuf> = ["TMPDIR", "TMP", "TEMP"]
            .iter()
            .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
            .collect();
        for root in ["/dev/shm", "/tmp", "/var/tmp"] {
            candidates.push(PathBuf::from(root));
        }
        if let Some(uid) = std::env::var_os("UID") {
            candidates.push(PathBuf::from("/run/user").join(uid));
        }
        candidates
    }
}

fn is_other_volume(dir: &Path) -> bool {
    if dir.as_os_str().is_empty() || std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = std::env::temp_dir().join(format!("ull-cli-probe-{}", std::process::id()));
    if std::fs::write(&probe, b"probe").is_err() {
        return false;
    }
    let landed = dir.join(format!("ull-cli-probe-{}", std::process::id()));
    let cross = matches!(
        std::fs::rename(&probe, &landed),
        Err(ref error) if error.kind() == std::io::ErrorKind::CrossesDevices
    );
    let _ = std::fs::remove_file(&probe);
    let _ = std::fs::remove_file(&landed);
    cross
}

/// 跑一次 CLI，返回（退出码, stdout, stderr）。
pub fn run_cli(args: &[&str]) -> (i32, String, String) {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = unlucky_cli::run(&owned, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

/// 解析 `--json` 输出（断言它是合法 JSON 并返回）。
pub fn parse_json(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).unwrap_or_else(|error| panic!("输出不是 JSON：{error}\n{stdout}"))
}

/// 写一份最小可组装的新版版本 JSON + 空 client jar。
pub fn write_version(data_root: &Path, id: &str) {
    let dir = data_root.join(".minecraft").join("versions").join(id);
    std::fs::create_dir_all(&dir).expect("创建版本目录");
    let version = serde_json::json!({
        "id": id,
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "5",
        "assetIndex": { "id": "5" },
        "arguments": {
            "game": ["--username", "${auth_player_name}", "--version", "${version_name}"],
            "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
        },
        "libraries": [],
        "downloads": { "client": { "size": 4, "url": "http://example.invalid/client.jar" } }
    });
    std::fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_vec_pretty(&version).expect("序列化版本 JSON"),
    )
    .expect("写版本 JSON");
    std::fs::write(dir.join(format!("{id}.jar")), b"junk").expect("写 client jar");
}

/// 写 `config/profiles.json`（实例名 → 版本 id）。
pub fn write_profiles(data_root: &Path, profiles: &[(&str, &str)]) {
    let dir = data_root.join(".ULL").join("config");
    std::fs::create_dir_all(&dir).expect("创建配置目录");
    let entries: Vec<serde_json::Value> = profiles
        .iter()
        .enumerate()
        .map(|(index, (name, version))| {
            serde_json::json!({
                "id": format!("11111111-2222-4333-8444-{:012}", index + 1),
                "name": name,
                "version_id": version,
            })
        })
        .collect();
    let document = serde_json::json!({ "schema_version": 1, "profiles": entries });
    std::fs::write(
        dir.join("profiles.json"),
        serde_json::to_vec_pretty(&document).expect("序列化 profiles.json"),
    )
    .expect("写 profiles.json");
}

/// 写一份「退出码 1 + OOM」的会话索引与事件流，返回会话 id。
///
/// **形状必须与生产写入一致**（M211/D2）：`log_path` 是**相对会话根**的 `game/<sid>.jsonl`，
/// 事件流真落点是 `<会话根>/game/<sid>.jsonl`。这里用的是旧家（`data/agent/sessions`）作会话根
/// ——那是 M188 之前的生产落点，也是用户盘上存量会话的真实形状，读取侧必须还能读回它们。
/// 夹具此前写的是不带 `game/` 的 `"{sid}.jsonl"`，于是「读取侧多 join 一层 `game/`」这个 bug
/// 在测试里永远看不见：夹具自己说了另一种形状。
pub fn write_session(data_root: &Path) -> String {
    let sid = "20261030T101500-abcdef";
    let dir = data_root
        .join(".ULL")
        .join("data")
        .join("agent")
        .join("sessions")
        .join("game");
    std::fs::create_dir_all(&dir).expect("创建会话目录");
    let events = [
        serde_json::json!({"v":1,"seq":0,"ts":"2026-10-30T10:15:00Z","t_ms":0,"kind":"session_start","session_id":sid,"payload":{"instance_name":"生存服","game_version":"1.20.1"}}),
        serde_json::json!({"v":1,"seq":1,"ts":"2026-10-30T10:15:02Z","t_ms":1500,"kind":"stdout","session_id":sid,"payload":{"line":"[main/INFO]: Loading Minecraft 1.20.1","level":"info"}}),
        serde_json::json!({"v":1,"seq":2,"ts":"2026-10-30T10:15:05Z","t_ms":4200,"kind":"stderr","session_id":sid,"payload":{"line":"java.lang.OutOfMemoryError: Java heap space","level":"error"}}),
    ];
    let mut text = String::new();
    for event in events {
        text.push_str(&serde_json::to_string(&event).expect("序列化事件"));
        text.push('\n');
    }
    std::fs::write(dir.join(format!("{sid}.jsonl")), text).expect("写事件流");
    let entry = serde_json::json!({
        "v": 1,
        "session_id": sid,
        "instance_id": "11111111-2222-4333-8444-555555555555",
        "instance_name": "生存服",
        "game_version": "1.20.1",
        "started_at": "2026-10-30T10:15:00Z",
        "ended_at": "2026-10-30T10:15:06Z",
        "duration_ms": 5000,
        "exit_code": 1,
        "exit_type": "application_error",
        "silent_failure": false,
        "window_detected": false,
        "event_count": 3,
        "log_path": unlucky_core::launch::session_index::log_path_for(sid),
        "crash_found": false,
        "diagnosis_top": null
    });
    std::fs::write(
        dir.join("index.jsonl"),
        format!("{}\n", serde_json::to_string(&entry).expect("序列化索引")),
    )
    .expect("写会话索引");
    sid.to_string()
}
