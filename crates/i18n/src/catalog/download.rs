//! 版本管理页文案（M94 建页时叫「下载页」，M108 起显示名改成「版本管理」）。
//!
//! 一句话一个 key：带变量的句子用 `{name}`，页面只做 [`crate::Catalog::render`]，不拼片段
//! （[`crate::bridge`] 的规矩 1）。
//!
//! 文案口径（M94 r1 / P2-2）：**只说这个页面真能做到的动作**（切分类、刷新、看日志），
//! 不写「稍后会自动重试」这类 UI 没有的能力，也不拿文案去补功能。
//!
//! **「下载」字样不许再指向这一页**（M108 用户令）：这一页管的是「版本与加载器」，用户点它
//! 不是为了下载点什么。唯一的例外是 [`Key::DownloadStageDownloading`]——那是安装流程里**真在
//! 传文件**的那一步，说的不是页面身份而是当时的动作，留着才对（守门见 `pages/tests.rs` 的
//! `the_download_route_is_presented_as_version_management`，它把本域每条 key 的两种语言都扫一遍）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! download_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // --- version management page (M94; renamed in M108) ---
            DownloadTitle => ("Version management", "版本管理"),
            // 例外说明（M108 复审 p3）：副标题承诺「安装、升级、修复」，但页面目前只有
            // 「选版本 → 安装」一条路，「修复」与「加载器选择」都还没有入口
            // （`InstallView::of` 连 `InstallProgressView.loader` 都没读）。
            // 这是用户令（2026-11-04）定的口径，能力由 M105 / M113 补齐；
            // 补齐之前不许拿这句话当已实现功能宣传。
            DownloadSubtitle => (
                "Manage versions and loaders: install, upgrade, repair",
                "管理版本与加载器：安装、升级、修复"
            ),
            DownloadRefresh => ("Refresh", "刷新"),
            DownloadInstall => ("Install", "安装"),
            DownloadCancel => ("Cancel install", "取消安装"),
            DownloadInstalled => ("Installed", "已安装"),
            DownloadCount => ("{count} versions", "{count} 个版本"),
            DownloadRowSupporting => ("{kind} · released {date}", "{kind} · 发布于 {date}"),
            DownloadSelectHint => ("Pick a version, then install", "选一个版本再安装"),
            DownloadLoading => ("Loading versions…", "正在加载版本列表…"),
            DownloadLoadingHint => (
                "If this stays here, press Refresh in the top right.",
                "如果一直停在这里，点右上角的刷新。"
            ),
            DownloadEmptyTitle => ("No versions to show", "没有可显示的版本"),
            DownloadEmptyBody => (
                "Pick another category, or refresh the list.",
                "换一个分类，或刷新一次列表。"
            ),
            DownloadCategoryAll => ("All", "全部"),
            DownloadCategoryRelease => ("Release", "正式版"),
            DownloadCategorySnapshot => ("Snapshot", "预览版"),
            DownloadCategoryAncient => ("Ancient", "远古版"),
            DownloadStagePlanning => ("Planning", "解析版本"),
            DownloadStageDownloading => ("Downloading files", "下载文件"),
            DownloadStageInstalling => ("Building the version", "生成版本"),
            DownloadStageRegistering => ("Registering the instance", "登记实例"),
            DownloadStageDone => ("Done", "已完成"),
            DownloadStageFailed => ("Install failed", "安装失败"),
            DownloadStageCancelled => ("Install cancelled", "安装已取消"),
            DownloadProgress => (
                "{done}/{total} files",
                "{done}/{total} 文件"
            ),
            DownloadProgressUnknown => ("Waiting for the first file…", "等第一个文件…"),
            // 速率与剩余时间是两个独立短语，各占一个 key（`DownloadRate` 只负责把它们排成一行）
            DownloadRate => ("{speed} · {eta}", "{speed}，{eta}"),
            DownloadEtaSecs => ("{secs}s left", "剩 {secs}s"),
            DownloadEtaMins => ("{mins}m{secs}s left", "剩 {mins}m{secs}s"),
            DownloadFailedHint => (
                "The Logs page has the full reason.",
                "完整原因在日志页。"
            ),
            DownloadOpenLogs => ("Open logs", "打开日志"),
            DownloadDismiss => ("Dismiss", "关闭"),
            // --- 安装选项（M105）---
            DownloadOptionsTitle => ("Install options", "安装选项"),
            DownloadOptionsLoaderVersion => ("Loader version", "加载器版本"),
            DownloadOptionsLoaderVersionsLoading => (
                "Fetching loader versions…",
                "正在获取加载器版本…"
            ),
            DownloadOptionsLoaderVersionsFailed => (
                "Could not fetch loader versions: {reason}",
                "取不到加载器版本：{reason}"
            ),
            DownloadOptionsLoaderVersionsEmpty => (
                "No loader builds published for {version}",
                "官方没有为 {version} 发布加载器版本"
            ),
            DownloadOptionsInstanceName => ("Instance name", "实例名"),
            DownloadOptionsInstanceNameHint => (
                "Empty = use the version id",
                "留空就用版本号"
            ),
            DownloadOptionsIncompatible => (
                "{loader} has no builds for {version}",
                "{loader} 没有适配 {version} 的构建"
            ),
            DownloadOptionsNeedsAtLeast => (
                "{loader} needs Minecraft {min} or newer",
                "{loader} 需要 Minecraft {min} 或更新"
            ),
            // M222：下载页只做「原版 + 加载器」一层，Forge 父链只有命令行能走。中性说明，
            // 不是错误（不禁用确认键、不套错误色）——所以它不走 `problem` 那条通道。
            DownloadOptionsLiteLoaderVanillaOnly => (
                "Vanilla parent only — Forge goes through the CLI",
                "只叠原版；叠 Forge 请走命令行"
            ),
            DownloadCancelled => (
                "Install cancelled — finished files are kept, the instance was not created",
                "安装已取消 —— 已下好的文件保留，实例未登记"
            ),
        } }
    };
}

pub(crate) use download_entries;

/// **M107 版本管理页**的侧栏 / 搜索文案。
///
/// 为什么单独一个宏、并**接在装配链最后**：`catalog/golden.rs` 是「`Key` 声明顺序 × 译文」的逐字节
/// 快照，M107 的纪律是**只追加不重排**。把新 key 插在 `download_entries` 中间会让 `Key::ALL`
/// 的顺序在 `DownloadDismiss` 与 `LogsSubtitleIdle` 之间断开，金表就得整体重排（那正是要避免的
/// 风险）。追加在链尾 ⇒ 新 key 落在 `Key::ALL` 末尾 ⇒ 金表末尾追加同样几行即可。
macro_rules! version_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // --- version management page (M107) ---
            VersionRailTitle => ("Version type", "版本类型"),
            VersionMajorTitle => ("Major version", "主版本"),
            VersionMajorAll => ("All versions", "全部版本"),
            VersionSearchPlaceholder => ("Search version numbers", "搜版本号"),
            VersionSearchClear => ("Clear search", "清空搜索"),
            VersionSearchHits => ("{count} matches", "{count} 个匹配"),
            VersionSearchEmptyTitle => ("No matching versions", "没有匹配的版本"),
            VersionSearchEmptyBody => (
                "Try a shorter piece of the number — 1.21, 24w, a1.",
                "试试更短的片段 — 1.21、24w、a1。"
            ),
            VersionSearchElsewhere => (
                "{count} versions match, but they are in another type.",
                "有 {count} 个版本匹配，但它们在别的类型里。"
            ),
            VersionSearchElsewhereAction => ("Show all types", "看全部类型"),
            // --- M107 r2：复数口径与「在别处」的标题（追加在域尾 ⇒ `Key::ALL` 顺序只增不动） ---
            VersionSearchHitsOne => ("1 match", "1 个匹配"),
            VersionSearchElsewhereTitle => ("Found in another type", "在别的类型里找到了"),
            VersionSearchElsewhereOne => (
                "1 version matches, but it is in another type.",
                "有 1 个版本匹配，但它在别的类型里。"
            ),
        } }
    };
}

pub(crate) use version_entries;
