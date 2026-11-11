//! 截图通道（D30 验收用）：`iced::window::screenshot` 的真窗口出图 → PNG，以及两图逐像素对比。
//!
//! ## 为什么用真窗口而不是 `iced_test::Emulator`
//!
//! 任务书首选路径①：真开窗截图覆盖了标题栏/DPI/窗口管理的真实行为，离屏通道不覆盖
//! （M60 §Q6 的边界说明）。本机与 Windows CI 都有交互会话，所以走路径①。
//!
//! ## 关键事实（M60 踩坑 Q3）
//!
//! `Screenshot` 的字节字段在 0.14 叫 **`rgba`**（0.13 是 `bytes`），`size` 是**物理像素**，
//! 另带 `scale_factor`（换算逻辑坐标用）。落盘用 `image::save_buffer` + `ExtendedColorType::Rgba8`。
//!
//! ## 为什么自己带一个 diff
//!
//! D30 的硬项是「GPU 档与 CPU 档**几何逐像素一致**，只有颜色/材质不同」。CI 里不该假设有
//! Python/PIL/ImageMagick，所以把「读两张 PNG → 数差异像素 → 给最大/平均通道差」做进本程序
//! （`--diff a.png b.png`），复用同一个二进制。

use std::path::{Path, PathBuf};

use iced::window;

use crate::theme;

/// 把一帧截图写到 `path`（自动建父目录）。
///
/// # Errors
/// 目录建不出来、PNG 编码失败、字节数与尺寸对不上时返回人话（截图通道要能一眼看出哪一步断）。
pub fn save(screenshot: &window::Screenshot, path: &Path) -> Result<Summary, String> {
    let width = screenshot.size.width;
    let height = screenshot.size.height;
    let bytes = screenshot.rgba.as_ref();

    let expected = width as usize * height as usize * 4;
    if bytes.len() != expected {
        return Err(format!(
            "截图字节数对不上：{width}x{height} 应该 {expected} B，实际 {} B",
            bytes.len()
        ));
    }

    write_rgba(bytes, width, height, path)?;

    Ok(Summary {
        width,
        height,
        scale_factor: screenshot.scale_factor,
        path: path.to_path_buf(),
    })
}

fn write_rgba(bytes: &[u8], width: u32, height: u32, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("建目录 {} 失败：{error}", parent.display()))?;
        }
    }

    image::save_buffer(path, bytes, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|error| format!("写 PNG {} 失败：{error}", path.display()))
}

/// 一次截图的结果摘要（打日志与报告用）。
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// 物理宽（px）。
    pub width: u32,
    /// 物理高（px）。
    pub height: u32,
    /// 窗口缩放因子（100% DPI = 1.0）。
    pub scale_factor: f32,
    /// 落盘路径。
    pub path: PathBuf,
}

impl Summary {
    /// 一行日志。
    pub fn describe(&self) -> String {
        format!(
            "[shot] {}x{} scale={} -> {}",
            self.width,
            self.height,
            self.scale_factor,
            self.path.display()
        )
    }
}

mod diff;

pub use diff::{compare, diff_files, diff_files_with_scan, shift_scan, Diff, Image};

/// 主题名 → 归档文件名片段（`visual-acceptance-protocol.md` §7 的命名）。
pub fn flavor_slug(ui: &theme::Ui) -> &'static str {
    match ui.flavor() {
        unlucky_design::color::SchemeFlavor::Light => "light",
        unlucky_design::color::SchemeFlavor::Dark => "dark",
    }
}
