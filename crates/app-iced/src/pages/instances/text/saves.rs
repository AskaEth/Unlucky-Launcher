//! 实例页文本入口的**存档 / 截图一族**（从 `text.rs` 拆出：那边加完缩略图那四句之后越过
//! 400 行硬线，M251）。
//!
//! 拆法是「**纯搬移 + 一族新增**」：M235 的读路径、M247 的写操作、M251 的缩略图三批 key 全在这一
//! 个 `impl Texts` 块里，`text.rs` 保留导航 / 元信息 / MODS 那几族。两处都是 `impl Texts`（Rust
//! 允许这么写），调用方依旧写 `texts.saves_*()`，路径不变。
//!
//! 口径与 `text.rs` 完全一致：**不拼中文**，每个方法只把 `Key` 翻成 `&'static str`（带占位符的用
//! `Catalog::render` 现填）；带 `{reason}` / `{file}` 的句子由调用方把「已完成的事实」传进来。

use unlucky_i18n::Key;
use unlucky_presenter::dto::{SavesFailureKind, ScreenshotThumbKind};

use super::Texts;

impl Texts {
    // ------------------------------------------------------------- 存档区（M235）
    pub(in crate::pages::instances) fn saves_title(self) -> &'static str {
        self.text(Key::InstancesSavesTitle)
    }
    /// 「3 个世界 · 12 张截图」。
    pub(in crate::pages::instances) fn saves_count(self, worlds: usize, shots: usize) -> String {
        self.0.render(
            Key::InstancesSavesCount,
            &[
                ("worlds", &worlds.to_string()),
                ("shots", &shots.to_string()),
            ],
        )
    }
    pub(in crate::pages::instances) fn saves_loading(self) -> &'static str {
        self.text(Key::InstancesSavesLoading)
    }
    pub(in crate::pages::instances) fn saves_empty(self) -> &'static str {
        self.text(Key::InstancesSavesEmpty)
    }
    pub(in crate::pages::instances) fn saves_missing_dir(self) -> &'static str {
        self.text(Key::InstancesSavesMissingDir)
    }
    pub(in crate::pages::instances) fn saves_worlds(self) -> &'static str {
        self.text(Key::InstancesSavesWorlds)
    }
    pub(in crate::pages::instances) fn saves_screenshots(self) -> &'static str {
        self.text(Key::InstancesSavesScreenshots)
    }
    pub(in crate::pages::instances) fn saves_no_worlds(self) -> &'static str {
        self.text(Key::InstancesSavesNoWorlds)
    }
    pub(in crate::pages::instances) fn saves_no_screenshots(self) -> &'static str {
        self.text(Key::InstancesSavesNoScreenshots)
    }
    /// 「有图标」：只判 `icon.png` 存不存在。
    ///
    /// M251 之后**仍未解**世界图标——缩略图墙只覆盖 `screenshots/`（D54 的范围）。这行字眼
    /// 原来写的是「缩略图墙不在 U3 范围内」，那句话今天已经失真（墙做了），改成现状描述。
    pub(in crate::pages::instances) fn saves_world_icon(self) -> &'static str {
        self.text(Key::InstancesSavesWorldIcon)
    }
    pub(in crate::pages::instances) fn saves_time_unknown(self) -> &'static str {
        self.text(Key::InstancesSavesTimeUnknown)
    }
    /// 「跳过 3 项」。
    pub(in crate::pages::instances) fn saves_skipped(self, count: usize) -> String {
        self.0
            .render(Key::InstancesSavesSkipped, &[("count", &count.to_string())])
    }
    pub(in crate::pages::instances) fn saves_open_folder(self) -> &'static str {
        self.text(Key::InstancesSavesOpenFolder)
    }
    pub(in crate::pages::instances) fn saves_retry(self) -> &'static str {
        self.text(Key::InstancesSavesRetry)
    }
    /// 读目录失败的主句：`读取 saves/ 失败：目录不可用`。
    pub(in crate::pages::instances) fn saves_failed(self, reason: &str) -> String {
        self.0
            .render(Key::InstancesSavesFailed, &[("reason", reason)])
    }
    /// 打开文件夹失败的主句：`打开文件夹失败：文件管理器没起来`。
    pub(in crate::pages::instances) fn saves_open_failed(self, reason: &str) -> String {
        self.0
            .render(Key::InstancesSavesOpenFailed, &[("reason", reason)])
    }
    /// 失败大类的短句（`SavesFailureKind` → 一句话；写操作那八类各有自己的话）。
    pub(in crate::pages::instances) fn saves_reason(self, kind: SavesFailureKind) -> &'static str {
        self.text(match kind {
            SavesFailureKind::DirUnusable => Key::InstancesSavesReasonDir,
            SavesFailureKind::InstanceUnavailable => Key::InstancesSavesReasonInstance,
            SavesFailureKind::OpenFailed => Key::InstancesSavesReasonOpen,
            SavesFailureKind::WriteUnsafeName => Key::InstancesSavesReasonUnsafeName,
            SavesFailureKind::WriteSymlink => Key::InstancesSavesReasonSymlink,
            SavesFailureKind::WriteMissing => Key::InstancesSavesReasonMissing,
            SavesFailureKind::WriteNotADirectory => Key::InstancesSavesReasonNotDir,
            SavesFailureKind::WriteConflict => Key::InstancesSavesReasonConflict,
            SavesFailureKind::WriteBoundary => Key::InstancesSavesReasonBoundary,
            SavesFailureKind::WriteTooLarge => Key::InstancesSavesReasonTooLarge,
            SavesFailureKind::GameRunning => Key::InstancesSavesReasonRunning,
            SavesFailureKind::WriteFailed => Key::InstancesSavesReasonWrite,
        })
    }

    // ------------------------------------------------- 存档写操作（M247 · SAVESOPS）
    pub(in crate::pages::instances) fn saves_rename(self) -> &'static str {
        self.text(Key::InstancesSavesRename)
    }
    pub(in crate::pages::instances) fn saves_backup(self) -> &'static str {
        self.text(Key::InstancesSavesBackup)
    }
    pub(in crate::pages::instances) fn saves_delete(self) -> &'static str {
        self.text(Key::InstancesSavesDelete)
    }
    /// 行内输入框的占位符（不是按钮文案：它说「该填什么」）。
    pub(in crate::pages::instances) fn saves_rename_placeholder(self) -> &'static str {
        self.text(Key::InstancesSavesRenamePlaceholder)
    }
    pub(in crate::pages::instances) fn saves_rename_confirm(self) -> &'static str {
        self.text(Key::InstancesSavesRenameConfirm)
    }
    pub(in crate::pages::instances) fn saves_cancel(self) -> &'static str {
        self.text(Key::InstancesSavesCancel)
    }
    /// 「正在处理…」：写请求在飞时顶替那一行的操作区。
    pub(in crate::pages::instances) fn saves_busy(self) -> &'static str {
        self.text(Key::InstancesSavesBusy)
    }
    pub(in crate::pages::instances) fn saves_delete_title(self) -> &'static str {
        self.text(Key::InstancesSavesDeleteTitle)
    }
    /// 删除确认框的正文：`{dir} 和它里面的全部内容都会被删除，且不能撤销。`
    pub(in crate::pages::instances) fn saves_delete_body(self, dir: &str) -> String {
        self.0
            .render(Key::InstancesSavesDeleteBody, &[("dir", dir)])
    }
    pub(in crate::pages::instances) fn saves_delete_confirm(self) -> &'static str {
        self.text(Key::InstancesSavesDeleteConfirm)
    }
    /// 改名成功的回执：`已把 A 改名为 B。`
    pub(in crate::pages::instances) fn saves_renamed(self, from: &str, to: &str) -> String {
        self.0
            .render(Key::InstancesSavesRenamed, &[("from", from), ("to", to)])
    }
    /// 删除成功的回执：`已删除 A。`
    pub(in crate::pages::instances) fn saves_deleted(self, dir: &str) -> String {
        self.0.render(Key::InstancesSavesDeleted, &[("dir", dir)])
    }
    /// 备份成功的回执：`已把 A 备份到 D:/…／A-…zip（1.2 MiB）。`
    pub(in crate::pages::instances) fn saves_backed_up(
        self,
        dir: &str,
        path: &str,
        size: &str,
    ) -> String {
        self.0.render(
            Key::InstancesSavesBackedUp,
            &[("dir", dir), ("path", path), ("size", size)],
        )
    }
    /// 「跳过 N 个符号链接」（备份时没跟随的那些）。
    pub(in crate::pages::instances) fn saves_backup_skipped(self, count: usize) -> String {
        self.0.render(
            Key::InstancesSavesBackupSkipped,
            &[("count", &count.to_string())],
        )
    }
    /// 重命名失败的主句：`重命名 A 失败：同名世界已存在`。
    pub(in crate::pages::instances) fn saves_rename_failed(
        self,
        file: &str,
        reason: &str,
    ) -> String {
        self.0.render(
            Key::InstancesSavesRenameFailed,
            &[("file", file), ("reason", reason)],
        )
    }
    /// 删除失败的主句。
    pub(in crate::pages::instances) fn saves_delete_failed(
        self,
        file: &str,
        reason: &str,
    ) -> String {
        self.0.render(
            Key::InstancesSavesDeleteFailed,
            &[("file", file), ("reason", reason)],
        )
    }
    /// 备份失败的主句。
    pub(in crate::pages::instances) fn saves_backup_failed(
        self,
        file: &str,
        reason: &str,
    ) -> String {
        self.0.render(
            Key::InstancesSavesBackupFailed,
            &[("file", file), ("reason", reason)],
        )
    }
    // ------------------------------------------------- 截图缩略图（M251 · SAVESHOTS）
    /// 「这一格为什么没有图」的短句（`未生成预览` / 太大 / 读不出 / 解不出）。
    pub(in crate::pages::instances) fn saves_preview_reason(
        self,
        kind: ScreenshotThumbKind,
    ) -> &'static str {
        self.text(match kind {
            ScreenshotThumbKind::TooLarge => Key::InstancesSavesPreviewTooLarge,
            ScreenshotThumbKind::Unreadable => Key::InstancesSavesPreviewUnreadable,
            ScreenshotThumbKind::Decode => Key::InstancesSavesPreviewDecode,
        })
    }

    /// 「没轮到」那一句（超 60 张刷新预算 / 这一拍还没解到）。
    pub(in crate::pages::instances) fn saves_preview_none(self) -> &'static str {
        self.text(Key::InstancesSavesPreviewNone)
    }
}
