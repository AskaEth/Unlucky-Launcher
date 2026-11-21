//! 带进度与取消的 Java 扫描（M175）：**先收集候选、再逐个探测**。
//!
//! 为什么不能直接调 [`super::discover::candidates_in_roots`]：那条路把「收集」与「探测」揉在一起，
//! 一个 JDK 跑一次 `java -version`（几十到几百毫秒），一台装了七八个 JDK 的机器上整段要几秒。
//! 设置页要的是**能显示 3/7、能中途取消、且不卡住 UI**——于是这里把两段拆开：
//!
//! ```text
//! candidates(roots, manual, system)   纯文件系统，快、无子进程
//!   → scan(&candidates, on_progress, cancelled)  逐个探测；任一回调说停就停
//! ```
//!
//! 判据与 [`super::discover`] 一致（同一套去重键、同一套标准位置），差别只有「可观测」这一层——
//! 所以 `ull java list` 与设置页看到的是同一批 JDK。

use std::path::PathBuf;

use super::discover::{self, path_key};
use super::extract::{java_binary_name, read_stamp};
use super::probe::{self, JavaOrigin, JavaRuntime};
use super::vendor::JdkVendor;

/// 候选来源（比 [`JavaOrigin`] 多一个「用户手动加的」，UI 也按它分组）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// 托管目录（`ull java install` 装的）
    Managed,
    /// `JAVA_HOME`
    JavaHome,
    /// `PATH` 上的 `java`
    Path,
    /// 标准安装位置（Program Files 系 / 官方启动器 runtime / 注册表 / `/usr/lib/jvm`）
    System,
    /// 用户在设置页手动指的路径
    Manual,
}

impl Source {
    /// 稳定 id（DTO / UI 文案选档用）。
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::JavaHome => "env",
            Self::Path => "path",
            Self::System => "system",
            Self::Manual => "manual",
        }
    }

    /// 展示优先级（小者先；与 [`JavaOrigin::rank`] 同序，手动路径排在系统安装之前——
    /// 用户亲手指的是明确的意图）。
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Managed => 1,
            Self::JavaHome => 2,
            Self::Path => 3,
            Self::Manual => 4,
            Self::System => 5,
        }
    }

    /// 探测时用的 [`JavaOrigin`]（`JavaOrigin` 没有「手动」这一档，用 `System` 承接下来即可：
    /// 展示走本枚举的 [`Source::id`]，不看 `JavaRuntime.origin`）。
    #[must_use]
    pub const fn origin(self) -> JavaOrigin {
        match self {
            Self::Managed => JavaOrigin::Managed,
            Self::JavaHome => JavaOrigin::JavaHome,
            Self::Path => JavaOrigin::Path,
            Self::System | Self::Manual => JavaOrigin::System,
        }
    }
}

/// 一个待探测的候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// `bin/java[.exe]` 的绝对路径
    pub binary: PathBuf,
    /// 来源
    pub source: Source,
}

/// 收集候选（**不探测**，所以很快；目录不存在一律跳过）。
///
/// `system = false` 用于测试与「只看托管区」的场景：它跳过 `JAVA_HOME`/`PATH`/标准位置，
/// 于是同一份输入在任何机器上得到同一份候选表（不依赖本机装了什么）。
#[must_use]
pub fn candidates(managed_roots: &[PathBuf], manual: &[PathBuf], system: bool) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for root in managed_roots {
        out.extend(managed_in(root));
    }
    // 手动路径：目录 → `bin/java`；文件原样（用户在输入框里粘过两种形态，都认）。
    for path in manual {
        let binary = if path.is_dir() {
            path.join("bin").join(java_binary_name())
        } else {
            path.clone()
        };
        if binary.is_file() {
            out.push(Candidate {
                binary,
                source: Source::Manual,
            });
        }
    }
    if system {
        out.extend(env_candidates());
        out.extend(
            discover::scan_java_binaries()
                .into_iter()
                .map(|binary| Candidate {
                    binary,
                    source: Source::System,
                }),
        );
    }
    dedupe(out)
}

/// 托管安装根里的候选（两种布局都认，**必须有戳记**——没戳记的目录可能是用户塞的别的东西）。
fn managed_in(root: &std::path::Path) -> Vec<Candidate> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() || name.starts_with('.') {
            continue;
        }
        if let Some(binary) = stamped_binary(&path) {
            found.push(Candidate {
                binary,
                source: Source::Managed,
            });
            continue;
        }
        // 两层布局 `<vendor>/<version>`：厂商名取自父目录名。
        if JdkVendor::parse(&name).is_some() {
            let Ok(children) = std::fs::read_dir(&path) else {
                continue;
            };
            for child in children.flatten() {
                if let Some(binary) = stamped_binary(&child.path()) {
                    found.push(Candidate {
                        binary,
                        source: Source::Managed,
                    });
                }
            }
        }
    }
    found
}

/// 带戳记、且 `bin/java[.exe]` 在盘上的托管目录。
fn stamped_binary(dir: &std::path::Path) -> Option<PathBuf> {
    if !dir.is_dir() || read_stamp(dir).is_none() {
        return None;
    }
    let binary = dir.join("bin").join(java_binary_name());
    binary.is_file().then_some(binary)
}

/// `JAVA_HOME` 与 `PATH`（与 [`discover::candidates_in_roots`] 同一口径）。
fn env_candidates() -> Vec<Candidate> {
    let mut found = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let binary = PathBuf::from(home).join("bin").join(java_binary_name());
        if binary.is_file() {
            found.push(Candidate {
                binary,
                source: Source::JavaHome,
            });
        }
    }
    if let Some(binary) = probe::resolve_on_path(java_binary_name()) {
        found.push(Candidate {
            binary,
            source: Source::Path,
        });
    }
    found
}

/// 按路径去重，保留**先出现**的（调用方按优先级排好序）。
fn dedupe(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut seen: Vec<String> = Vec::with_capacity(candidates.len());
    let mut kept = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let key = path_key(&candidate.binary);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        kept.push(candidate);
    }
    kept
}

/// 逐个探测。`on_progress(done, total)` 返回 `false` ⇒ 立刻停（取消），已探测的照样返回。
///
/// `cancelled()` 在一次探测**进行中**也会被查询：卡住的候选会被 [`super::proc::PROBE_TIMEOUT`]
/// 或取消打断，而不是把扫描线程钉死在一个 `-version` 上（见 [`super::proc::run_with_timeout`]）。
///
/// 返回的列表按**大版本升序、同版本按路径**排序，探测不出主版本的排在最后
/// （与 [`super::probe::discover_managed`] 的排序口径一致，用户看到的第一行不该是「版本未知」）。
#[must_use]
pub fn scan<F>(
    candidates: &[Candidate],
    mut on_progress: F,
    cancelled: &dyn Fn() -> bool,
) -> Vec<JavaRuntime>
where
    F: FnMut(usize, usize) -> bool,
{
    let total = candidates.len();
    let mut found: Vec<JavaRuntime> = Vec::with_capacity(total);
    for (index, candidate) in candidates.iter().enumerate() {
        if cancelled() || !on_progress(index, total) {
            break;
        }
        // 发行方要从**整段** `-version` 输出里认（首行只有 `openjdk version "…"`，品牌在第二行）。
        // 超时与取消都交给这一层：卡住的子进程会被 kill，取消不必等它自己结束。
        let (runtime, _output) = super::manual::probe_with_timeout(
            &candidate.binary,
            candidate.source.origin(),
            super::proc::PROBE_TIMEOUT,
            cancelled,
        );
        found.push(runtime);
    }
    found.sort_by(|left, right| {
        left.major
            .unwrap_or(u32::MAX)
            .cmp(&right.major.unwrap_or(u32::MAX))
            .then_with(|| left.binary.cmp(&right.binary))
    });
    found
}

/// 候选 + 扫描一步到位（不与 UI 打交道的调用方用；设置页走两段式）。
#[must_use]
pub fn scan_roots(managed_roots: &[PathBuf], manual: &[PathBuf], system: bool) -> Vec<JavaRuntime> {
    let list = candidates(managed_roots, manual, system);
    scan(&list, |_, _| true, &|| false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-java-staged-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("临时目录");
        dir
    }

    /// 造一个「看起来像 JDK」的目录（`bin/java` 是空文件 ⇒ 探测出「版本未知」，
    /// 但它确实被扫到了——这正是「列表来自真实扫描」要的证据）。
    fn fake_jdk(root: &std::path::Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("bin")).expect("bin");
        std::fs::write(dir.join("bin").join(java_binary_name()), b"").expect("写 java");
        dir
    }

    #[test]
    fn the_list_comes_from_the_disk_not_from_a_constant() {
        let root = temp("list");
        let one = fake_jdk(&root, "jdk-a");
        let two = fake_jdk(&root, "jdk-b");

        let found = scan_roots(&[], &[one.clone(), two.clone()], false);
        assert_eq!(found.len(), 2, "两个假 JDK 目录必须扫出两项：{found:?}");
        assert!(found
            .iter()
            .any(|runtime| runtime.binary == one.join("bin").join(java_binary_name())));

        // 删掉一个：列表必须少一项（证明它读的是盘，不是写死的字符串）。
        std::fs::remove_dir_all(&one).expect("删 jdk-a");
        let after = scan_roots(&[], &[one, two], false);
        assert_eq!(after.len(), 1, "删掉一个 JDK 之后列表必须少一项：{after:?}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_managed_directory_without_a_stamp_is_not_claimed() {
        let root = temp("stamp");
        // 长得像托管目录，但没有戳记 ⇒ 不是我们装的，不去猜。
        fake_jdk(&root, "jdk-21-temurin");
        let found = scan_roots(std::slice::from_ref(&root), &[], false);
        assert!(found.is_empty(), "无戳记目录不该被认领：{found:?}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn progress_reports_every_candidate_and_cancel_stops_early() {
        let root = temp("progress");
        let paths: Vec<PathBuf> = (0..3)
            .map(|n| fake_jdk(&root, &format!("jdk-{n}")))
            .collect();
        let list = candidates(&[], &paths, false);
        assert_eq!(list.len(), 3);

        let mut seen = Vec::new();
        let all = scan(
            &list,
            |done, total| {
                seen.push((done, total));
                true
            },
            &|| false,
        );
        assert_eq!(seen, vec![(0, 3), (1, 3), (2, 3)], "每个候选都要报一次进度");
        assert_eq!(all.len(), 3);

        let stopped = scan(&list, |done, _| done < 1, &|| false);
        assert_eq!(stopped.len(), 1, "取消之后只留已探测的那一项");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn candidates_dedupe_the_same_jdk_written_two_ways() {
        let root = temp("dedupe");
        let jdk = fake_jdk(&root, "jdk-x");
        let binary = jdk.join("bin").join(java_binary_name());
        // 同一份 JDK 用「目录」和「可执行文件」两种写法各登记一次。
        let list = candidates(&[], &[jdk, binary], false);
        assert_eq!(list.len(), 1, "同一路径只列一次：{list:?}");
        std::fs::remove_dir_all(&root).ok();
    }
}
