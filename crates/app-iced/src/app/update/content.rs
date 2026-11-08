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
    AppCommand, ContentDetailRequest, ContentInstallRequest, ContentQuery, ContentSource,
    ExportRequestView,
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
                // M162：选中就拉这个项目的详情/版本列表（一条独立通道，失败只影响详情面板）
                self.content.selected = Some(id.clone());
                self.state.content.request_detail(&id);
                if let Some(request) = self.content_detail_request() {
                    self.dispatch(AppCommand::LoadContentDetail(Box::new(request)));
                }
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

    /// 组一份导出请求（预检与写盘**共用同一份**，确认时原样发回）。
    ///
    /// 没有选中实例就没有导出对象 ⇒ `None`。**「Save to」必须真的进请求**
    /// （复审 r1 P1-1：以前这里硬编码 `target: None`，那个输入框是死的）。
    pub(super) fn pack_export_request(&self) -> Option<ExportRequestView> {
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

    /// 详情的请求载荷（选中项 + 目标实例的版本/加载器；没有选中项就没有请求）。
    ///
    /// 详情是**独立通道**：检索结果里只有 facets（支持哪些版本/加载器），拿不到「有哪些版本
    /// 可以装」；跟着检索一起拉等于每页多 N 次请求，所以只在选中时发一条。
    fn content_detail_request(&self) -> Option<ContentDetailRequest> {
        let catalog = unlucky_i18n::Lang::En.catalog();
        let model = Content::from_snapshot(&self.state, &self.content, catalog);
        let hit = model.selected_hit()?;
        let target = model
            .targets
            .iter()
            .find(|target| Some(&target.id) == model.target.as_ref());
        Some(ContentDetailRequest {
            project: hit.id.clone(),
            source: hit.source,
            game_version: target.map(|target| target.mc_version.clone()),
            loader: target.map(|target| target.loader),
        })
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
