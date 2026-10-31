//! 集成测试共用夹具：临时数据根、命令执行器、最小版本 JSON / 会话索引。
//!
//! 每个测试目标都会整份编译本文件但只用其中一部分，因此这里放开 dead_code。
#![allow(dead_code)]

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
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-cli-test-{tag}-{}-{seq}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
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
        "log_path": format!("{sid}.jsonl"),
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
