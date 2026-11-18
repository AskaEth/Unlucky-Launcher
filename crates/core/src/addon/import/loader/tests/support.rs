//! M311 用例的共用夹具：最小 mrpack、假装载器、启动侧读版本 JSON 的那一步。
//!
//! 从 `tests.rs` 拆出（那边贴着 400 行硬线）：与任何一条断言无关的机械动作放这里，
//! "抓得住什么"写在每条用例自己的注释里（与 `addon/pack/tests`、`presenter/.../tests/harness.rs`
//! 同一套分工）。
//!
//! # 夹具为什么自己造（不用 M110 那三份）
//!
//! 三份现成夹具（`addon/pack/tests/fixtures.rs`）的 `hashes` 是**形状正确的占位值**，用途是
//! "读得出、写得回"；要拿它们跑通导入，得配 `addon/tests/support.rs` 里的假远端 + 重写摘要，
//! 而那两处不在 M311 的 scope。第六跳与包内文件内容无关，所以用一份最小 mrpack：走的是同一条
//! 真管线（开包 → 探格式 → 读清单 → 组计划 → 落盘 → 登记），只是不需要网络。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::addon::import::{ProcessorStep, ProcessorSummary};
use crate::addon::loader::processors::{StepOutcome, StepStatus};
use crate::addon::temp::TempRoot;
use crate::game::install::instance::read_profiles;
use crate::game::java::JavaOrigin;
use crate::game::layout::GameLayout;
use crate::game::manifest::VersionJson;
use crate::platform::layout::LauncherLayout;

use super::super::{
    LoaderFuture, LoaderInstallError, LoaderInstallJob, LoaderInstalled, LoaderInstaller,
};

/// 夹具声明的加载器版本（与清单逐字一致是这一跳的硬要求）。
pub(super) const LOADER_VERSION: &str = "0.15.11";
/// 夹具声明的原版版本。
pub(super) const MC_VERSION: &str = "1.20.1";
/// 夹具应得的版本 id（`<mc>-fabric-<loader>`）。
pub(super) const VERSION_ID: &str = "1.20.1-fabric-0.15.11";

// ============================ 夹具 ============================

/// 一份最小 mrpack：依赖里声明 `fabric-loader 0.15.11`，内容全在 `overrides/` 里（零网络）。
pub(super) fn mrpack(tag: &str) -> TempRoot {
    build(
        tag,
        &dependencies(MC_VERSION, "fabric-loader", LOADER_VERSION),
        r#""files": []"#,
    )
}

/// 同上，但**带一条补不出地址的远端条目**：`Fetch` 阶段才会真的发射一次（顺序判据要用到
/// 六个阶段全在），代价是这条导入以 `Partial` 收尾（缺一个文件，可容忍）。
pub(super) fn mrpack_with_remote(tag: &str) -> TempRoot {
    build(
        tag,
        &dependencies(MC_VERSION, "fabric-loader", LOADER_VERSION),
        r#""files": [{
            "path": "mods/ghost.jar",
            "hashes": {
                "sha1": "0123456789abcdef0123456789abcdef01234567",
                "sha512": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            },
            "env": {"client": "required", "server": "required"},
            "downloads": [],
            "fileSize": 11
        }]"#,
    )
}

/// 声明任意加载器的一份最小 mrpack（M332：老版 Forge 的短形 / 双后缀形要用到）。
pub(super) fn mrpack_for(
    tag: &str,
    mc_version: &str,
    loader_key: &str,
    loader_version: &str,
) -> TempRoot {
    build(
        tag,
        &dependencies(mc_version, loader_key, loader_version),
        r#""files": []"#,
    )
}

/// mrpack 的 `dependencies` 段原文（Modrinth 的键名约定：`fabric-loader` / `forge` / `neoforge`）。
fn dependencies(mc_version: &str, loader_key: &str, loader_version: &str) -> String {
    format!(
        r#""dependencies": {{"minecraft": "{mc_version}", "{loader_key}": "{loader_version}"}}"#
    )
}

/// 打 zip（`stored`，便于断言字节）。
fn build(tag: &str, dependencies: &str, files: &str) -> TempRoot {
    let fixture = TempRoot::at(tag, "stub.mrpack");
    let file = std::fs::File::create(fixture.path()).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let index = format!(
        r#"{{
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "M311 fixture",
            {dependencies},
            {files}
        }}"#
    );
    for (name, bytes) in [
        ("modrinth.index.json", index.as_bytes()),
        ("overrides/config/demo.toml", b"k = 1\n".as_slice()),
    ] {
        writer.start_file(name, options).expect("写 zip 条目");
        writer.write_all(bytes).expect("写 zip 内容");
    }
    writer.finish().expect("收尾 zip");
    fixture
}

/// 假装载器：记下"被请求装的是哪个版本"、按剧本写/不写版本 JSON（**不联网、不起 java**）。
#[derive(Default)]
pub(super) struct FakeInstaller {
    /// 每次被调时请求的版本 id。
    pub(super) calls: Mutex<Vec<String>>,
    /// 被调时"目标实例目录存在吗"（证明装载器排在落盘**之前**）。
    pub(super) target_seen: Mutex<Vec<bool>>,
    /// 要盯的那个目标目录（由用例给）。
    pub(super) watch: Option<PathBuf>,
    /// `Some` = 换个版本装（验"版本不一致就拒"）。
    pub(super) different: Option<String>,
    /// 真**落盘**的版本 id（`Some` 时与 [`Self::different`] 分开看）。
    ///
    /// 生产实现会把落盘 id 钉成清单声明的那个（`version::adopt_declared_id`），所以夹具要能
    /// 分别表达「报回来的 id」与「版本 JSON 落在哪个 id 下」。`None` = 就是报回来的那个。
    pub(super) installs_as: Option<String>,
    /// `Some(reason)` = 装不上。
    pub(super) fail: Option<String>,
    /// 被调时就直接取消（验取消路径）。
    pub(super) cancel: bool,
    /// 往 `job.steps` 里发几步（0 = 不发，与真 Fabric 路径同形）。
    ///
    /// M321：导入路径以前根本不建 sink（`processors::run(..., None)`），所以"装了几步"在
    /// 报告与输出里都没有痕迹。这个开关让假件**像真 engine 一样**逐段外发。
    pub(super) steps: usize,
    /// 要带回去的计划期 / 解析期告警（`release.warnings` + `plan.warnings()`）。
    pub(super) warnings: Vec<String>,
    /// 要带回去的 processor 期告警（归 `ProcessorSummary` 自己，见 `support::fake_summary`）。
    pub(super) processor_warnings: Vec<String>,
}

impl FakeInstaller {
    pub(super) fn watching(target: &Path) -> Self {
        Self {
            watch: Some(target.to_path_buf()),
            ..Self::default()
        }
    }

    pub(super) fn called(&self) -> Vec<String> {
        self.calls.lock().expect("锁").clone()
    }

    pub(super) fn target_seen(&self) -> Vec<bool> {
        self.target_seen.lock().expect("锁").clone()
    }
}

impl LoaderInstaller for FakeInstaller {
    fn install<'a>(
        &'a self,
        job: LoaderInstallJob<'a>,
    ) -> LoaderFuture<'a, Result<LoaderInstalled, LoaderInstallError>> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("锁")
                .push(job.target.version_id.clone());
            if let Some(watch) = &self.watch {
                self.target_seen.lock().expect("锁").push(watch.exists());
            }
            if self.cancel {
                return Err(LoaderInstallError::Cancelled);
            }
            if let Some(reason) = &self.fail {
                return Err(
                    LoaderInstallError::failed(reason.clone()).with_hint("把加载器装好再重导一次")
                );
            }
            let installed = self
                .different
                .clone()
                .unwrap_or_else(|| job.target.version_id.clone());
            // 真装载器也会落到同一个位置：`versions/<id>/<id>.json`。
            let written = self
                .installs_as
                .clone()
                .unwrap_or_else(|| installed.clone());
            write_version_json(job.layout, &written, &job.target.mc_version);
            // M321：像真 engine 一样逐步骤外发（`job.steps` 是导入侧转交进来的那份回调）。
            if let Some(sink) = &job.steps {
                for index in 1..=self.steps {
                    sink(&StepOutcome {
                        index,
                        label: format!("net.example:step{index}:1.0"),
                        status: StepStatus::Executed,
                        outputs: 1,
                        elapsed_ms: 120,
                        stdout_tail: None,
                        stderr_tail: None,
                    });
                }
            }
            Ok(LoaderInstalled {
                version_id: installed,
                detail: format!("假件：Fabric {LOADER_VERSION}"),
                warnings: self.warnings.clone(),
                processors: (self.steps > 0)
                    .then(|| fake_summary(self.steps, &self.processor_warnings)),
            })
        })
    }
}

/// 假件版的 processor 汇总（真实现是 `ProcessorSummary::from_run`，形状逐字相同）。
fn fake_summary(steps: usize, warnings: &[String]) -> ProcessorSummary {
    ProcessorSummary {
        java: "C:/fake/java/bin/java.exe".to_string(),
        java_origin: JavaOrigin::Managed,
        java_major: Some(17),
        executed: steps,
        skipped: 0,
        steps: (1..=steps)
            .map(|index| ProcessorStep {
                index,
                label: format!("net.example:step{index}:1.0"),
                status: "executed",
                outputs: 1,
                elapsed_ms: 120,
            })
            .collect(),
        warnings: warnings.to_vec(),
    }
}

/// 写一份 loader 版本 JSON（`inheritsFrom` 指原版，与真装载器同形）。
fn write_version_json(layout: &GameLayout, id: &str, mc_version: &str) {
    let path = layout.version_json(id);
    std::fs::create_dir_all(path.parent().expect("父目录")).expect("建版本目录");
    let json = serde_json::json!({
        "id": id,
        "inheritsFrom": mc_version,
        "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
    });
    std::fs::write(&path, json.to_string()).expect("写版本 JSON");
}

/// 启动侧「读版本 JSON」那一步（与 presenter `host/launch/build.rs` 逐句同形）：
/// 读不出来 / 解析不了 = `VersionManifest` 阶段失败（+ 路径）。
pub(super) fn launch_stage_version_manifest(
    layout: &GameLayout,
    version_id: &str,
) -> Result<VersionJson, String> {
    let path = layout.version_json(version_id);
    let text =
        std::fs::read_to_string(&path).map_err(|error| format!("{}：{error}", path.display()))?;
    VersionJson::from_json_str(&text).map_err(|error| format!("{}：{error}", path.display()))
}

/// 登记过的实例数（没有 `profiles.json` 就是 0 条）。
pub(super) fn registered_count(layout: &LauncherLayout) -> usize {
    read_profiles(layout).map_or(0, |profiles| profiles.profiles.len())
}
