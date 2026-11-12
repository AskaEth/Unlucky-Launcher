//! 一张截图的**预览**（M251 · SAVESHOTS，D54 第 2/6 条）。
//!
//! ## 像素为什么走 DTO
//!
//! D54 定「**不落盘缩略图**」：写 PNG 要编码器，而编码器只为一份磁盘缓存引入不划算。于是像素
//! 就直接躺在快照里跟着 IPC 面走，由壳层用 `iced::widget::image::Handle::from_rgba` 建句柄。
//! 代价是快照变大（60 张 × 160² × 4B ≈ 6MB），收益是零编码、零临时文件、零清理。
//!
//! ## 三种状态，一种都不许静默
//!
//! | 状态 | 什么时候 | 界面 |
//! |---|---|---|
//! | [`ScreenshotPreview::Ready`] | 解出来了 | 缩略图 |
//! | [`ScreenshotPreview::NotGenerated`] | 本次刷新超出 60 张上限（按 mtime 倒序只解前 60） | 「未生成预览」占位 |
//! | [`ScreenshotPreview::Failed`] | 超过 8 MiB / 读不出来 / 解不出来 | 占位 + **逐条**原因 |
//!
//! **不拼人读文案**（M161）：这里只有结构与 [`ScreenshotThumbKind`]，句子由 app 层按 i18n key 现求。
//!
//! ## 为什么失败也带 `detail`
//!
//! 「读不出来」有五种完全不同的原因（权限 / 被占用 / 文件已删 / 是目录 / 盘掉了），用户能自己
//! 处理的那几种全靠这句话。原文只进括号，不进主句（与 [`super::SavesFailureView`] 同一条口径）。

use serde::{Deserialize, Serialize};

/// 一张截图的预览状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenshotPreview {
    /// 还没生成：本次刷新超出上限（按 mtime 倒序只解前 60 张，上限常量在
    /// `crate::host::saves::thumbs::MAX_PER_REFRESH`——策略住在解码那一侧，DTO 只表达状态）。
    NotGenerated,
    /// 解出来了（RGBA8，行主序，`width × height × 4`）。
    Ready(ThumbnailView),
    /// 没解出来（逐条可见的原因 + 原文）。
    Failed(ScreenshotThumbFailure),
}

impl ScreenshotPreview {
    /// 这一拍有没有可画的像素。
    #[must_use]
    pub fn thumbnail(&self) -> Option<&ThumbnailView> {
        match self {
            Self::Ready(view) => Some(view),
            Self::NotGenerated | Self::Failed(_) => None,
        }
    }
}

/// 一张缩略图的像素（**未落盘**，见模块文档）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbnailView {
    /// 宽（像素）。
    pub width: u32,
    /// 高（像素）。
    pub height: u32,
    /// RGBA8 字节，行主序，长度必须是 `width × height × 4`。
    pub pixels: Vec<u8>,
}

impl ThumbnailView {
    /// 像素数与宽高对得上吗（壳层建 `Handle` 之前**必须**成立：`from_rgba` 不校验长度，
    /// 长度不对就是一张花屏图或者一次 panic 后的空窗）。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        let expected = u64::from(self.width) * u64::from(self.height) * 4;
        expected == self.pixels.len() as u64
    }
}

/// 一张截图的预览为什么没出来（**每一种在 UI 上都有一句话**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotThumbFailure {
    /// 大类。
    pub kind: ScreenshotThumbKind,
    /// 底层原文（OS 错误 / 解码器原文）；只进括号，不进主句。
    pub detail: String,
}

/// 预览失败的大类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenshotThumbKind {
    /// 单张超过解码上限（8 MiB）：**不解码**，直接跳过（D54 第 5 条）。
    TooLarge,
    /// 读不出来（不存在 / 无权限 / 被占用）。
    Unreadable,
    /// 读到了但解不出来（坏 PNG / 不支持的格式）。
    Decode,
}
