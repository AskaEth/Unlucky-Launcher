//! 整合包导入/导出区的消息落点（M113 + M160）：**从 `content.rs` 拆出来守 400 行硬线**。
//!
//! ## 为什么单独一个文件
//!
//! M160 给这一区加了预检、冲突清单、逐项决策三条通道，`content.rs` 到 409 行越线。拆的口径与
//! 仓库其它拆分一致：**纯搬移**，`on_pack` 及其私有 helper（组请求）原样搬过来，别处一行不改。
//!
//! ## 两条口径（与原来同一句）
//!
//! 1. **页内选择改本地状态**（路径草稿 / 目标实例 / 格式 / 包名 / 展开的冲突）——它们不属于业务
//!    状态，不进 presenter；
//! 2. **意图只派指令，不做乐观回显**（导入 / 预检 / 确认 / 关面板）：进度、报告、预检结果全部由
//!    `CoreEvent::Pack` 回来（`dto::pack::event` 的 `PackState::apply` 是唯一写入点）。
//!
//! ## M160 加的那条纪律
//!
//! 点「导入」先发 `PackCommand::Scan`（只读），拿到 `PackEvent::Scan` 之后：**没有冲突就直接往下导**
//! （`clock::pump` 里的 `continue_import_after_clean_scan`），有冲突才画清单等用户点头。决策的缺省
//! 档永远是 `KeepAll`——没问过用户就不会覆盖。

use unlucky_presenter::dto::{
    AppCommand, ConflictChoiceView, ConflictDecisionsView, ImportRequestView, PackCommand,
};

use crate::pages;

use super::*;

impl App {
    /// 导入/导出区的消息（M113）。
    ///
    /// 两条口径：
    ///
    /// 1. **页内选择改本地状态**（路径草稿 / 目标实例 / 格式 / 包名 / 展开的冲突）——
    ///    它们不属于业务状态，不进 presenter（同 `QueryChanged` 的处理）；
    /// 2. **意图只派指令，不做乐观回显**（导入 / 预检 / 确认写盘 / 关面板）：进度、报告、预检结果
    ///    全部由 `CoreEvent::Pack` 回来（`dto::pack::event` 的 `PackState::apply` 是唯一写入点）。
    pub(super) fn on_pack(&mut self, message: pages::content::PackMessage) {
        use pages::content::PackMessage;
        match message {
            PackMessage::Dropped(path) => {
                // 拖进来的包属于「整合包」这一类：顺手把类型切过去，否则这一块根本不在屏上
                self.content.kind = unlucky_presenter::dto::ContentKind::Modpack;
                self.content.pack.draft = path.display().to_string();
                self.content.pack.hover = false;
                self.view_dirty = true;
            }
            PackMessage::Hover(hover) => {
                self.content.pack.hover = hover;
                self.view_dirty = true;
            }
            PackMessage::PathChanged(path) => {
                self.content.pack.draft = path;
                self.view_dirty = true;
            }
            PackMessage::SetInstance(instance) => {
                self.content.pack.instance = instance;
                self.view_dirty = true;
            }
            PackMessage::SetInstanceName(name) => {
                self.content.pack.instance_name = name;
                self.view_dirty = true;
            }
            PackMessage::SetFormat(format) => {
                self.content.pack.format = format;
                self.view_dirty = true;
            }
            PackMessage::SetExportName(name) => {
                self.content.pack.export_name = name;
                self.view_dirty = true;
            }
            PackMessage::SetExportVersion(version) => {
                self.content.pack.export_version = version;
                self.view_dirty = true;
            }
            PackMessage::SetOutput(path) => {
                self.content.pack.output = path;
                self.view_dirty = true;
            }
            PackMessage::ToggleConflict(path) => {
                self.content.pack.expanded = if self.content.pack.expanded.as_deref() == Some(&path)
                {
                    None
                } else {
                    Some(path)
                };
                self.view_dirty = true;
            }
            PackMessage::OpenExport => {
                self.content.pack.export_open = true;
                // 包名缺省取目标实例名（用户不该为一个名字再敲一遍键盘）
                if self.content.pack.export_name.trim().is_empty() {
                    self.content.pack.export_name =
                        self.content.pack.instance.clone().unwrap_or_default();
                }
                self.view_dirty = true;
            }
            PackMessage::AbortExport => {
                self.content.pack.export_open = false;
                // 连预检结果一起清：留着会让用户以为「还能再点一次确认」
                self.dispatch(AppCommand::Pack(Box::new(PackCommand::Dismiss)));
                self.view_dirty = true;
            }
            PackMessage::Dismiss => {
                // 报告 / 探测诊断 / 预检都在 presenter 里：关面板 = 让 host 清掉它们
                // （`PackEvent::Cleared`），于是界面回到入口态，浏览网格重新露出来
                self.content.pack.expanded = None;
                self.dispatch(AppCommand::Pack(Box::new(PackCommand::Dismiss)));
                self.view_dirty = true;
            }
            PackMessage::Import => {
                // 先预检（M160）：没有冲突就直接导，有冲突就把清单画出来等用户点头。
                self.content.pack.pick_page = 0;
                self.content.pack.pick_choices.clear();
                self.content.pack.pick_overwrite_all = false;
                if let Some(request) = self.pack_import_request() {
                    self.dispatch(AppCommand::Pack(Box::new(PackCommand::Scan(Box::new(
                        request,
                    )))));
                }
            }
            PackMessage::PickPage(page) => {
                self.content.pack.pick_page = page;
                self.view_dirty = true;
            }
            PackMessage::TogglePick(path) => {
                // 点一下就翻到另一侧，并把结果**原样记进表里**（保留也记）。
                // 表里为什么保留"保留"项：整批覆盖之后单独放回某几条时，「保留」必须能盖住
                // 整批档；等到发请求时 `PackPageState::decisions` 再把"与缺省档相同"的项过滤掉
                // （所以"一次都没点过"仍然等于 `KeepAll`）。
                let choice = self.content.pack.pick_choice(&path);
                let flipped = match choice {
                    ConflictChoiceView::KeepUser => ConflictChoiceView::Overwrite,
                    ConflictChoiceView::Overwrite => ConflictChoiceView::KeepUser,
                };
                self.content.pack.pick_choices.insert(path, flipped);
                self.view_dirty = true;
            }
            PackMessage::PickKeepAll => {
                self.content.pack.pick_overwrite_all = false;
                self.content.pack.pick_choices.clear();
                self.view_dirty = true;
            }
            PackMessage::PickOverwriteAll => {
                self.content.pack.pick_overwrite_all = true;
                self.content.pack.pick_choices.clear();
                self.view_dirty = true;
            }
            PackMessage::ConfirmImport => {
                let decisions = self
                    .content
                    .pack
                    .decisions(self.state.pack.import.scan.as_ref());
                if let Some(request) = self.pack_import_request_with(decisions) {
                    self.dispatch(AppCommand::Pack(Box::new(PackCommand::Import(Box::new(
                        request,
                    )))));
                }
            }
            PackMessage::CancelImport => {
                // 取消 = 什么都不导（预检是只读的，没有东西要撤）
                self.content.pack.pick_page = 0;
                self.content.pack.pick_choices.clear();
                self.content.pack.pick_overwrite_all = false;
                self.dispatch(AppCommand::Pack(Box::new(PackCommand::Dismiss)));
                self.view_dirty = true;
            }
            PackMessage::Overwrite => {
                // 「以包为准重新导入」：这条是**导入之后**从报告里点的，所以直接带整批覆盖再跑一遍。
                if let Some(request) =
                    self.pack_import_request_with(ConflictDecisionsView::OverwriteAll)
                {
                    self.dispatch(AppCommand::Pack(Box::new(PackCommand::Import(Box::new(
                        request,
                    )))));
                }
            }
            PackMessage::Cancel => {
                // 与面板上那个「取消」同一个指令：host 会把导入的取消标记与代次一起处理
                self.dispatch(AppCommand::CancelInstall);
            }
            PackMessage::Precheck => {
                if let Some(request) = self.pack_export_request() {
                    // 先记下「这次预检对的是哪份请求」，确认写盘时原样发回
                    self.state.pack.set_export_request(request.clone());
                    self.view_dirty = true;
                    self.dispatch(AppCommand::Pack(Box::new(PackCommand::Export(Box::new(
                        request,
                    )))));
                }
            }
            PackMessage::Confirm => {
                if let Some(request) = self.state.pack.export.request.clone() {
                    self.dispatch(AppCommand::Pack(Box::new(PackCommand::Confirm(Box::new(
                        request,
                    )))));
                }
            }
        }
    }

    /// 预检干净（没有冲突）时直接往下导。
    ///
    /// 这条是"不打扰不该打扰的文件"的另一半：清单为空时弹一屏"共 0 项冲突"只会让人以为出错了。
    /// 决策固定给 [`ConflictDecisionsView::KeepAll`]——即使预检与导入之间磁盘上突然多出个同名文件，
    /// 结论也只会是"保留用户的"。
    pub(in crate::app) fn continue_import_after_clean_scan(&mut self) {
        if let Some(request) = self.pack_import_request_with(ConflictDecisionsView::KeepAll) {
            self.dispatch(AppCommand::Pack(Box::new(PackCommand::Import(Box::new(
                request,
            )))));
        }
    }

    /// 组一份导入请求（路径草稿 → DTO）。草稿为空 ⇒ `None`（按钮不该发指令）。
    ///
    /// **决策缺省是「全部保留」**（[`ConflictDecisionsView::default`]）：预检时还不知道有没有冲突，
    /// 而 core 侧那份缺省同样落在保留上——所以"还没问过用户"绝不会变成"覆盖"。
    fn pack_import_request(&self) -> Option<ImportRequestView> {
        self.pack_import_request_with(ConflictDecisionsView::default())
    }

    /// 组一份带决策的导入请求（逐项 / 整批都从这里出去）。
    fn pack_import_request_with(
        &self,
        decisions: ConflictDecisionsView,
    ) -> Option<ImportRequestView> {
        let draft = self.content.pack.draft.trim();
        if draft.is_empty() {
            return None;
        }
        let name = self.content.pack.instance_name.trim();
        Some(ImportRequestView {
            pack: std::path::PathBuf::from(draft),
            instance: self.content.pack.instance.clone(),
            instance_name: (!name.is_empty()).then(|| name.to_string()),
            decisions,
        })
    }
}
