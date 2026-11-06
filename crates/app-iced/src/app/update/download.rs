//! 下载页在 app 层的那几支（M94；M105 补上安装选项）。
//!
//! 为什么单独一个文件：M93（外观三件套接线）合入后 `update.rs` 已经贴到 400 行，下载页再往里塞
//! 几个 arm 就顶破硬线；拆出来是既不动 M93 的代码、又能守住 ≤400 的办法（`app/update/**` 属本
//! mission scope）。
//!
//! ## 各支各自的纪律（M105 后的口径）
//!
//! | 消息 | 落点 | 为什么 |
//! |---|---|---|
//! | [`pages::Message::SelectVersion`] | 只改**草稿** | 选中是页面内状态，不是业务命令；顺手清掉加载器版本（换了原版版本，旧版本号几乎必然不适用） |
//! | [`pages::Message::SetLoader`] | 改草稿 + 必要时派 `ListLoaderVersions` | 加载器版本列表来自官方元数据，页面不能自己编 |
//! | [`pages::Message::SetLoaderVersion`] / `SetInstanceName` | 只改草稿 | 纯输入 |
//! | [`pages::Message::InstallVersion`] | `AppCommand::CreateInstance(草稿)` | **确认才开工**（带加载器 / 加载器版本 / 实例名） |
//! | [`pages::Message::CancelInstall`] | `AppCommand::CancelInstall` | 见 `host/dispatch.rs`：真停 + 作废旧代次 |
//! | [`pages::Message::SetCategory`] | 写分类 → 重建 → `AppCommand::ListVersions` | 见下 |
//!
//! [「刷新下载源」](pages::Action::RefreshSources) 也在这里：它不是占位动作，具体分类 → 重拉该分类、
//! 「全部」→ 让 host 重扫一遍（`RefreshAll`）。
//!
//! ## 分类为什么既写视图模型、又派指令
//!
//! 真 host 的 `AppCommand::ListVersions(_)` 会丢掉分类参数、回全量清单（复审 P2-1），所以**过滤在
//! 本页做**：写进视图模型 → 立刻 `refresh_view()`（它会把 category 带回去、按同一谓词过滤行）→
//! 再派指令。这不是「乐观回显」：写的是**过滤条件本身**，chip 与列表用同一个
//! [`Category::matches`](crate::pages::Category)，于是不可能出现「chip 说正式版、列表是全量」的两份真相。
//!
//! ## 草稿为什么住在这里而不是页面模型里
//!
//! `refresh_view()` 每次事件都把 `self.view` 整棵重建（连安装进度都算），页面存不住「用户正在填、
//! 还没确认」的东西。于是草稿放 `App::download_draft`，与分类筛选同一个做法：
//! 改草稿 → `view_dirty = true` → `refresh_view()`。

use unlucky_presenter::dto::CreateRequest;

use super::*;

impl App {
    /// 下载页的消息。**处理了返回 `true`**（`on_page` 里就不用再写一遍这些 arm）。
    pub(super) fn on_download(&mut self, message: &pages::Message) -> bool {
        match message {
            pages::Message::SelectVersion(id) => {
                if self.download_draft.selected.as_deref() != Some(id.as_str()) {
                    // 换了原版版本：加载器版本是**按 (加载器, 原版版本)** 取的（见
                    // `CoreEvent::LoaderVersions` 的缓存键），旧的版本号留着只会误导。
                    self.download_draft.loader_version.clear();
                }
                self.download_draft.selected = Some(id.clone());
                self.mark_download_dirty();
                // 选了加载器版本之后换原版版本，列表要跟着换——已经查过的直接从快照读，不重复派。
                self.ensure_loader_versions();
            }
            pages::Message::SetLoader(loader) => {
                self.download_draft.loader = *loader;
                self.download_draft.loader_version.clear();
                self.mark_download_dirty();
                self.ensure_loader_versions();
            }
            pages::Message::SetLoaderVersion(version) => {
                self.download_draft.loader_version = version.clone();
                self.mark_download_dirty();
            }
            pages::Message::SetInstanceName(name) => {
                self.download_draft.instance_name = name.clone();
                self.mark_download_dirty();
            }
            pages::Message::InstallVersion(id) => {
                // 防守：按钮带的是**页面上那一行**的 id，草稿里的选中项理论上与它一致；
                // 万一不一致（例如列表刚换过、用户手快），不能让 A 版本的加载器版本落到 B 版本上。
                if self.download_draft.selected.as_deref() != Some(id.as_str()) {
                    self.download_draft.selected = Some(id.clone());
                    self.download_draft.loader_version.clear();
                }
                let request = self.install_request(id);
                self.dispatch(AppCommand::CreateInstance(request));
            }
            pages::Message::CancelInstall => self.dispatch(AppCommand::CancelInstall),
            pages::Message::SetCategory(category) => {
                self.view.download.category = *category;
                self.view_dirty = true;
                self.refresh_view();
                if let Some(kind) = category.command() {
                    self.dispatch(AppCommand::ListVersions(kind));
                }
            }
            // M107：查询词落在快照里（无状态视图的唯一存活方式），再走同一条重建路径过滤。
            pages::Message::SetVersionQuery(query) => {
                self.rebuild_after_query(query.clone());
            }
            pages::Message::ClearVersionQuery => {
                self.rebuild_after_query(String::new());
            }
            // M107：主版本桶的选中是纯视图态，不重建、不派命令（同 `SelectVersion`）。
            //
            // **搜索期间点桶 = 「我就是要看这个桶」**（复审 r2 P2-1(b)）：搜索让第二级失效（P2-1 的
            // 一致性放宽），如果这里只改选择而不清查询，用户会看到「点了没反应、清空搜索后才生效」
            // 的活控件。所以搜索中点桶顺带退出搜索，按新桶过滤——一次点击做完用户想要的两件事。
            pages::Message::SelectMajor(key) => {
                if self.view.download.has_query() {
                    // 先退出搜索再落选择：`rebuild_after_query` 会用**全量行**重算桶并回填选择
                    // （它带回 `selected_major`，且校验桶仍存在）。
                    self.view.download.selected_major = key.clone();
                    self.rebuild_after_query(String::new());
                } else {
                    self.view.download.selected_major = key.clone();
                }
            }
            // M107：类型栏的手动折叠也是纯视图态。
            pages::Message::ToggleTypeRail => {
                self.view.download.type_rail_folded = !self.view.download.type_rail_folded;
            }
            pages::Message::Action(pages::Action::RefreshSources) => {
                match self.view.download.category.command() {
                    Some(kind) => self.dispatch(AppCommand::ListVersions(kind)),
                    None => self.dispatch(AppCommand::RefreshAll),
                }
            }
            _ => return false,
        }
        true
    }

    /// 改草稿之后统一走这一条：置脏 + 立刻重建（不等下一拍，点了就有反应）。
    fn mark_download_dirty(&mut self) {
        self.view_dirty = true;
        self.refresh_view();
    }

    /// 确认安装时的 `CreateRequest`（草稿 → 指令载荷）。
    ///
    /// 实例名留空就让 core 用版本号（`CreateRequest::instance_name` 的既有口径），
    /// 所以这里**不**自己拼一个默认名——两处各拼一份迟早会漂。
    fn install_request(&self, id: &str) -> CreateRequest {
        let draft = &self.download_draft;
        CreateRequest {
            name: draft.instance_name.trim().to_string(),
            version: id.to_string(),
            loader: draft.loader.kind(),
            loader_version: draft.loader_version.trim().to_string(),
        }
    }

    /// 当前草稿需要加载器版本列表、而快照里还没有时，派一次查询。
    ///
    /// 判据是 `state.loader_versions_for(...)` 的 `None`（= 没查过）：查过的（成功或失败）都不再重复派，
    /// 失败要重试走 `RefreshSources` 那条路，页面上的红色原因行不会因为一次点击就消失。
    fn ensure_loader_versions(&mut self) {
        let loader = self.download_draft.loader;
        if loader == pages::Loader::Vanilla {
            return;
        }
        let Some(version) = self.download_draft.selected.clone() else {
            return;
        };
        // 静态就不支持的组合不联网（否则只会换来一条「取不到」的假原因；真正的原因是
        // 「这个加载器没有这个版本的构建」，页面已经用静态规则说清了）。
        let kind = self
            .state
            .versions
            .iter()
            .find(|summary| summary.id == version)
            .map(|summary| summary.kind);
        if let Some(kind) = kind {
            if loader.supports(&version, kind).is_err() {
                return;
            }
        }
        if self
            .state
            .loader_versions_for(loader.kind(), &version)
            .is_some()
        {
            return;
        }
        self.dispatch(AppCommand::ListLoaderVersions {
            kind: loader.kind(),
            mc_version: version,
        });
    }

    /// 搜索词变了：写进快照 → 重建视图 → **把纯视图态捡回来**。
    ///
    /// `refresh_view()` 整块换掉 `self.view`，而它只把 `category` 与**安装草稿**带回去（`app/state.rs`
    /// 归 M105/M106）。所以「折叠状态」与「选中的主版本桶」必须在重建前后自己搬一次——否则用户敲第一个
    /// 字符，展开着的类型栏就自己收起来、选中的主版本也跳回「全部」。
    /// 「搜索时侧栏保留当前位置」这条要求，落点就是这里的四行。
    fn rebuild_after_query(&mut self, query: String) {
        self.state.download_query = query;
        let major = self.view.download.selected_major.take();
        let folded = self.view.download.type_rail_folded;
        self.view_dirty = true;
        self.refresh_view();
        // 回填要**校验**：桶是按当前行聚合法出来的，新桶列表里没有的那个选择不该被复活
        // （复审 P2-1(a)：否则会出现「侧栏没有任何一项点亮」的幽灵选中）。
        self.view.download.selected_major = major.filter(|key| {
            self.view
                .download
                .buckets
                .iter()
                .any(|bucket| bucket.key.as_deref() == Some(key.as_str()))
        });
        self.view.download.type_rail_folded = folded;
    }
}

#[cfg(test)]
mod tests {
    //! 搜索链路的**端到端**断言（复审 P2-5）。
    //!
    //! 为什么非要在 app 层写：这条链是 `Message → on_page → on_download → AppState.download_query
    //! → refresh_view → from_snapshot 过滤`，中间任何一环断掉，页面层测试都看不见（页面只拿到
    //! 已经过滤好的模型）。复审原话：「把 `rebuild_after_query` 第一行删掉，现有测试全绿，而搜索
    //! 变成假控件」——这几条就是补那个洞的。

    use std::cell::RefCell;
    use std::rc::Rc;

    use unlucky_presenter::dto::{AppEvent, Backend, CoreEvent, VersionKind, VersionSummary};
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use super::*;

    /// 只记账、不回事件的后端（同 `app/update/download_wiring.rs`）：命令出了 UI 层，状态一个字节
    /// 都不许变——否则「本地乐观回显」与「事件回流」终态相同，断言会假绿。
    struct Recorder(Rc<RefCell<Vec<AppCommand>>>);

    impl Backend for Recorder {
        fn drain_events(&mut self) -> Vec<AppEvent> {
            Vec::new()
        }
        fn dispatch(&mut self, command: AppCommand) {
            self.0.borrow_mut().push(command);
        }
    }

    fn version(id: &str) -> VersionSummary {
        VersionSummary {
            id: id.to_string(),
            kind: VersionKind::Release,
            released: "2026-09-20".to_string(),
            installed: false,
        }
    }

    /// 一个装好三行版本的 App（`1.21.9` / `1.20.4` / `1.20.1`）。
    fn app() -> App {
        let families = crate::theme::Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        };
        let mut app = App::with_presenter(
            crate::cli::Options::default(),
            families,
            SchemeFlavor::Light,
            Box::new(Recorder(Rc::new(RefCell::new(Vec::new())))),
            SettingsBridge::memory_only(),
        );
        app.state
            .apply(AppEvent::Core(CoreEvent::VersionListChanged(vec![
                version("1.21.9"),
                version("1.20.4"),
                version("1.20.1"),
            ])));
        app.refresh_view();
        app
    }

    /// 键入 → 快照 → 列表：一条走完的链。
    ///
    /// 抓得住：`rebuild_after_query` 不再写 `AppState.download_query`（搜索变假控件）、
    /// 或 `from_snapshot` 不再按 query 过滤（列表不跟着收窄）。
    /// 抓不住：真机上 `text_input` 的按键是否走到这里（那是组件的事，`search_bar` 已接 `on_input`）。
    #[test]
    fn typing_a_query_reaches_the_state_and_narrows_the_list() {
        let mut app = app();
        assert_eq!(app.view.download.rows.len(), 3, "初始是全量");

        let _ = app.on_page(pages::Message::SetVersionQuery("1.20".to_string()));

        assert_eq!(app.state.download_query, "1.20", "查询词落在快照里");
        assert_eq!(app.view.download.rows.len(), 2, "列表收窄到两行");
        assert_eq!(app.view.download.hits, 2);
        assert_eq!(app.view.download.query, "1.20", "模型回显查询词");

        // 清空回到全量（`ClearVersionQuery` 那一支）
        let _ = app.on_page(pages::Message::ClearVersionQuery);
        assert!(app.state.download_query.is_empty());
        assert_eq!(app.view.download.rows.len(), 3);
    }

    /// 查询词**跨事件存活**：再来一次重建，列表仍是收窄的。
    ///
    /// 抓得住：把查询词只写在视图模型里（下一次 `refresh_view` 就丢）——这是「搜索看起来能用、
    /// 一来事件就复位」的坏法。
    #[test]
    fn the_query_survives_a_snapshot_rebuild() {
        let mut app = app();
        let _ = app.on_page(pages::Message::SetVersionQuery("1.21".to_string()));
        assert_eq!(app.view.download.rows.len(), 1);

        // 安装进度这类事件回来时，`pump()` 会重建视图：查询词必须还在
        app.state
            .apply(AppEvent::Core(CoreEvent::InstallProgress(None)));
        app.refresh_view();

        assert_eq!(app.state.download_query, "1.21", "查询词在快照里，重建不丢");
        assert_eq!(app.view.download.rows.len(), 1, "列表仍收窄");
    }

    /// **搜索期间点主版本桶 = 退出搜索并按该桶过滤**（复审 r2 P2-1(b)）。
    ///
    /// 抓得住：`SelectMajor` 只改选择不清查询 —— 那时搜索让第二级失效（P2-1 的一致性放宽），
    /// 用户点了当场没反应、要等清空搜索那次点击才生效，就是「可点但无反应」的活控件。
    /// 抓不住：真机点击是否走到这里（`ListItem`/`Chip` 的 `on_press` 是组件既有实现）。
    #[test]
    fn clicking_a_bucket_during_a_search_leaves_the_search() {
        let mut app = app();
        let _ = app.on_page(pages::Message::SetVersionQuery("1.20".to_string()));
        assert_eq!(app.view.download.rows.len(), 2, "此刻在搜 1.20");

        // 点「1.21」这个桶
        let _ = app.on_page(pages::Message::SelectMajor(Some("1.21".to_string())));

        assert!(app.state.download_query.is_empty(), "点桶顺带退出搜索");
        assert!(!app.view.download.has_query());
        assert_eq!(
            app.view.download.selected_major.as_deref(),
            Some("1.21"),
            "选择落在刚点的那个桶上"
        );
        assert_eq!(
            app.view.download.rows.len(),
            3,
            "查询词清掉之后模型回到全量（桶过滤发生在渲染时的 visible 里）"
        );
        assert_eq!(
            app.view
                .download
                .buckets
                .iter()
                .find(|bucket| bucket.key.as_deref() == Some("1.21"))
                .map(|bucket| bucket.count),
            Some(1),
            "1.21 桶里只有一个版本 ⇒ 列表会显示一行"
        );

        // 没有搜索词时点桶照旧只改选择（不触发多余重建）
        let _ = app.on_page(pages::Message::SelectMajor(None));
        assert_eq!(app.view.download.selected_major, None);
    }

    /// 搜索时侧栏的**位置与折叠**不许丢（`rebuild_after_query` 的三行）。
    ///
    /// 抓得住：删掉那两行回填（敲第一个字符，类型栏自己弹开、主版本跳回「全部」）。
    /// 抓不住：其它事件（安装进度）回来时也会复位 —— 那条要 `app/state.rs` 的 `refresh_view`
    /// 一起改，不在本 mission scope，已在报告里明确登记「留到 M105 之后」。
    #[test]
    fn a_query_keeps_the_rail_position_and_the_fold() {
        let mut app = app();
        app.view.download.selected_major = Some("1.20".to_string());
        app.view.download.type_rail_folded = true;

        let _ = app.on_page(pages::Message::SetVersionQuery("1.20".to_string()));

        assert_eq!(
            app.view.download.selected_major.as_deref(),
            Some("1.20"),
            "桶选择还在（生效值在搜索期间放宽，但选择本身不许被清）"
        );
        assert!(app.view.download.type_rail_folded, "折叠状态还在");

        // 选中一个**新桶列表里不存在**的桶：搜索后不许把它复活成幽灵选中（P2-1(a)）
        app.view.download.selected_major = Some("9.99".to_string());
        let _ = app.on_page(pages::Message::SetVersionQuery("1.20".to_string()));
        assert_eq!(
            app.view.download.selected_major, None,
            "新桶列表里没有 9.99 ⇒ 回填空值，而不是留一个点不亮的幽灵选中"
        );
    }
}
