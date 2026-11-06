//! 内容库页的消息落点（M109）：`pages::Message::Content(..)` 的穷尽分发。
//!
//! ## 两件事分开（与 `update/download.rs` 同一条纪律）
//!
//! | 意图 | 谁改状态 | 派指令吗 |
//! |---|---|---|
//! | 关键词草稿 / 选中项目 / 目标实例 | `App::content`（页面内选择） | 不派 |
//! | 检索条件（类型/来源/排序/版本/加载器/关键词提交/翻页/重试） | `AppState::content`（presenter） | 派 `SearchContent` |
//! | 装入 | 谁都不改 | 派 `InstallContent` |
//!
//! 「装入」不改本地状态是关键：装上的证据是 `InstallProgress` 事件回流到安装面板
//! （`pages/download::install_panel`），页面自己不做乐观回显。
//!
//! ## 为什么条件写进 presenter 再派
//!
//! `AppState::content.query` 是「当前结果对应哪份条件」的唯一真相：结果回来时
//! `apply_content` 用它判「迟到的事件要不要丢」。页面自己存一份就会有两份真相。

use unlucky_presenter::dto::{
    AppCommand, ContentInstallRequest, ContentQuery, ContentSource, ExportRequestView,
    ImportRequestView, PackCommand,
};

use crate::pages::content::{Blocker, Content};

use super::*;

impl App {
    /// 进入内容库页时补一次检索（首个用户动作不该是「再点一下搜索」）。
    ///
    /// 判据是 `AppState::content` 里还没有结果、也没有请求在飞：重复进出不重复打网络。
    pub(super) fn ensure_content_loaded(&mut self) {
        if self.state.content.results.is_some() || self.state.content.pending {
            return;
        }
        let query = ContentQuery::first(self.content.kind);
        self.search_content(query);
    }

    /// 页面消息（穷尽分发：漏一支报 E0004）。
    pub(super) fn on_content(&mut self, message: pages::content::Message) -> Task<Message> {
        match message {
            pages::content::Message::SetKind(kind) => {
                // 换类型 = 换一份条件（facet 全变）：选中项作废，条件回到该类型的首页
                self.content.kind = kind;
                self.content.selected = None;
                let mut query = ContentQuery::first(kind);
                // 保留用户已经选好的来源/排序/版本/加载器：换成默认值会让筛选条自己跳回去
                query.source = self.state.content.query.source;
                query.sort = self.state.content.query.sort;
                query.game_version = self.state.content.query.game_version.clone();
                query.loader = self.state.content.query.loader;
                self.search_content(query);
            }
            pages::content::Message::QueryChanged(text) => {
                self.content.draft = text;
                self.view_dirty = true;
            }
            pages::content::Message::Search => {
                let mut query = self.state.content.query.clone();
                query.text = self.content.draft.trim().to_string();
                query.offset = 0;
                self.search_content(query);
            }
            pages::content::Message::SetSource(source) => {
                if source == ContentSource::Curseforge && !self.curseforge_available() {
                    // 筛选条那条本来就是禁用的；这里再挡一次（防绕过 UI 的消息）
                    return Task::none();
                }
                self.with_query(|query| query.source = source);
            }
            pages::content::Message::SetSort(sort) => self.with_query(|query| query.sort = sort),
            pages::content::Message::SetVersion(version) => {
                self.with_query(|query| query.game_version = version);
            }
            pages::content::Message::SetLoader(loader) => {
                self.with_query(|query| query.loader = loader)
            }
            pages::content::Message::Select(id) => {
                self.content.selected = Some(id);
                self.view_dirty = true;
            }
            pages::content::Message::SetTarget(id) => {
                self.content.target = Some(id);
                self.view_dirty = true;
            }
            pages::content::Message::LoadMore => {
                let next = self.state.content.query.next_page();
                self.search_content(next);
            }
            pages::content::Message::Retry => {
                let mut query = self.state.content.query.clone();
                query.offset = 0;
                self.search_content(query);
            }
            pages::content::Message::Install => {
                if let Some(request) = self.content_install_request() {
                    self.dispatch(AppCommand::InstallContent(Box::new(request)));
                }
            }
            // M113: import/export —— 导入/导出区（细节在 `on_pack`）
            pages::content::Message::Pack(message) => self.on_pack(message),
        }
        Task::none()
    }

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
                if let Some(request) = self.pack_import_request(false) {
                    self.dispatch(AppCommand::Pack(Box::new(PackCommand::Import(Box::new(
                        request,
                    )))));
                }
            }
            PackMessage::Overwrite => {
                // 「以包为准重新导入」：整次导入翻成 Overwrite 档（core 只有全局档）
                if let Some(request) = self.pack_import_request(true) {
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

    /// 组一份导入请求（路径草稿 → DTO）。草稿为空 ⇒ `None`（按钮不该发指令）。
    ///
    /// `overwrite` 只有「以包为准重新导入」会给 `true`；默认路径永远是 `false`
    /// （core 那边就是 `ConflictPolicy::KeepUser`，**绝不默认覆盖用户改过的文件**）。
    fn pack_import_request(&self, overwrite: bool) -> Option<ImportRequestView> {
        let draft = self.content.pack.draft.trim();
        if draft.is_empty() {
            return None;
        }
        let name = self.content.pack.instance_name.trim();
        Some(ImportRequestView {
            pack: std::path::PathBuf::from(draft),
            instance: self.content.pack.instance.clone(),
            instance_name: (!name.is_empty()).then(|| name.to_string()),
            overwrite,
        })
    }

    /// 组一份导出请求（预检与写盘**共用同一份**，确认时原样发回）。
    ///
    /// 没有选中实例就没有导出对象 ⇒ `None`。**「Save to」必须真的进请求**
    /// （复审 r1 P1-1：以前这里硬编码 `target: None`，那个输入框是死的）。
    fn pack_export_request(&self) -> Option<ExportRequestView> {
        let instance = self.content.pack.instance.clone()?;
        Some(pages::content::request_export(
            &self.content.pack,
            &instance,
        ))
    }

    /// 拖入一个文件/目录（`app/update.rs` 的窗口事件转发到这里）。
    pub(super) fn on_pack_dropped(&mut self, path: std::path::PathBuf) -> Task<Message> {
        // 拖入是「现在就要看到结果」的动作：先把用户带到内容库页，再落草稿
        self.state
            .navigate(unlucky_presenter::router::Route::Content);
        self.on_pack(pages::content::PackMessage::Dropped(path));
        self.refresh_view();
        Task::none()
    }

    /// 拖拽悬停状态（有文件在窗口上方时高亮入口）。
    pub(super) fn on_pack_hover(&mut self, hover: bool) -> Task<Message> {
        // 状态没变就不重画：winit 对同一批文件会连发几次 hover 事件
        let changed = self.content.pack.hover != hover;
        self.on_pack(pages::content::PackMessage::Hover(hover));
        if changed {
            self.refresh_view();
        }
        Task::none()
    }

    /// 改一个筛选字段并**立刻重查**（第一页）。
    fn with_query(&mut self, change: impl FnOnce(&mut ContentQuery)) {
        let mut query = self.state.content.query.clone();
        change(&mut query);
        query.offset = 0;
        self.search_content(query);
    }

    /// 提交一次检索：条件先落进 presenter（迟到事件的判据），再派指令。
    fn search_content(&mut self, query: ContentQuery) {
        self.state.set_content_query(query.clone());
        self.view_dirty = true;
        self.dispatch(AppCommand::SearchContent(query));
    }

    /// CF 源可用吗（结果里带回的那一位；还没拉过 ⇒ `false`）。
    fn curseforge_available(&self) -> bool {
        self.state
            .content
            .results
            .as_ref()
            .is_some_and(|results| results.curseforge)
    }

    /// 组一份内容库的「装入」请求；闸门与页面上的按钮**同一个判据**（`Content::can_install`）。
    ///
    /// 名字带 `content_`：M105 的版本安装已经占了 `install_request`（版本 → `CreateRequest`），
    /// 两者语义完全不同，撞名会被编译器拦下（本轮 rebase 撞过）。
    ///
    /// 这里再判一次不是重复劳动：页面禁用的只是那一个按钮，消息仍然可能从别的路径进来，
    /// 而 host 侧对整合包会直接回失败——三道闸门同一个结论，才不会出现「有的路能过」。
    fn content_install_request(&self) -> Option<ContentInstallRequest> {
        let catalog = unlucky_i18n::Lang::En.catalog();
        let model = Content::from_snapshot(&self.state, &self.content, catalog);
        if !model.can_install() {
            // 有原因就打印出来（取证用）；没有选中项时什么都不做
            for reason in model.blocker_reasons(catalog) {
                eprintln!("[content] 装入被挡下：{reason}");
            }
            if model.blockers.contains(&Blocker::NeedsPipeline) {
                eprintln!("[content] 整合包装入未实现（M111）");
            }
            return None;
        }
        let hit = model.selected_hit()?;
        let target = model
            .targets
            .iter()
            .find(|target| Some(&target.id) == model.target.as_ref())?;
        Some(ContentInstallRequest {
            project: hit.id.clone(),
            title: hit.title.clone(),
            // 整合包走不到这里：`ContentKind::Modpack` 的 `dir()` 是 `None` ⇒ 必然带
            // `Blocker::NeedsPipeline` ⇒ `can_install()` 为假。所以这里就是「单文件」那一类。
            kind: model.kind,
            source: hit.source,
            instance: target.id.clone(),
            game_version: target.mc_version.clone(),
            loader: target.loader,
        })
    }
}
