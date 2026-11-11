//! 更新通道端到端验收的夹具（M169）：临时现场、真形状清单、测试通道的环境变量守卫。
//!
//! 两个测试目标共用（`update_e2e_test` 正路 / 幂等，`update_e2e_tamper_test` 篡改）；每个目标只
//! 用到其中一部分，所以与 `support/mod.rs` 一样放开 dead_code。
//!
//! 整个夹具只在 `update-test-source` feature 下真的被用到：那两个测试目标自己 `#![cfg]` 掉，
//! 这里只是跟着编译（不跑）。判据（feature + 两档环境变量）在 `core::update::job`。
//!
//! # 为什么这套「真形状」只在 Windows 上成立（r1 复审 P2-4）
//!
//! 产物名与形态按 **Windows** 的真实发布走：`unlucky-launcher_<ver>_windows_x64.exe` +
//! `portable-exe`（口径见 `packaging/make_artifacts.py` 的头注释）。Linux/macOS 的真实产物是
//! `.tar.gz` + `portable-zip`，会被 `ensure_self_replaceable` **直接拒**（不是可原地自替换的形态）。
//!
//! * 在 Windows 上跑 = 验真实形状；
//! * 在别的平台上跑绿只能证明「一个世上不存在的 linux/macOS portable-exe 能装上」——**假绿**。
//!
//! 所以 CI 里它是**一个独立的 Windows job**（`.github/workflows/ci.yml` 的 `update-e2e`，
//! `runs-on: windows-latest`），本文件也**不**为非 Windows 平台改断言：要真支持，得让产物名与
//! `format` 一起按平台分叉，那是另一个 mission 的事（真实 Linux 自更新根本不支持，D50 只发裸 exe）。
//!
//! ⚠ **验收必须给 `--dest`**：用例在**进程内**跑 `unlucky_cli::run`，`Job::for_process` 的
//! 「本进程的 exe」就是测试二进制——`apply` 不带 `--dest` 会真的把它 `rename` 成新字节
//! （Windows 允许改名正在运行的映像，见 `core::update::install` 的文档），整轮测试当场崩。
//! `Fixture::run` 里有结构性断言兜这一条（r1 复审 P2-5），不是只靠这段注释。
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use super::update_e2e_sign::write_signature;
use unlucky_core::update::{HashAlgo, OsFamily, Target, UPDATE_DIR_ENV, UPDATE_TEST_SOURCE_ENV};

/// 假装的「已安装版本」字节：与产物**必须不同**，否则「目标没被动过」这类断言会假绿。
pub const INSTALLED: &[u8] = b"installed 0.2.3 build (pretend)";

/// 产物体量上限（1 MiB）。
///
/// 现场构建出来的 `ull.exe` 是 **~400 MB**（debug + 全套 reqwest/rmcp/axum）；拿整份当过测材，
/// 光 sha256 + 拷贝就要几十秒，验收会变成「谁都不想跑」。所以截取**前 1 MiB**：PE 头与头几节都在，
/// 「这是真 exe 的字节」这一点保留，代价是「装上去能不能启动」不再由它证明——那一跳本来就要人工
/// （见报告「不验的部分」）。清单里的 `size` / `sha256` 按这份截短后的真实字节算，仍然自洽。
const PAYLOAD_LIMIT: usize = 1024 * 1024;

/// 清单里的更新说明（断言它会原样出现在 `check --json` 里）。
const NOTES: &str = "修掉启动崩溃；实例页加载更快";

/// SHA-256 按真实字节算（与双检同一条实现：`core::download::verify::digest_bytes`）。
pub fn sha256(bytes: &[u8]) -> String {
    unlucky_core::download::verify::digest_bytes(HashAlgo::Sha256, bytes)
}

/// 清单版本：**比编译期版本严格大一档次版本**。
///
/// 不能写死 `0.2.4`：workspace 版本一升，用例就变成「已是最新」的假红。次版本 +1 恒大于当前
/// 版本，且仍在同一 major 内（不会意外触发 `min_supported` 那条判据）。
pub fn next_version(current: &str) -> String {
    let mut parts = current.split('.');
    let major = parts.next().unwrap_or("0");
    let minor: u64 = parts
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or_else(|_| panic!("编译期版本号应是三段式：{current}"));
    assert!(parts.next().is_some(), "编译期版本号应是三段式：{current}");
    format!("{major}.{}.0", minor + 1)
}

/// 真实产物命名（D49）：`unlucky-launcher_<ver>_windows_x64.exe`。
///
/// 拿 `Target::as_str()`（`win-x64`）直接拼会得到一个世上不存在的文件名，清单那一栏也就
/// 与真实发布对不上——「真形状」的意义正在这里。
fn asset_name(version: &str) -> String {
    let target = Target::current();
    let os = match target.os {
        OsFamily::Windows => "windows",
        OsFamily::MacOs => "macos",
        OsFamily::Linux => "linux",
    };
    format!(
        "unlucky-launcher_{version}_{os}_{}.exe",
        target.arch.as_str()
    )
}

/// 一份与 `packaging/tests/manifest.golden.json` **同字段**的清单。
///
/// 字段一个不多一个不少：core 的 `deny_unknown_fields` 会把多余字段当清单损坏拒掉，所以这条
/// 同时也是「清单形状与真实生成器（`packaging/make_manifest.py`）对齐」的断言。
fn manifest(version: &str, asset: &str, size: u64, sha256: &str) -> String {
    format!(
        r#"{{
  "channel": "stable",
  "releases": [
    {{
      "version": "{version}",
      "channel": "stable",
      "released_at": "2026-01-02T03:04:05Z",
      "notes": "{NOTES}",
      "force": false,
      "min_supported": "0.1.0",
      "artifacts": [
        {{
          "target": "{}",
          "format": "portable-exe",
          "url": "{asset}",
          "size": {size},
          "sha256": "{sha256}"
        }}
      ]
    }}
  ]
}}"#,
        Target::current().as_str()
    )
}

/// 进程环境是共享的（同一测试进程里各用例并行）：动环境变量的用例都拿这把锁串起来。
pub fn channel_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 两个测试通道变量的现场：构造时记住原值，`Drop` 还原（用例 panic 也还原）。
pub struct Channel {
    dir: Option<std::ffi::OsString>,
    ack: Option<std::ffi::OsString>,
}

impl Channel {
    pub fn capture() -> Self {
        Self {
            dir: std::env::var_os(UPDATE_DIR_ENV),
            ack: std::env::var_os(UPDATE_TEST_SOURCE_ENV),
        }
    }

    /// 测试通道开启：两档环境变量全给（二进制还得带 `update-test-source` feature；只给路径不会被
    /// 认，判据见 `core::update::job`）。
    pub fn enable(root: &Path) {
        std::env::set_var(UPDATE_DIR_ENV, root);
        std::env::set_var(UPDATE_TEST_SOURCE_ENV, "1");
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        for (key, value) in [
            (UPDATE_DIR_ENV, self.dir.clone()),
            (UPDATE_TEST_SOURCE_ENV, self.ack.clone()),
        ] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// `--json` 信封 → 命令载荷。
///
/// 成功信封是 `{"ok":true,"data":{…},"warnings":[…]}`，失败信封是 `{"ok":false,"error":{…}}`
/// （没有 `data`）。所以「取载荷」= 有 `data` 取 `data`，否则原样返回——失败用例照样能看
/// `error.code` / `error.detail.kind`。
pub fn payload(json: &serde_json::Value) -> &serde_json::Value {
    json.get("data").unwrap_or(json)
}

/// 目录里的文件名（读不到就当空）。
fn list_dir(dir: &Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 一次验收的全部现场：
///
/// ```text
/// <temp>/<tag>-data/     --data-dir（自定义模式 ⇒ <data>/.ULL 与 <data>/.minecraft）
/// <temp>/<tag>-dist/     ULL_UPDATE_DIR（stable.json + 产物）
/// <temp>/<tag>-install/  --dest 所在目录（**临时副本**，不是本进程的 exe）
/// ```
///
/// 三个 `TempDir` 都要活到用例结束（`Fixture` 持有它们）。
pub struct Fixture {
    pub data: super::TempDir,
    pub dist: super::TempDir,
    install: super::TempDir,
    /// 产物字节 = 现场构建出来的 `ull.exe` 的前 1 MiB（真 PE 头，理由见 [`PAYLOAD_LIMIT`]）。
    pub payload: Vec<u8>,
    /// 清单里的版本号（= 编译期版本次版本 +1）。
    pub version: String,
    /// 产物文件名（清单 `url` 与盘上文件同名）。
    pub asset: String,
    /// 替换目标（安装目录里的副本）。
    pub dest: PathBuf,
}

/// 现场构建出来的 `ull.exe` 的前 [`PAYLOAD_LIMIT`] 字节（读一次，整份不超过 1 MiB）。
///
/// 为什么不用占位字符串：这是**发布矩阵里那个单文件 exe** 的真字节，PE 头、节表、体积都对得上；
/// 为什么截短：整份 ~400 MB，测材太重（理由见 [`PAYLOAD_LIMIT`]）。
fn payload_bytes() -> Vec<u8> {
    let path = env!("CARGO_BIN_EXE_ull");
    let mut payload = std::fs::read(path).expect("读构建出来的 ull.exe");
    payload.truncate(PAYLOAD_LIMIT);
    assert!(
        payload.len() >= 64 * 1024,
        "产物要像真的（至少 64 KiB），实际 {} 字节",
        payload.len()
    );
    if cfg!(windows) {
        assert!(
            payload.starts_with(b"MZ"),
            "Windows 产物得是 PE 映像（前两个字节 MZ），否则「真形状」这句话就没意义"
        );
    }
    payload
}

impl Fixture {
    pub fn new(tag: &str) -> Self {
        Self::with_install(tag, super::TempDir::new(&format!("{tag}-install")))
    }

    /// 安装目录在**另一块卷**上的现场（数据根与暂存仍在会话 temp）：M171 的真机跨卷形态。
    ///
    /// 找不到第二块可写卷时返回 `None`（调用方跳过并说明原因）——这条用例在只有一块卷的
    /// 机器/CI 上**不生效**，跨卷逻辑本身由 `crates/core` 的注入用例守着。
    pub fn cross_volume(tag: &str) -> Option<Self> {
        let install = super::TempDir::on_other_volume(&format!("{tag}-install"))?;
        Some(Self::with_install(tag, install))
    }

    fn with_install(tag: &str, install: super::TempDir) -> Self {
        let payload = payload_bytes();
        let version = next_version(env!("CARGO_PKG_VERSION"));
        let dest = install.path().join("unlucky-launcher.exe");
        std::fs::write(&dest, INSTALLED).expect("写旧版本");
        Self {
            data: super::TempDir::new(&format!("{tag}-data")),
            dist: super::TempDir::new(&format!("{tag}-dist")),
            install,
            asset: asset_name(&version),
            payload,
            version,
            dest,
        }
    }

    /// 发一版：清单 + 产物都按 `payload` 的真实长度与摘要。
    pub fn publish_good(&self) {
        self.publish(&sha256(&self.payload), self.payload.len() as u64);
    }

    /// 发一版，摘要/长度由调用方给（篡改用例用）。
    pub fn publish(&self, sha256: &str, size: u64) {
        std::fs::write(self.dist.path().join(&self.asset), &self.payload).expect("写产物");
        let path = self.dist.path().join("stable.json");
        std::fs::write(&path, manifest(&self.version, &self.asset, size, sha256)).expect("写清单");
        // M190：清单与签名成对落盘。篡改用例给的是「错的 sha/size」，签的是**改过之后**那份清单
        // —— 那正是「清单本身可信、包体不对」的现场（摘要判据要独立于签名判据被证到）。
        write_signature(&path);
    }

    /// 跑一条 `update` 子命令：`--data-dir` 指向临时数据根（与 GUI 一样由数据根派生暂存目录）。
    ///
    /// **结构性保险（r1 复审 P2-5 / r2 复审 P2-3）**：`apply` 必须显式带 `--dest`，且**这一次调用实际
    /// 传下去的那个 `--dest` 值**不许是本进程的 exe。不带 `--dest` 就是 `Job::for_process` 的「本进程
    /// exe」——在这套用例里那是**测试二进制**，而 Windows 允许改名正在运行的映像，于是整轮测试会把它
    /// 换成一个假启动器（症状极难查）。
    ///
    /// 比的是 **args 里那个 `--dest` 的值**（含 `--dest=<path>` 形式），不是夹具字段：r2 复审指出
    /// 「比 `self.dest`」等于只守住了夹具自己那一份，手写
    /// `fx.run(&["apply", "--dest", "<测试二进制>", "--json"])` 照样能过。夹具字段那一份作为额外一道
    /// 保留（`Fixture::apply()` 走的就是它）。
    ///
    /// 抓得住：漏 `--dest`、把 `--dest` 指到本进程 exe、夹具 dest 被改成 exe——都**当场 panic**。
    /// 抓不住：绕开 `Fixture` 直接调 `unlucky_cli::run`（那等于另写一套夹具，`--data-dir` 也没人拼了）。
    pub fn run(&self, args: &[&str]) -> (i32, String, String) {
        let requested = args
            .iter()
            .position(|arg| *arg == "--dest")
            .and_then(|index| args.get(index + 1).copied())
            .or_else(|| args.iter().find_map(|arg| arg.strip_prefix("--dest=")));
        if args.first() == Some(&"apply") {
            assert!(
                requested.is_some(),
                "apply 必须带 --dest：不带会替换本进程 exe（这套用例里就是测试二进制）。\
                 唯一的 apply 入口是 `Fixture::apply()`"
            );
        }
        if let Ok(exe) = std::env::current_exe().and_then(std::fs::canonicalize) {
            let asked = requested.and_then(|path| std::fs::canonicalize(path).ok());
            let fixture = std::fs::canonicalize(&self.dest).ok();
            for dest in [asked, fixture].into_iter().flatten() {
                assert_ne!(
                    dest, exe,
                    "替换目标不许是本进程的 exe（`--dest` 点的就是它）"
                );
            }
        }
        let mut full = vec!["update"];
        full.extend_from_slice(args);
        let data_dir = self.data.arg();
        full.push("--data-dir");
        full.push(data_dir.as_str());
        super::run_cli(&full)
    }

    /// 跑 `apply --dest <副本> --json`（**永远**带 `--dest`，见文件头）。
    pub fn apply(&self) -> (i32, serde_json::Value) {
        let dest = self.dest.display().to_string();
        let (code, out, _) = self.run(&["apply", "--dest", &dest, "--json"]);
        (code, super::parse_json(&out))
    }

    /// 暂存目录（`<数据根>/.ULL/cache/update/staging`，与 `core::update::staging_dir` 同形）。
    pub fn staging(&self) -> PathBuf {
        self.data
            .path()
            .join(".ULL")
            .join("cache")
            .join("update")
            .join("staging")
    }

    /// 暂存目录里还留着什么（`StagedPackage` 落地/被搬走之后应为空）。
    pub fn staging_leftovers(&self) -> Vec<String> {
        list_dir(&self.staging())
    }

    /// 安装目录里的备份（`.unlucky-launcher.exe.ullold-*`）。
    pub fn backups(&self) -> Vec<String> {
        list_dir(self.install.path())
            .into_iter()
            .filter(|name| name.contains(".ullold-"))
            .collect()
    }

    /// 目标文件当前字节。
    pub fn installed_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.dest).expect("读目标")
    }
}
