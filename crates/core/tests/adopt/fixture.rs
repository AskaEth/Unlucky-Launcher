//! 真实磁盘夹具：一份「别的启动器铺好的」`.minecraft` + 全量快照工具。
//!
//! # 为什么必须是真盘
//!
//! 本 mission 的核心判据是**「登记前后外部文件一个字节都不变」**。内存里的假目录证明不了这件事
//! ——mtime、目录项、符号链接都只有真文件系统才存在。所以每个用例都在 `std::env::temp_dir()`
//! 下建一棵真目录树，`Drop` 时整棵删掉。
//!
//! # 夹具里放了什么（逐项都有用意）
//!
//! | 路径 | 用意 |
//! | --- | --- |
//! | `versions/1.20.1/` | 正常版本（清单 + jar） |
//! | `versions/1.20.1-fabric-0.15.0/` | 正常版本，`inheritsFrom` 指向 1.20.1（父链闭合） |
//! | `versions/1.20.1-broken/` | 清单不是合法 JSON |
//! | `versions/1.20.1-orphan/` | `inheritsFrom` 指向不存在的父 |
//! | `versions/cycle-a/` `versions/cycle-b/` | 互相继承（成环） |
//! | `versions/1.20.1-nomanifest/` | 只有目录、没有同名清单 |
//! | `libraries/` `assets/` | 恒共享层（登记不该碰） |
//! | `mods/` `saves/` `config/` | 玩家数据（登记不该碰） |
//! | `launcher_profiles.json` | 别的启动器的元数据（连读都不该读，更不该写） |
//!
//! 快照工具在 `snapshot.rs`（**全节点**：目录/文件/链接各记形状与 mtime，读盘失败即失败），
//! 本模块只做 `pub use`，测试统一从 `crate::fixture::{snapshot, assert_unchanged, …}` 取。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use unlucky_core::game::adopt::{AdoptRequest, AdoptTarget};
use unlucky_core::game::GameLayout;

pub use super::snapshot::{assert_unchanged, snapshot, snapshot_opt, Node};
use unlucky_core::platform::layout::LauncherLayout;

/// 唯一临时目录守卫（`drop` 时递归删除）。
pub struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    /// 建一个唯一临时根。
    pub fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-adopt-{tag}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("临时根应可创建");
        Self { path }
    }

    /// 临时根路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `.minecraft`（游戏根）。
    pub fn game_root(&self) -> PathBuf {
        self.path.join(".minecraft")
    }

    /// 游戏布局。
    pub fn game(&self) -> GameLayout {
        GameLayout::new(self.game_root())
    }

    /// 启动器数据布局（`<root>/.ULL`）。
    pub fn launcher(&self) -> LauncherLayout {
        LauncherLayout::new(self.path.join(".ULL"))
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 造一份「别的启动器已经铺好的」`.minecraft`（见模块文档的表）。
pub fn foreign_minecraft(root: &Path) {
    let versions = root.join("versions");
    // 正常版本：清单 + jar。
    version_with(&versions, "1.20.1", MANIFEST_VANILLA, true);
    // 正常版本：带父链（fabric 形态：只有清单、没有自己的 jar）。
    version_with(&versions, "1.20.1-fabric-0.15.0", MANIFEST_FABRIC, false);
    // 清单坏掉。
    let broken = versions.join("1.20.1-broken");
    std::fs::create_dir_all(&broken).expect("建坏清单目录");
    std::fs::write(broken.join("1.20.1-broken.json"), b"{ this is not json").expect("写坏清单");
    // 缺父版本。
    version_with(&versions, "1.20.1-orphan", MANIFEST_ORPHAN, true);
    // 成环。
    version_with(&versions, "cycle-a", MANIFEST_CYCLE_A, true);
    version_with(&versions, "cycle-b", MANIFEST_CYCLE_B, true);
    // 只有目录、没有清单。
    std::fs::create_dir_all(versions.join("1.20.1-nomanifest")).expect("建无清单目录");

    // 恒共享层：库与资源（登记一个字节都不该动）。
    write_file(
        &root.join("libraries/com/mojang/brigadier/1.0.18/brigadier-1.0.18.jar"),
        b"library-bytes",
    );
    write_file(&root.join("assets/indexes/5.json"), b"{\"objects\":{}}");
    write_file(
        &root.join("assets/objects/ab/abcdef0123456789abcdef0123456789abcdef01"),
        b"asset-bytes",
    );
    // 玩家数据。
    write_file(&root.join("mods/jei.jar"), b"mod-bytes");
    write_file(&root.join("saves/World/level.dat"), b"save-bytes");
    write_file(&root.join("config/example.toml"), b"config = 1\n");
    // 别的启动器的元数据（登记不该读、更不该写）。
    write_file(
        &root.join("launcher_profiles.json"),
        br#"{"profiles":{"1.20.1":{"name":"1.20.1","lastVersionId":"1.20.1"}}}"#,
    );
}

fn version_with(versions: &Path, id: &str, manifest: &str, jar: bool) {
    let dir = versions.join(id);
    std::fs::create_dir_all(&dir).expect("建版本目录");
    std::fs::write(dir.join(format!("{id}.json")), manifest.as_bytes()).expect("写清单");
    if jar {
        std::fs::write(dir.join(format!("{id}.jar")), b"jar-bytes").expect("写 jar");
    }
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建父目录");
    }
    std::fs::write(path, bytes).expect("写文件");
}

/// 原版形态的最小清单（`downloads.client` 有值、无继承）。
const MANIFEST_VANILLA: &str = r#"{
  "id": "1.20.1",
  "type": "release",
  "mainClass": "net.minecraft.client.main.Main",
  "assets": "5",
  "assetIndex": { "id": "5" },
  "arguments": {
    "game": ["--username", "${auth_player_name}", "--version", "${version_name}",
             "--gameDir", "${game_directory}"],
    "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
  },
  "libraries": [],
  "downloads": { "client": { "size": 9, "url": "http://example.invalid/client.jar" } }
}"#;

/// Fabric 形态：`inheritsFrom` 指 1.20.1（父链闭合）。
const MANIFEST_FABRIC: &str = r#"{
  "id": "1.20.1-fabric-0.15.0",
  "type": "release",
  "inheritsFrom": "1.20.1",
  "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
  "libraries": []
}"#;

/// 缺父版本（父 id 在盘上不存在）。
const MANIFEST_ORPHAN: &str = r#"{
  "id": "1.20.1-orphan",
  "type": "release",
  "inheritsFrom": "1.20.1-not-on-disk",
  "mainClass": "net.minecraft.client.main.Main",
  "libraries": []
}"#;

/// 环的一侧。
const MANIFEST_CYCLE_A: &str = r#"{
  "id": "cycle-a",
  "type": "release",
  "inheritsFrom": "cycle-b",
  "mainClass": "net.minecraft.client.main.Main",
  "libraries": []
}"#;

/// 环的另一侧。
const MANIFEST_CYCLE_B: &str = r#"{
  "id": "cycle-b",
  "type": "release",
  "inheritsFrom": "cycle-a",
  "mainClass": "net.minecraft.client.main.Main",
  "libraries": []
}"#;

/// 一次登记请求（测试用：固定 `created_at`，名字缺省 = 版本 id）。
pub fn request(version_id: &str, target: AdoptTarget) -> AdoptRequest {
    AdoptRequest {
        version_id: version_id.to_string(),
        name: None,
        target,
        created_at: Some("2026-11-09T00:00:00.000Z".to_string()),
    }
}

/// 建目录符号链接（`true` = 建成）。Windows 上未开开发者模式/非管理员时可能失败。
pub fn try_symlink_dir(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(target, link).is_ok();
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_dir(target, link).is_ok();
    created
}

/// 用 `mklink /J` 建目录联接（junction 不需要管理员权限；建不出来返回 `false`）。
///
/// 只在 Windows 上有意义：junction 不是 `is_symlink`，是把别人版本目录挂进 `versions/` 最常见的
/// 形态，所以越界判据必须能看见它——这条夹具就是用来证明这点的。
#[cfg(windows)]
pub fn try_junction(target: &Path, link: &Path) -> bool {
    // `cmd /c mklink /J <link> <target>`：三个参数分开传，别拼成一整条（拼成一串会被 cmd 当语法错），
    // 路径也必须**逐段 join**（`cmd` 把正斜杠当开关）。
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output();
    match output {
        Ok(output) if output.status.success() && link.is_dir() => true,
        Ok(output) => {
            eprintln!(
                "[junction] mklink /J {} {} 失败（{:?}）：{}{}",
                link.display(),
                target.display(),
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            false
        }
        Err(error) => {
            eprintln!("[junction] 调不起 cmd：{error}");
            false
        }
    }
}

/// 建目录链接：Windows 先试符号链接、再试联接；其它平台只有符号链接。
pub fn try_link_dir(target: &Path, link: &Path) -> bool {
    if try_symlink_dir(target, link) {
        return true;
    }
    #[cfg(windows)]
    {
        try_junction(target, link)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 链接夹具建不出来时怎么走（**真值表**，复审 r1 的 R4 抓的就是这张表被写反）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkGate {
    /// 建成了：照常跑断言。
    Run,
    /// 没建成且**没要求强制**：打印一行记录后跳过（不静默）。
    Skip,
    /// 没建成且 `ULL_ADOPT_TEST_SYMLINK_REQUIRE=1`：**必红**，不许空跑。
    Fail,
}

/// 纯判据：`(建成了吗, 是否强制)` → 怎么走。
#[must_use]
pub fn link_gate(created: bool, required: bool) -> LinkGate {
    match (created, required) {
        (true, _) => LinkGate::Run,
        (false, true) => LinkGate::Fail,
        (false, false) => LinkGate::Skip,
    }
}

/// `ULL_ADOPT_TEST_SYMLINK_REQUIRE=1` 时链接夹具**不许**空跑。
#[must_use]
pub fn links_required() -> bool {
    std::env::var("ULL_ADOPT_TEST_SYMLINK_REQUIRE")
        .as_deref()
        .map(str::trim)
        == Ok("1")
}

/// 链接夹具的守卫：返回 `true` = 继续跑断言；`false` = 已显式跳过（调用方 `return`）。
///
/// `required` 由环境变量决定（[`links_required`]）；判据本身是 [`link_gate`]。
pub fn link_guard(created: bool, what: &str) -> bool {
    link_guard_with(created, links_required(), what)
}

/// 同 [`link_guard`]，但 `required` 显式传入（测试要能**执行**强制分支，而不靠改进程环境）。
///
/// # Panics
///
/// `required` 且没建成链接/联接：**红**（这是复审 r1 的 R4：旧写法把方向写反了，
/// 强制模式下建不出来反而直接「成功」返回）。
pub fn link_guard_with(created: bool, required: bool, what: &str) -> bool {
    match link_gate(created, required) {
        LinkGate::Run => true,
        LinkGate::Skip => {
            eprintln!(
                "[link-guard] {what}：本机建不出符号链接/目录联接，已显式跳过这条用例；                 设 ULL_ADOPT_TEST_SYMLINK_REQUIRE=1 可让它必跑（建不出来即红）"
            );
            false
        }
        LinkGate::Fail => panic!(
            "[link-guard] {what}：ULL_ADOPT_TEST_SYMLINK_REQUIRE=1，但本机建不出符号链接/目录联接             ——这条用例不许空跑，判红"
        ),
    }
}

/// 真值表的回归：**强制模式建不出来必须致命**（R4 的反转就是这张表写错）。
///
/// 抓得住：把 `(false, true)` 判成 `Skip`（旧实现的写法）、或把 `(false, false)` 判成 `Fail`。
/// 抓不住：真实建链接是否成功（那由各链接用例自己走 [`link_guard`]）。
#[test]
fn link_gate_truth_table() {
    assert_eq!(link_gate(true, false), LinkGate::Run);
    assert_eq!(link_gate(true, true), LinkGate::Run);
    assert_eq!(link_gate(false, false), LinkGate::Skip);
    assert_eq!(
        link_gate(false, true),
        LinkGate::Fail,
        "强制模式下建不出链接必须红"
    );
}

/// **执行**守卫的三个分支（不只测纯判据）：Run 继续、Skip 返回 false、Fail panic。
///
/// 抓得住：把 `Fail` 分支写成 `return false`（`catch_unwind` 捕不到 panic，最后一条断言红）；
/// 把 `Skip` 写成 panic（第一条 `catch_unwind` 不再 ok）。
/// 抓不住：真实建链接是否成功（各链接用例自己走 [`link_guard`]）；也不依赖进程级环境变量，
/// 因此可与并行用例共存。
#[test]
fn link_guard_executes_all_three_branches() {
    assert!(link_guard_with(true, false, "已建成"));
    assert!(
        std::panic::catch_unwind(|| link_guard_with(false, false, "非强制模式 · 建不出")).is_ok(),
        "非强制模式不许 panic"
    );
    assert!(!link_guard_with(false, false, "非强制模式 · 建不出"));
    let forced = std::panic::catch_unwind(|| link_guard_with(false, true, "强制模式 · 建不出"));
    assert!(forced.is_err(), "强制模式下建不出来必须 panic（R4）");
}
