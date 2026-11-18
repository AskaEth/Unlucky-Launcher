//! 归档形态的产物 → **启动器本体**（M308）。
//!
//! # 为什么有这一步
//!
//! `packaging/make_artifacts.py` 的发布矩阵里，Linux/macOS 的启动器是 `.tar.gz`（里面一个
//! `unlucky-launcher` + LICENSE/NOTICE/README），清单把它记成 [`PackageFormat::PortableZip`]。
//! M307 盘查抓到的真缺陷就是这里：`job::self_replaceable` 只认裸 exe，于是 Linux/macOS
//! **查得到更新、按钮画得出来、点下去必失败**。修法不是「把归档改名成 exe」（那只会得到一个
//! 跑不起来的文件），而是**在校验通过之后、替换之前**把本体取出来。
//!
//! # 顺序：先验签，再拆包（不许反过来）
//!
//! 清单里的 `size` / `sha256` 说的是**归档字节**，所以 [`super::verify::stage`] 仍然对归档做
//! 长度+摘要双检（先验签、再解析的同一口径见 [`super`] 模块文档）。本模块**只**在 `StagedPackage`
//! 已经存在之后被调用：拆出来的字节由「归档过了双检」背书，因此这里不另算一份摘要——
//! 多算一份只会让「信谁」变成两个答案。同理，本模块对外只交另一个 [`StagedPackage`]，
//! 替换那一跳（[`super::install::install_staged`]）一个字母都不用改。
//!
//! # 防路径穿越：条目名**从不**当路径用
//!
//! 本模块只写**一个**文件，路径是自己在暂存目录里挑的名字；归档里那些条目名只用于
//! ①挑候选 ②错误信息。所以「`../../etc/passwd`」这类条目即使存在也落不到盘上。除此之外还挡两道：
//!
//! * [`is_launcher_candidate`] 只放行**普通文件**（目录 / 符号链接 / 硬链接 / 设备条目一律不算
//!   候选），且条目名必须是相对路径、不含 `..`；
//! * 候选必须**恰好一个**：0 个或 ≥2 个都拒（凭空猜「哪个是启动器」比装失败更糟）。
//!
//! # 三种失败各有具名错误（都能归因到「谁的错」）
//!
//! | 情况 | 分类 |
//! |---|---|
//! | 归档里没有唯一的本体（0 个 / 不止一个） | [`UpdateErrorKind::LauncherMissing`] |
//! | 本体不是 `target` 那个平台的可执行文件（魔数不对） | [`UpdateErrorKind::LauncherNotExecutable`] |
//! | 本体 0 字节 / 超过上限 / 与归档头声明的大小不符 | [`UpdateErrorKind::LauncherSizeMismatch`] |
//!
//! 三条都是**发布侧**问题（换源、重试、隔一会儿再试都不会变好），所以都不算可重试。
//! 「清单说是归档、字节却不是 tar（.gz）」不在上表：那是清单与产物对不上，归
//! [`UpdateErrorKind::InvalidManifest`]。
//!
//! # 只认 tar（.gz），**不认 zip**
//!
//! 发布矩阵只出 `.tar.gz`（`make_artifacts.py` 的 `SLOTS`），而 tar 头里有 `mode` 字段——
//! 打包脚本给启动器写 `0o755`、给随包文本写 `0o644`，于是「哪个条目是本体」有客观判据。
//! zip 没有等价约定（unix 模式位住在外部属性高 16 位，DOS 写的 zip 那一格是另一种语义），
//! 挑错本体等于装错文件——所以 zip 在这里是**明确拒绝**（`InvalidManifest` 且文案点名原因），
//! 不是猜。真要发 zip，先定义「怎么挑本体」，再回来改这里。
//!
//! # 为什么不是复用 `core::game::java::extract`
//!
//! 那一份是「把归档摊成一棵目录树」：它用 `tar::Entry::unpack_in` 逐条目落盘，语义是
//! 「解压一个 JDK」。本模块一条都不要：只有一个文件、路径由自己定、大小要上限、且要看魔数。
//! 搬过来会得到一份「改得面目全非的复制品」，两份代码反而更容易漂（M307 抓到的正是两份判据漂了）。
//! 共同的**实现手段**（`flate2` / `tar`）当然是同一套——这也是本 mission 选「真做」而不是
//! 「检查阶段过滤」的理由之一：零新依赖。

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::platform::os_info::OsFamily;

use super::error::{UpdateError, UpdateErrorKind};
use super::manifest::Artifact;
use super::target::PackageFormat;
use super::verify::StagedPackage;

/// 本体大小上限（防解压炸弹：归档可能只有几 MB，摊开却是几十 GB）。
///
/// 启动器本体实测 ~20 MB 量级，256 MiB 留了一个数量级的余量；真撞上说明包不对，
/// 而不是「上限太紧」。
pub(super) const MAX_LAUNCHER_BYTES: u64 = 256 * 1024 * 1024;

/// 归档形态的产物 → 可以直接交给 `install_staged` 的启动器本体。
///
/// 裸可执行（[`PackageFormat::PortableExe`]）原样返回（只补一次可执行位）；归档
/// （[`PackageFormat::PortableZip`]）拆出本体并落成暂存目录里的一个临时文件，由返回的
/// [`StagedPackage`] 在 `Drop` 时清掉。
///
/// # Errors
///
/// 见模块文档的三张表；字节不是 tar(.gz) 是 `InvalidManifest`。
pub(super) fn launcher_binary(
    staged: StagedPackage,
    artifact: &Artifact,
) -> Result<StagedPackage, UpdateError> {
    if artifact.format != PackageFormat::PortableZip {
        make_executable(staged.path())?;
        return Ok(staged);
    }
    let format = sniff(staged.path())?;
    let found = list(staged.path(), format)?;
    let name = pick_candidate(&found, staged.path())?.name.clone();
    let out = temp_path(staged.path());
    let written = tar::extract(staged.path(), format, &name, &out)?;
    // 归档本身用完就走：`staged` 在这里被 drop（`StagedPackage::Drop` 删暂存文件），
    // 返回的那一份指向拆出来的本体。
    adopt(&out, written, &name, artifact, &staged)
}

/// 候选条目（只有名字：它进错误信息，也是第二次读时匹配那一条的键；**从不**作为落盘路径）。
///
/// 大小**不在**这里粗判：真正的门槛在 `tar::write_entry`（声明 0 / 超上限 / 写出字节数与声明不符），
/// 那里已经有归档头声明的大小，另存一份只会让「谁说了算」多一个答案。
#[derive(Debug)]
struct Candidate {
    /// 归档内的条目名。
    name: String,
}

mod tar;

/// 归档容器（按魔数判，不看扩展名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Container {
    /// gzip 包着的 tar（发布矩阵里 Linux/macOS 的 `.tar.gz`）。
    TarGz,
    /// 裸 tar（有人手打 `tar -cf`，扩展名一样是 `.tar`）。
    Tar,
}

/// 前 512 字节够判两种容器（tar 的 `ustar` 魔数在偏移 257）。
fn sniff(path: &Path) -> Result<Container, UpdateError> {
    let mut head = [0_u8; 512];
    let read = read_head(path, &mut head)?;
    let head = &head[..read];
    if head.starts_with(&[0x1f, 0x8b]) {
        return Ok(Container::TarGz);
    }
    if head.get(257..262) == Some(b"ustar") {
        return Ok(Container::Tar);
    }
    let detail = if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        "是 zip：发布矩阵只出 `.tar.gz`，而 zip 没有可靠的「哪个条目是可执行文件」判据\
         （unix 模式位 vs DOS 属性），所以不猜——发包侧请改用 tar.gz"
    } else {
        "既不是 tar.gz 也不是 tar：前 512 字节对不上任何归档魔数"
    };
    Err(UpdateError::invalid_manifest(format!(
        "清单说 {} 是归档形态，但 {} {detail}",
        PackageFormat::PortableZip.as_str(),
        path.display()
    ))
    .with_path(path))
}

fn read_head(path: &Path, head: &mut [u8]) -> Result<usize, UpdateError> {
    let mut file = File::open(path).map_err(|error| UpdateError::io_at(path, &error))?;
    let mut filled = 0;
    while filled < head.len() {
        let read = file
            .read(&mut head[filled..])
            .map_err(|error| UpdateError::io_at(path, &error))?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

/// 列出归档里的**启动器候选**（普通文件 + 可执行位 / `.exe` 名字）。
fn list(path: &Path, format: Container) -> Result<Vec<Candidate>, UpdateError> {
    tar::list(path, format == Container::TarGz)
}

/// 归档条目算不算「启动器本体」的候选。
///
/// 两条判据取并集，理由都是**发布侧的写法**：
///
/// | 判据 | 谁在用 |
/// |---|---|
/// | 有可执行位（`mode & 0o111`） | `make_artifacts.py` 给启动器打 `0o755`、给随包文本打 `0o644` |
/// | 名字以 `.exe` 结尾（不区分大小写） | Windows 那套文件名约定（归档里没有 unix 权限位时） |
///
/// 另外挡掉畸形的条目名：绝对路径、`..`、空名。本模块**不**拿条目名当落盘路径（见模块文档），
/// 这条只是「连候选都不给它」。
fn is_launcher_candidate(name: &str, mode: u32) -> bool {
    if mode & 0o111 != 0 || name.to_ascii_lowercase().ends_with(".exe") {
        return plainly_relative(name);
    }
    false
}

/// 条目名是不是一个「规规矩矩的相对路径」（没有盘符/根、没有 `..`、没有空分量）。
fn plainly_relative(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// 候选必须恰好一个：0 个（归档里没本体）与 ≥2 个（猜不出该装哪个）都拒。
fn pick_candidate<'a>(
    found: &'a [Candidate],
    archive: &Path,
) -> Result<&'a Candidate, UpdateError> {
    match found {
        [only] => Ok(only),
        [] => Err(UpdateError::new(
            UpdateErrorKind::LauncherMissing,
            format!(
                "归档 {} 里没有启动器本体：可执行位（0o111）或 `.exe` 名字的普通文件一个都没有\
                 （发布脚本是不是漏了给启动器 0o755？）",
                archive.display()
            ),
        )
        .with_path(archive)),
        many => Err(UpdateError::new(
            UpdateErrorKind::LauncherMissing,
            format!(
                "归档 {} 里有 {} 个像启动器本体的条目（{}），无法确定该装哪一个",
                archive.display(),
                many.len(),
                many.iter()
                    .map(|candidate| candidate.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            ),
        )
        .with_path(archive)),
    }
}

/// 拆出来的本体落在哪：与归档暂存文件同目录（跨卷那一跳由 `install` 负责，这里不做假设）。
fn temp_path(archive: &Path) -> PathBuf {
    let name = archive.file_name().map_or_else(
        || String::from("launcher"),
        |name| name.to_string_lossy().into_owned(),
    );
    archive.with_file_name(format!("{name}-launcher"))
}

/// 本体过三关（大小、魔数、可执行位）之后包成暂存包。
fn adopt(
    out: &Path,
    written: u64,
    name: &str,
    artifact: &Artifact,
    archive: &StagedPackage,
) -> Result<StagedPackage, UpdateError> {
    let mut head = [0_u8; 4];
    let read = read_head(out, &mut head)?;
    if !looks_startable(&head[..read], artifact.target.os) {
        let _ = fs::remove_file(out);
        return Err(UpdateError::new(
            UpdateErrorKind::LauncherNotExecutable,
            format!(
                "归档里的 `{name}` 不是 {} 平台的可执行文件（前 {} 字节是 {}，{} 要的魔数不是它；\
                 发布侧把别的平台的产物挂到了这个 target 上）",
                artifact.target.as_str(),
                read,
                hex(&head[..read]),
                artifact.target.as_str()
            ),
        )
        .with_path(archive.path()));
    }
    make_executable(out)?;
    // 摘要沿用**归档**的：拆出来的字节由「归档过了双检」背书（模块文档「先验签再拆包」）。
    Ok(StagedPackage::adopt(
        out.to_path_buf(),
        written,
        archive.integrity().clone(),
    ))
}

/// 前几个字节判「是不是这个平台的可执行文件」。
///
/// 与 `packaging/make_artifacts.py::image_arch` 同一套魔数（打包侧用来拒绝「按 win-x64 命名却是
/// arm64 的 exe」，这里是它的运行期镜像）：归档里的字节由整包摘要背书，清单**没有**说
/// 「里头的这一个是不是本平台的」——所以只能自己看。
fn looks_startable(head: &[u8], os: OsFamily) -> bool {
    /// Mach-O：64 位小端/大端 + universal（32/64）。
    const MACHO: [[u8; 4]; 4] = [
        [0xcf, 0xfa, 0xed, 0xfe],
        [0xfe, 0xed, 0xfa, 0xcf],
        [0xca, 0xfe, 0xba, 0xbe],
        [0xca, 0xfe, 0xba, 0xbf],
    ];
    match os {
        OsFamily::Windows => head.starts_with(b"MZ"),
        OsFamily::Linux => head.starts_with(b"\x7fELF"),
        OsFamily::MacOs => MACHO.iter().any(|magic| head.starts_with(magic)),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 补可执行位。
///
/// 拆出来的文件是 `File::create` 建的（Unix 上 0o644），裸 exe 产物落暂存时也一样——不补这一下，
/// 装好之后的那份**启动器起不来**（`Permission denied`），而那正是本 mission 要修的那类
/// 「看起来成功、其实坏了」。Windows 没有这一位，什么都不做。
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)
        .map_err(|error| UpdateError::io_at(path, &error))?
        .permissions()
        .mode();
    if mode & 0o111 == 0o111 {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o755))
        .map_err(|error| UpdateError::io_at(path, &error))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), UpdateError> {
    Ok(())
}
