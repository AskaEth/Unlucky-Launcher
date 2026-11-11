//! 桌面壁纸**路径探测**（D38 配色三级优先里的第②级输入）。
//!
//! 用户原话（D38，2026-11-01）：「如果用户设置了启动器背景图，则使用莫奈取色从背景图取，
//! 没有背景图时用户也可以开启莫奈取色，此时从桌面壁纸取色。」本模块只承担中间那一级的
//! **输入**：按可信度给出「这台机器的桌面壁纸可能是哪个文件」的有序候选列表。
//!
//! # 边界（只做路径探测）
//!
//! - **不解码图片、不取色、不生成调色板**。莫奈取色本身、D38 三级降级、
//!   「取色失败回落到色卡当前选择 + 设置页可读提示」都在 `crates/design` 的后续 mission。
//! - 全部找不到时返回**空列表**（[`WallpaperProbe::is_empty`]）而不是错误：没有桌面壁纸是
//!   正常状态（服务器、无桌面环境、壁纸是纯色），调用方按 D38 直接落到第③级色卡。
//! - **零 `unsafe`、零 FFI**（core 是 `#![forbid(unsafe_code)]`）：不用
//!   `SystemParametersInfo(SPI_GETDESKWALLPAPER)`、不碰 win32 注册表 API，全部走文件读取与
//!   `reg.exe` / `gsettings` / `defaults` 子进程——与 [`super::theme_probe`]、
//!   [`super::windows_version`] 同一套路子。
//!
//! # 分层与可测性
//!
//! - 采集逻辑按平台分文件（`windows` / `linux` / `macos` 子模块），**无条件编译**、不按
//!   `cfg` 裁剪：CI 是 Linux，Windows / macOS 的夹具用例同样要能在那里跑；
//! - 平台分流只发生在 [`detect_with`] 一处；
//! - 外部 IO 全部经 [`ProbeIo`]（文件内容 / 文件头 / 目录列表 / 子进程）与
//!   [`EnvSource`]（环境变量表）注入，[`detect`] 只是把它们换成真实实现。
//!
//! # 什么算「候选壁纸」
//!
//! 只收「归一化后是绝对路径 + 文件头认得出是图片」的条目（魔数集合见 `parse` 子模块的
//! `looks_like_image`）。这条校验既挡掉过期配置里指向已删除文件的路径，也挡掉 macOS
//! 二进制 plist 字符串扫描出的碎片串。

use std::path::PathBuf;

use crate::platform::paths::{EnvSource, Platform, ProcessEnv};

use self::io::{HostIo, HostRoots, ProbeIo};
use self::parse::{looks_like_image, normalize_path};

mod io;
mod linux;
mod macos;
mod parse;
mod windows;

/// 读文件头做魔数嗅探时的字节数（够 `ftyp` 分支与文本 SVG 头）。
const IMAGE_HEAD_LEN: usize = 512;

/// 一个候选壁纸是**怎么被找到的**：平台 + 桌面环境标识。
///
/// 标识只表示「这条路径从哪个通道读出来」，不承诺该通道一定存在（读不到就走下一个）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Windows：`%APPDATA%\Microsoft\Windows\Themes\TranscodedWallpaper`（系统转码后的当前壁纸）。
    WindowsTranscoded,
    /// Windows：`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes` 的 `CurrentTheme`
    /// 指向的 `.theme`（Win10+ 落在 `%LOCALAPPDATA%\Microsoft\Windows\Themes\Custom.theme`）。
    ///
    /// 这是**当前生效**的那份主题，比 `%APPDATA%` 下的主题包可靠得多。
    WindowsCurrentTheme,
    /// Windows：`%APPDATA%\Microsoft\Windows\Themes\*.theme` 里的 `Wallpaper=` 行。
    ///
    /// 用户另存的主题包（不一定被启用），只作最后一档兜底。
    WindowsThemeFile,
    /// Windows：注册表 `HKCU\Control Panel\Desktop` 的 `WallPaper` 值（`reg.exe` 子进程）。
    WindowsRegistry,
    /// Linux · GNOME：`gsettings get org.gnome.desktop.background picture-uri`(`-dark`)。
    Gnome,
    /// Linux · KDE Plasma：`plasma-org.kde.plasma.desktop-appletsrc` 的 `Image=` 行。
    Kde,
    /// Linux · swaybg / hyprpaper 配置里的 wallpaper 行。
    WlrootsCompositor,
    /// Linux · XFCE：`xfce4-desktop.xml` 的 `last-image` / `image-path` 属性。
    Xfce,
    /// macOS：`com.apple.wallpaper/Store/Index.plist` 里的 file URL / 绝对路径串。
    MacOsStore,
    /// macOS：`defaults read com.apple.desktop Background`（老式每屏壁纸设置）。
    MacOsDesktopDefaults,
}

impl SourceKind {
    /// 稳定标识（日志 / 设置页提示用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WindowsTranscoded => "windows-transcoded",
            Self::WindowsCurrentTheme => "windows-current-theme",
            Self::WindowsThemeFile => "windows-theme-file",
            Self::WindowsRegistry => "windows-registry",
            Self::Gnome => "linux-gnome",
            Self::Kde => "linux-kde",
            Self::WlrootsCompositor => "linux-wlroots",
            Self::Xfce => "linux-xfce",
            Self::MacOsStore => "macos-store",
            Self::MacOsDesktopDefaults => "macos-desktop-defaults",
        }
    }

    /// 这条通道属于哪个平台族。
    #[must_use]
    pub const fn platform(self) -> Platform {
        match self {
            Self::WindowsTranscoded
            | Self::WindowsCurrentTheme
            | Self::WindowsThemeFile
            | Self::WindowsRegistry => Platform::Windows,
            Self::MacOsStore | Self::MacOsDesktopDefaults => Platform::MacOs,
            Self::Gnome | Self::Kde | Self::WlrootsCompositor | Self::Xfce => Platform::Linux,
        }
    }
}

/// 一个候选壁纸来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperSource {
    /// 通道标识（平台 + 桌面环境）。
    pub kind: SourceKind,
    /// 归一化后的绝对路径。
    pub path: PathBuf,
    /// 原始证据：配置文件 / 注册表键 / 命令行，用于日志与设置页的「从哪读到的」。
    pub evidence: String,
}

/// 探测结果：有序候选列表 + 各步失败原因。
///
/// 列表**不是** `Result`：找不到壁纸不是错误，见模块文档的边界一节。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WallpaperProbe {
    sources: Vec<WallpaperSource>,
    notes: Vec<String>,
}

impl WallpaperProbe {
    /// 候选列表，按可信度从高到低（第一个就是首选）。
    #[must_use]
    pub fn sources(&self) -> &[WallpaperSource] {
        &self.sources
    }

    /// 首选候选（等价于 `sources().first()`）。
    #[must_use]
    pub fn best(&self) -> Option<&WallpaperSource> {
        self.sources.first()
    }

    /// 候选迭代器（按可信度顺序）。
    pub fn iter(&self) -> impl Iterator<Item = &WallpaperSource> {
        self.sources.iter()
    }

    /// 候选个数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// 一个候选都没有（调用方应落到 D38 的第③级色卡）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// 各步的失败原因（`通道: 原因`），可直接展示在设置页的提示里。
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// 取走候选列表。
    #[must_use]
    pub fn into_sources(self) -> Vec<WallpaperSource> {
        self.sources
    }
}

/// 真实探测宿主平台（[`HostIo`] + 进程环境 + 编译目标平台）。
#[must_use]
pub fn detect() -> WallpaperProbe {
    detect_with(&HostIo, &ProcessEnv, Platform::current())
}

/// 注入式探测：IO、环境变量表、平台族全部由调用方给。
///
/// 三平台采集逻辑都不带 `cfg`，所以任意宿主都能用固定夹具跑某一平台的用例。
#[must_use]
pub fn detect_with(io: &dyn ProbeIo, env: &dyn EnvSource, platform: Platform) -> WallpaperProbe {
    let Some(roots) = HostRoots::from_env(env) else {
        return WallpaperProbe {
            sources: Vec::new(),
            notes: vec!["host: HOME / USERPROFILE 都读不到，无法定位用户目录".to_owned()],
        };
    };
    let mut collect = Collect::new(io, roots, platform);
    match platform {
        Platform::Windows => windows::collect(&mut collect),
        Platform::MacOs => macos::collect(&mut collect),
        // 其它 Unix 按 Linux 的桌面习惯兜底（与 `paths` 模块的处理一致）。
        Platform::Linux | Platform::Other => linux::collect(&mut collect),
    }
    collect.finish()
}

/// 采集过程的累加器：负责归一化、去重、图片校验与失败原因记录。
pub(crate) struct Collect<'io> {
    io: &'io dyn ProbeIo,
    roots: HostRoots,
    platform: Platform,
    sources: Vec<WallpaperSource>,
    notes: Vec<String>,
    seen: Vec<String>,
}

impl<'io> Collect<'io> {
    /// 构造空累加器。
    pub(crate) fn new(io: &'io dyn ProbeIo, roots: HostRoots, platform: Platform) -> Self {
        Self {
            io,
            roots,
            platform,
            sources: Vec::new(),
            notes: Vec::new(),
            seen: Vec::new(),
        }
    }

    /// 注入的 IO（文件 / 目录 / 子进程）。
    pub(crate) fn io(&self) -> &'io dyn ProbeIo {
        self.io
    }

    /// 宿主根目录（由环境变量表推出）。
    pub(crate) fn roots(&self) -> &HostRoots {
        &self.roots
    }

    /// 记一条失败原因，探测继续往下走。
    pub(crate) fn note(&mut self, step: &str, detail: impl std::fmt::Display) {
        self.notes.push(format!("{step}: {detail}"));
    }

    /// 收一个候选：归一化 → 去重 → 图片校验，任何一步不过都只记原因、不中断。
    ///
    /// 返回是否真的收下（调用方据此决定要不要继续试别名）。
    pub(crate) fn add(&mut self, kind: SourceKind, evidence: &str, raw: &str) -> bool {
        match normalize_path(raw, &self.roots) {
            Some(path) => self.add_path(kind, evidence, path),
            None => {
                self.note(evidence, format!("不是可用的绝对路径（{raw}）"));
                false
            }
        }
    }

    /// 收一个已经算好的路径（调用方自己拼的，不再归一化）。
    pub(crate) fn add_path(&mut self, kind: SourceKind, evidence: &str, path: PathBuf) -> bool {
        let key = dedupe_key(self.platform, &path);
        if self.seen.contains(&key) {
            return false;
        }
        let head = self.io.read_head(&path, IMAGE_HEAD_LEN);
        if !head.as_deref().is_some_and(looks_like_image) {
            self.note(
                evidence,
                format!("读不到或认不出图片格式（{}）", path.display()),
            );
            return false;
        }
        self.seen.push(key);
        self.sources.push(WallpaperSource {
            kind,
            path,
            evidence: evidence.to_owned(),
        });
        true
    }

    /// 收尾。
    pub(crate) fn finish(self) -> WallpaperProbe {
        WallpaperProbe {
            sources: self.sources,
            notes: self.notes,
        }
    }
}

/// 去重键：Windows 路径大小写不敏感（同一文件可能被写出两种大小写），其它平台按原样。
fn dedupe_key(platform: Platform, path: &std::path::Path) -> String {
    let text = path.to_string_lossy().into_owned();
    if platform == Platform::Windows {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::io::{HostIo, HostRoots};
    use super::*;
    use crate::platform::paths::MapEnv;

    #[test]
    fn empty_probe_is_empty_not_error() {
        let probe = WallpaperProbe::default();
        assert!(probe.is_empty());
        assert_eq!(probe.len(), 0);
        assert!(probe.best().is_none());
        assert_eq!(probe.iter().count(), 0);
        assert!(probe.notes().is_empty());
        assert!(probe.into_sources().is_empty());
    }

    #[test]
    fn missing_home_yields_note_and_no_candidates() {
        let env = MapEnv::new();
        for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
            let probe = detect_with(&HostIo, &env, platform);
            assert!(probe.is_empty(), "{platform:?} 没有 HOME 时不该有候选");
            assert_eq!(probe.notes().len(), 1);
            assert!(probe.notes()[0].contains("HOME"));
        }
    }

    #[test]
    fn source_kind_exposes_platform_and_stable_name() {
        assert_eq!(SourceKind::Gnome.platform(), Platform::Linux);
        assert_eq!(SourceKind::WindowsRegistry.platform(), Platform::Windows);
        assert_eq!(SourceKind::MacOsStore.platform(), Platform::MacOs);
        assert_eq!(SourceKind::WindowsTranscoded.as_str(), "windows-transcoded");
        assert!(SourceKind::Kde.as_str().starts_with("linux-"));
    }

    #[test]
    fn collect_dedupes_and_keeps_priority_order() {
        let env = MapEnv::new().with("HOME", "/home/u");
        let roots = HostRoots::from_env(&env).expect("HOME 已给");
        let io = io::fake::FakeIo::new()
            .with_image("/home/u/a.png")
            .with_image("/home/u/b.jpg");
        let mut collect = Collect::new(&io, roots, Platform::Linux);
        collect.add(SourceKind::Gnome, "gsettings", "/home/u/a.png");
        collect.add(SourceKind::Kde, "kde", "file:///home/u/b.jpg");
        // 同一条路径再来一次（换了个通道）不该重复计入。
        collect.add(SourceKind::Xfce, "xfce", "/home/u/a.png");
        collect.add(SourceKind::Xfce, "xfce", "/home/u/missing.png");
        let probe = collect.finish();
        assert_eq!(probe.len(), 2);
        assert_eq!(probe.best().expect("有候选").kind, SourceKind::Gnome);
        assert_eq!(probe.sources()[1].path, PathBuf::from("/home/u/b.jpg"));
        assert!(
            probe
                .notes()
                .iter()
                .any(|note| note.contains("missing.png")),
            "落选的候选要留原因：{:?}",
            probe.notes()
        );
    }
}
