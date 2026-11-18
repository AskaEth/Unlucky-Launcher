//! natives 解压：把版本链里的 natives 分类器构件解到 `versions/<id>/natives/`。
//!
//! 这是 10 阶段管线里**第 6 段「环境准备」**的落盘部分（`PlanDoc/06` §1），发生顺序在
//! 命令行组装之前（HMCL 也在组装期做：`DefaultLauncher.decompressNatives`，
//! `analyse/03` §6.4-(7)）。
//!
//! # 两条路径
//!
//! - [`extract`]：真启动走这条，把原生库落到 `java.library.path`；
//! - [`preview`]：`--dry-run` 走这条，**零落盘**（不建目录、不写文件、不删文件），
//!   只回答「真启动会解出几个文件 / 目录是不是已经就绪」。断言口径与 [`extract`]
//!   共用（同一套条目筛选、同一份戳记），所以预览能当「装完能不能起」的依据，
//!   又不会在别人的版本树里留下 `natives/` 目录或改掉父目录 mtime。
//!
//! # 三个时代（`SubAgentsTalk/launch-natives-解压与时代分界.md`，piston-meta 实测）
//!
//! | 版本区间 | natives 形态 | 谁负责 `-Djava.library.path` |
//! |---|---|---|
//! | ≤1.12.2（LWJGL 2.9） | `natives` 表 + `downloads.classifiers` | 启动器补（`defaults::legacy_jvm_arguments`） |
//! | 1.13–1.18.2（LWJGL 3.1/3.2） | 同上（另有同坐标的主构件条目） | 清单自己写 `arguments.jvm` |
//! | 1.19+（LWJGL 3.3） | **没有 `natives` 表**：natives jar 是普通 classpath 库 | 清单写 4 个目录属性，LWJGL 运行期自解压 |
//!
//! 所以三个时代走**同一条代码路径**：有 jar 就解压，没 jar 就只把目录建出来
//! （1.19+ 的 `-Dorg.lwjgl.system.SharedLibraryExtractPath` 指向它；目录不存在时
//! LWJGL 会静默走到「找不到 native」）。
//!
//! # 幂等
//!
//! 解压完在目录里写一份戳记 [`jar::STAMP_FILE`]（每个源 jar 的路径 + 字节数 + mtime）。
//! 下次启动：戳记与当前 jar 集完全一致 → 目录视为就绪，**不读 jar、不写盘**直接跳过；
//! 不一致（换版本 / 库升级 / 用户删了目录）→ 清掉旧原生库再全量解压，避免两代 dll
//! 混在同一个 `java.library.path` 里（HMCL 也是每次清目录，这里只把「没变就不动」补上）。

pub(crate) mod jar;
/// 报告与错误类型（`mod.rs` 只留计划与两个入口）。
pub(crate) mod report;

use std::path::{Path, PathBuf};

use crate::game::arguments::Environment;
use crate::game::layout::GameLayout;
use crate::game::library::Library;
use crate::launch::natives::jar::{
    clear_natives, extract_jar, jar_stamps, read_stamp, write_stamp,
};

/// 戳记文件名：放在 natives 目录里，跟着目录一起删 / 建。
///
/// 公开出来是为了「一键修复」类功能能认出自己的痕迹（判断目录里哪些文件是我们解出来的）。
pub const STAMP_FILE: &str = ".ull-natives.json";

/// 一个待解压的 natives 构件（版本链里 `natives` 分类器命中的 jar）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeJar {
    /// 磁盘路径（`libraries/<group>/.../<artifact>-<ver>-natives-<os>.jar`）。
    pub path: PathBuf,
    /// 坐标 + 分类器（`org.lwjgl:lwjgl:3.2.2:natives-windows`，报错与事件里用）。
    pub name: String,
    /// 清单 `extract.exclude` 的排除前缀（`["META-INF/"]`）。
    pub exclude: Vec<String>,
}

impl NativeJar {
    /// 构造（无排除规则）。
    pub fn new(path: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            exclude: Vec::new(),
        }
    }

    /// 带清单排除规则构造。
    pub fn with_exclude(
        path: impl Into<PathBuf>,
        name: impl Into<String>,
        exclude: Vec<String>,
    ) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            exclude,
        }
    }
}

/// natives 处理的时代（决定「要不要解压」以及「`-Djava.library.path` 从哪来」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativesEra {
    /// ≤1.12.2：清单没有 `arguments.jvm`，启动器自己补 `-Djava.library.path`。
    #[default]
    Legacy,
    /// 1.13–1.18.2：清单有 `arguments.jvm`，natives 走分类器 jar（需要解压）。
    Lwjgl3,
    /// 1.19+：natives jar 是普通 classpath 库，LWJGL 3.3 运行期自解压（只需目录存在）。
    ClasspathNative,
}

impl NativesEra {
    /// 按「清单有没有 `arguments.jvm`」+「有没有 natives 分类器 jar」判时代。
    pub fn detect(manifest_jvm_args: usize, natives_jars: usize) -> Self {
        if manifest_jvm_args == 0 {
            Self::Legacy
        } else if natives_jars == 0 {
            Self::ClasspathNative
        } else {
            Self::Lwjgl3
        }
    }

    /// 事件/日志里的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Lwjgl3 => "lwjgl3",
            Self::ClasspathNative => "classpath_native",
        }
    }
}

/// 解压计划（`LaunchRequest` 的一部分，纯数据）。
///
/// 目标目录**不在**这里：唯一真相源是 `LaunchDirs::natives_dir`（`${natives_directory}` 也用它），
/// 免得出现「参数指向 A、dll 解到 B」的静默错位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativesPlan {
    /// 待解压的 natives 分类器构件。
    pub jars: Vec<NativeJar>,
    /// 时代（只影响报告字段；解压路径三时代统一）。
    pub era: NativesEra,
    /// 目录是否用户自定义（事件 payload 的 `custom` 字段，给「自备 dll 被覆盖」留证据）。
    pub custom: bool,
}

impl Default for NativesPlan {
    fn default() -> Self {
        Self {
            jars: Vec::new(),
            era: NativesEra::ClasspathNative,
            custom: false,
        }
    }
}

impl NativesPlan {
    /// 以 jar 清单构造：有 jar 按 [`NativesEra::Lwjgl3`]，没有按 [`NativesEra::ClasspathNative`]。
    ///
    /// 这一层分不出「≤1.12.2 的 LWJGL 2」与「1.13–1.18.2 的 LWJGL 3」（两者都有分类器 jar），
    /// 所以适配层用 [`NativesPlan::with_era`] 明确给值；直接构造只适合测试与脚本。
    pub fn new(jars: Vec<NativeJar>) -> Self {
        let era = NativesEra::detect(usize::from(!jars.is_empty()), jars.len());
        Self {
            jars,
            era,
            custom: false,
        }
    }

    /// 指定时代。
    pub fn with_era(mut self, era: NativesEra) -> Self {
        self.era = era;
        self
    }

    /// 标记目录为用户自定义。
    pub fn with_custom(mut self, custom: bool) -> Self {
        self.custom = custom;
        self
    }

    /// 没有任何 natives 构件（1.19+ 的常态）。
    pub fn is_empty(&self) -> bool {
        self.jars.is_empty()
    }
}

/// 解压结果 / 预览结果与失败原因单独成文件（`mod.rs` 只留计划与两个入口）。
pub use report::{NativesError, NativesReport};

/// 从版本链库表收集 natives 分类器构件（adapter / app / CLI 共用这一段，别各写一份）。
///
/// 口径与安装层 `game::install::libraries::plan` **除 `hint=local` 落点外**逐条对齐：
/// 1. 先按环境过滤（`Library::applies`）；
/// 2. `natives` 表命中当前 OS 才取分类器（1.19+ 没有 `natives` 表 → 返回空表）；
/// 3. 落点优先用清单 `downloads.classifiers.<classifier>.path`，缺了才按 Maven 坐标推导
///    （`Library::native_relative_path`）；`hint=local` 的库在实例私有目录里
///    （`analyse/03` §6.8 的 HMCL 口径）。
///
/// ⚠️ 安装层当前的 natives 分类器分支硬编码共享 `libraries/`、不认 `hint=local`（主构件认），
/// 所以「`hint=local` + `natives` 同体」的 manifest（只有 HMCL 补丁导入才会有）会出现
/// 「装到共享目录、解压去实例目录」→ 由 [`crate::launch::adapter::AdapterWarning::MissingNativeJar`]
/// 转成启动前 `missing_components` 显式失败。此处仍按 HMCL 权威口径实现，等 install 侧对齐。
pub fn collect(
    layout: &GameLayout,
    version_id: &str,
    libraries: &[Library],
    environment: &Environment,
) -> Vec<NativeJar> {
    let mut natives = Vec::new();
    for library in libraries {
        if !library.applies(environment) {
            continue;
        }
        let Some(classifier) = library.native_classifier(environment) else {
            continue;
        };
        let Ok(relative) = library.native_relative_path(&classifier) else {
            continue;
        };
        // 安装层当前对 natives 不做这条分支（见上方警告）；这里按 HMCL 口径落实例私有目录。
        let base = if library.is_local() {
            layout.version_libraries_dir(version_id)
        } else {
            layout.libraries_dir()
        };
        let exclude = library
            .extract
            .as_ref()
            .map(|extract| extract.exclude.clone())
            .unwrap_or_default();
        natives.push(NativeJar::with_exclude(
            base.join(relative),
            format!("{}:{classifier}", library.name),
            exclude,
        ));
    }
    natives
}

/// 目录状态的三种结论：两个入口共用一条判定链，别各写一份。
enum DirStatus {
    /// 无需解压：1.19+ 没有 natives 分类器 jar，只要目录在。
    NoJars,
    /// 戳记与当前 jar 集一致：目录已就绪，上次解出 `usize` 个原生库。
    Ready(usize),
    /// 需要（重新）解压。
    Stale,
}

/// 判定链 + 本次源 jar 指纹（真解压拿它写戳记，预览丢弃）。
///
/// `extract` 与 `preview` **只在这里**决定「要不要解压」：改判定只改这一处，
/// 两边不会漂。
fn inspect(dir: &Path, plan: &NativesPlan) -> (DirStatus, Vec<jar::JarStamp>) {
    let stamps = jar_stamps(&plan.jars);
    if plan.jars.is_empty() {
        return (DirStatus::NoJars, stamps);
    }
    let ready = read_stamp(dir)
        .filter(|stamp| stamp.matches(&stamps))
        .map(|stamp| stamp.files());
    match ready {
        Some(files) => (DirStatus::Ready(files), stamps),
        None => (DirStatus::Stale, stamps),
    }
}

/// 解压一个计划（幂等：目录与源 jar 都没变时零写盘）。
///
/// `--dry-run` 不走这里，走 [`preview`]：那里一个字节都不写。
///
/// # Errors
///
/// 目录为空、建目录 / 写文件失败、jar 不是合法 zip 时返回 [`NativesError`]。
pub fn extract(dir: &Path, plan: &NativesPlan) -> Result<NativesReport, NativesError> {
    if dir.as_os_str().is_empty() {
        return Err(NativesError::NoDir);
    }
    let mut report = NativesReport {
        dir: dir.to_path_buf(),
        era: plan.era,
        jars: plan.jars.len(),
        files: 0,
        skipped: 0,
        ignored: 0,
        custom: plan.custom,
        up_to_date: false,
        planned: false,
        dir_exists: false,
    };
    // 1.19+ 没有 jar 也要建目录：LWJGL 3.3 往 SharedLibraryExtractPath 里解压。
    std::fs::create_dir_all(dir)
        .map_err(|source| jar::io_error("create_natives_dir", dir, source))?;
    report.dir_exists = true;
    let (status, stamps) = inspect(dir, plan);
    match status {
        DirStatus::NoJars => {
            report.up_to_date = true;
            return Ok(report);
        }
        DirStatus::Ready(files) => {
            report.up_to_date = true;
            report.skipped = files;
            return Ok(report);
        }
        DirStatus::Stale => {}
    }

    // 源变了：先清旧原生库（避免两代 dll 混在同一个 java.library.path 里），再全量解。
    clear_natives(dir);
    for native in &plan.jars {
        extract_jar(native, dir, &mut report)?;
    }
    write_stamp(dir, report.files, &stamps)?;
    Ok(report)
}

/// 预览一个计划：算出「真启动会怎么解压」，**不建目录、不写文件、不删文件**。
///
/// `--dry-run` 的唯一环境准备入口。与 [`extract`] 共用判定链（[`inspect`]）与条目
/// 筛选口径：无 jar（1.19+）= 无需解压；戳记与当前 jar 集一致 = 已就绪；否则读 jar
/// 清点「将写下几个原生库」。区别只有「不落盘」：没有 `create_dir_all`、没有
/// `clear_natives`、没有 `write_stamp`，因此版本树的目录结构与父目录 mtime 都不动。
///
/// jar 打不开 / 不是合法 zip 照样报错（与真启动同一条失败路径），预览不会藏坏包。
///
/// # Errors
///
/// 目录为空、jar 打不开或不是合法 zip 时返回 [`NativesError`]。
pub fn preview(dir: &Path, plan: &NativesPlan) -> Result<NativesReport, NativesError> {
    if dir.as_os_str().is_empty() {
        return Err(NativesError::NoDir);
    }
    let mut report = NativesReport {
        dir: dir.to_path_buf(),
        era: plan.era,
        jars: plan.jars.len(),
        files: 0,
        skipped: 0,
        ignored: 0,
        custom: plan.custom,
        up_to_date: false,
        planned: true,
        dir_exists: dir.is_dir(),
    };
    let (status, _stamps) = inspect(dir, plan);
    match status {
        DirStatus::NoJars => {
            report.up_to_date = true;
            return Ok(report);
        }
        DirStatus::Ready(files) => {
            report.up_to_date = true;
            report.skipped = files;
            return Ok(report);
        }
        DirStatus::Stale => {}
    }

    for native in &plan.jars {
        jar::count_jar(native, &mut report)?;
    }
    Ok(report)
}
