//! M352 的**合成夹具**：手搓到刚好能触发某条 `ImportIssue` 的最小包（不碰真包、不联网）。
//!
//! 与 `addon/tests/support.rs` 的分工写清楚：那边要复刻「声明与实际一致」的整条链路（为了让
//! 导入真的成功），这里只造**前置状态**——被跳过的条目 / 被遮蔽的覆盖文件 / 坏基线 / 引擎残片
//! ——然后断言那条 `ImportIssue` 的**级别**。所以这些包刻意小：读起来能一眼看出"哪个字段触发
//! 了哪条注记"。
//!
//! mrpack 的 `hashes.sha512` 只填形状（128 位十六进制）：导入只校验 sha1，M110 的读者只校验
//! sha512 的形状（这条分工见 `addon/tests/support.rs` 的模块文档）。

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use unlucky_core::addon::import::{
    FetchError, FetchErrorKind, FetchFuture, FetchRequest, FetchedFile, PackFetcher,
};
use unlucky_core::download::verify::digest_bytes;
use unlucky_core::download::HashAlgo;

/// 远端条目要下下来的字节（成功路径用例用）。
pub(super) const REMOTE_BYTES: &[u8] = b"level guard remote bytes";
/// 成功路径用例里那个远端条目的地址。
pub(super) const REMOTE_URL: &str = "stub://mods/jei.jar";

/// mrpack 规范里 `sha512` 的形状（128 位十六进制，内容不重要）。
fn sha512_shape() -> String {
    "0123456789abcdef".repeat(8)
}

/// 一个字节的 SHA-1（清单声明值与「实际下载到的字节」必须对得上，否则导入判 `Failed`）。
pub(super) fn sha1(bytes: &[u8]) -> String {
    digest_bytes(HashAlgo::Sha1, bytes)
}

/// 一份合成包：临时根（`Drop` 即删）+ 包路径（zip 或目录，两者都是产品路径支持的输入）。
pub(super) struct Pack {
    _dir: TempDir,
    path: PathBuf,
}

impl Pack {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

// ============================ 临时目录 ============================

/// 一次性临时根（`Drop` 即删，panic 也删）。
///
/// 不复用 `crate::addon::temp::TempRoot`：那是 `pub(crate)`，集成测试够不到；这里要的也只是
/// 「建一个、跑完删掉」。
pub(super) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(super) fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ull-m352-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self { path }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 写一个文件（自动建父目录）。
pub(super) fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, bytes).expect("写文件");
}

/// 打一个 zip（`Stored`：内容可逐字节断言，也不依赖压缩库版本）。
fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建 zip 目录");
    }
    let file = std::fs::File::create(path).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).expect("写 zip 条目");
        writer.write_all(bytes).expect("写 zip 内容");
    }
    writer.finish().expect("收尾 zip");
}

// ============================ mrpack 夹具 ============================

/// 一份 mrpack 的 `files[]` 条目（声明与实际一致，`downloads` 指向 stub）。
fn remote_entry(path: &str, client: &str, bytes: &[u8]) -> String {
    format!(
        r#"{{
            "path": "{path}",
            "hashes": {{"sha1": "{}", "sha512": "{}"}},
            "env": {{"client": "{client}", "server": "required"}},
            "downloads": ["{REMOTE_URL}"],
            "fileSize": {}
        }}"#,
        sha1(bytes),
        sha512_shape(),
        bytes.len()
    )
}

fn index(name: &str, dependencies: &str, files: &[String]) -> String {
    format!(
        r#"{{
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "{name}",
            "dependencies": {{{dependencies}}},
            "files": [{}]
        }}"#,
        files.join(",")
    )
}

/// **plan 层四条注记 + 三条文件注记**的夹具（每条注记的触发条件写在 `plan_cases.rs` 的断言旁）。
///
/// 造出来的前置状态：
///
/// | 状态 | 触发的构造点 |
/// |------|-------------|
/// | `server-overrides/` 里有一个文件 | `plan.rs` 的 `NOTE_SERVER_OVERRIDES_SKIPPED` |
/// | 一条 `env.client=unsupported` | `plan.rs` 的 `NOTE_ENV_UNSUPPORTED` |
/// | 一条 `env.client=optional` | `plan.rs` 的 `NOTE_OPTIONAL_FILE` |
/// | 两个认得出的加载器 + 一个认不出的键 | `plan.rs` 的 `NOTE_LOADER_MULTIPLE` / `NOTE_UNKNOWN_DEPENDENCY` |
/// | 同一路径同时出现在 `overrides/` 与 `client-overrides/` | `plan/layer.rs` 的 `NOTE_OVERRIDE_SHADOWED` |
/// | `mods/x.jar.old` 与 `mods/y.jar.disabled.old` | `plan/paths.rs` 的 `NOTE_OLD_SUFFIX` / `.disabled.old` |
pub(super) fn mrpack_with_plan_notes(tag: &str) -> Pack {
    let dir = TempDir::new(tag);
    let pack = dir.join("plan.mrpack");
    let dependencies = r#""minecraft": "1.20.1",
        "fabric-loader": "0.15.11",
        "forge": "47.2.0",
        "parchment": "2024.1.15""#;
    let files = vec![
        remote_entry("mods/optional.jar", "optional", b"optional bytes"),
        remote_entry("mods/serveronly.jar", "unsupported", b"server only bytes"),
    ];
    let index = index("levelguard-plan", dependencies, &files);
    write_zip(
        &pack,
        &[
            ("modrinth.index.json", index.as_bytes()),
            ("overrides/mods/shared.jar", b"overrides copy"),
            ("client-overrides/mods/shared.jar", b"client copy"),
            (
                "server-overrides/config/server.properties",
                b"motd=levelguard\n",
            ),
            ("overrides/mods/legacy.jar.old", b"old bytes"),
            ("overrides/mods/offold.jar.disabled.old", b"off old bytes"),
        ],
    );
    Pack {
        _dir: dir,
        path: pack,
    }
}

/// 一份**只有一个远端条目**的 mrpack：远端成功下载 ⇒ 才能走到「提交前剔残片」与「登记实例」。
pub(super) fn mrpack_with_remote(tag: &str) -> Pack {
    let dir = TempDir::new(tag);
    let pack = dir.join("remote.mrpack");
    let dependencies = r#""minecraft": "1.20.1""#;
    let files = vec![remote_entry("mods/jei.jar", "required", REMOTE_BYTES)];
    let index = index("levelguard-remote", dependencies, &files);
    write_zip(
        &pack,
        &[
            ("modrinth.index.json", index.as_bytes()),
            ("overrides/config/demo.toml", b"k = 1\n"),
        ],
    );
    Pack {
        _dir: dir,
        path: pack,
    }
}

// ============================ 目录 / CF 夹具 ============================

/// MultiMC 实例目录（目录形态的"包"，不需要 zip）。
///
/// `override_memory` 决定 `instance.cfg` 里 `OverrideMemory` 的取值：Prism 的规矩是
/// `MaxMemAlloc` **只在开关为 true 时生效**，两条分支各落一条 `NOTE_INSTANCE_CFG_MEMORY`。
pub(super) fn multimc(tag: &str, override_memory: bool) -> Pack {
    let dir = TempDir::new(tag);
    write_file(
        &dir.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write_file(
        &dir.join("instance.cfg"),
        format!(
            "MaxMemAlloc=4096\nOverrideMemory={}\n",
            if override_memory { "true" } else { "false" }
        )
        .as_bytes(),
    );
    write_file(&dir.join(".minecraft/mods/sodium.jar"), &[0x11; 32]);
    let path = dir.path().to_path_buf();
    Pack { _dir: dir, path }
}

/// 一份 CurseForge 清单：`files[]` 只有 `projectID/fileID`，**补全前没有路径** ⇒ 预检判不了。
pub(super) fn curseforge(tag: &str) -> Pack {
    let dir = TempDir::new(tag);
    let pack = dir.join("cf.zip");
    let manifest = r#"{
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": "levelguard-cf",
        "version": "1.0.0",
        "author": "tower",
        "minecraft": {
            "version": "1.20.1",
            "modLoaders": [{"id": "forge-47.2.0", "primary": true}]
        },
        "files": [{"projectID": 238222, "fileID": 4800000, "required": true}]
    }"#;
    write_zip(
        &pack,
        &[
            ("manifest.json", manifest.as_bytes()),
            ("overrides/x.txt", b"x"),
        ],
    );
    Pack {
        _dir: dir,
        path: pack,
    }
}

/// 基线文件相对实例根的路径（与读写两侧同源，不在测试里手抄）。
pub(super) fn baseline_relative() -> String {
    unlucky_core::addon::import::baseline_relative()
}

/// 写一份**属于别的包**的基线（包身份不符 ⇒ 不许拿它当 base）。
pub(super) fn write_foreign_baseline(target: &Path) {
    let baseline = serde_json::json!({
        "schemaVersion": 1,
        "packName": "另一个整合包",
        "format": "modrinth",
        "files": {},
    });
    write_file(
        &target.join(baseline_relative()),
        baseline.to_string().as_bytes(),
    );
}

// ============================ 假远端 ============================

/// 内存 fetcher：按 URL 查表写盘；`artifacts` 打开时像真引擎那样在目标旁边留下续传残片。
pub(super) struct StubFetcher {
    files: BTreeMap<String, Vec<u8>>,
    artifacts: bool,
}

impl StubFetcher {
    pub(super) fn new() -> Self {
        let mut files = BTreeMap::new();
        files.insert(REMOTE_URL.to_string(), REMOTE_BYTES.to_vec());
        Self {
            files,
            artifacts: false,
        }
    }

    /// 像真引擎那样，每次"下载"后留下 `<名>.ullpart-<pid>-0` 与 `<名>.ullmeta.json`。
    pub(super) fn with_engine_artifacts(mut self) -> Self {
        self.artifacts = true;
        self
    }
}

impl PackFetcher for StubFetcher {
    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, FetchError>> {
        Box::pin(async move {
            let bytes = self.files.get(request.url).cloned().ok_or_else(|| {
                FetchError::new(
                    FetchErrorKind::NotFound,
                    format!("stub 里没有 {}", request.url),
                )
            })?;
            if let Some(parent) = request.dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| FetchError::new(FetchErrorKind::Io, error.to_string()))?;
            }
            std::fs::write(request.dest, &bytes)
                .map_err(|error| FetchError::new(FetchErrorKind::Io, error.to_string()))?;
            if self.artifacts {
                leave_engine_artifacts(request.dest);
            }
            Ok(FetchedFile {
                bytes: bytes.len() as u64,
            })
        })
    }
}

/// 在目标旁边留下与 `core::download::atomic` 同名的续传残片与 meta。
fn leave_engine_artifacts(dest: &Path) {
    let Some(name) = dest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
    else {
        return;
    };
    let part = dest.with_file_name(format!("{name}.ullpart-{}-0", std::process::id()));
    let meta = dest.with_file_name(format!("{name}.ullmeta.json"));
    std::fs::write(part, b"partial bytes").expect("写续传残片");
    std::fs::write(meta, b"{}").expect("写续传 meta");
}
