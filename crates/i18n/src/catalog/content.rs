//! 内容库文案（M109 `CONTENTDL`）。
//!
//! 一句话一个 key：带变量的句子用 `{name}`，页面只做 [`crate::Catalog::render`]，不拼片段。
//!
//! ## 三条口径
//!
//! 1. **只说能做的事**：整合包那条写的是「要等导入管线」，不是「稍后自动支持」；
//! 2. **站名也进词表**：`Modrinth` / `CurseForge` 的**中文译文必须与英文不同**
//!    （`catalog/tests.rs` 的 `chinese_catalog_is_actually_chinese` 会红），所以中文写成「Modrinth 源」；
//! 3. **禁用态要能解释自己**：`ContentIncompatible*` / `ContentModpackNotReady` 就是禁用按钮旁边那几句话——
//!    没有它们，禁用就等于「点了没反应」。
//!
//! 域文件的形状见 `crate::catalog` 的「域装配」一节。

macro_rules! content_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // --- content library page (M109) ---
            ContentTitle => ("Content", "内容库"),
            ContentSubtitle => (
                "Modpacks, mods, resource packs and shaders",
                "整合包、Mod、资源包与光影"
            ),
            ContentSectionVersions => ("Version management", "版本管理"),
            ContentSectionContent => ("Content library", "内容库"),
            ContentKindModpack => ("Modpacks", "整合包"),
            ContentKindMod => ("Mods", "Mod"),
            ContentKindResourcepack => ("Resource packs", "资源包"),
            ContentKindShader => ("Shaders", "光影"),
            ContentSourceModrinth => ("Modrinth", "Modrinth 源"),
            ContentSourceCurseforge => ("CurseForge", "CurseForge 源"),
            ContentSortRelevance => ("Relevance", "相关度"),
            ContentSortDownloads => ("Downloads", "下载量"),
            ContentSortNewest => ("Newest", "最新发布"),
            ContentSortUpdated => ("Recently updated", "最近更新"),
            ContentSearchPlaceholder => ("Search content…", "搜索内容…"),
            ContentSearch => ("Search", "搜索"),
            ContentFilterVersion => ("Game version", "游戏版本"),
            ContentFilterLoader => ("Loader", "加载器"),
            ContentFilterAny => ("Any", "不限"),
            ContentCount => ("{count} results", "{count} 个结果"),
            ContentLoading => ("Loading content…", "正在加载内容…"),
            ContentLoadingHint => (
                "Results come from the network; nothing is cached offline yet.",
                "结果来自网络，暂时没有离线缓存。"
            ),
            ContentEmptyTitle => ("Nothing matched", "没有匹配的内容"),
            ContentEmptyBody => (
                "Try another keyword, or clear a filter.",
                "换个关键词，或清掉一个筛选条件。"
            ),
            ContentFailedTitle => ("Could not load content", "内容加载失败"),
            ContentFailedBody => ("Check the network, then retry.", "检查网络后重试。"),
            ContentRetry => ("Retry", "重试"),
            ContentLoadMore => ("Load more", "加载更多"),
            ContentNoMore => ("No more results", "没有更多了"),
            ContentDetailEmpty => (
                "Pick an item to see what it supports.",
                "选一个项目看它支持什么。"
            ),
            ContentDetailDownloads => ("{count} downloads", "下载量 {count}"),
            ContentDetailUpdated => ("Updated {date}", "更新于 {date}"),
            ContentDetailLoaders => ("Loaders: {list}", "加载器：{list}"),
            ContentDetailVersions => ("Game versions: {list}", "游戏版本：{list}"),
            ContentInstall => ("Install into instance", "装入实例"),
            ContentTargetInstance => ("Target instance", "目标实例"),
            ContentNoInstances => ("No instance to install into", "没有可装入的实例"),
            ContentIncompatibleVersion => (
                "Needs {version}; this instance runs {actual}",
                "需要 {version}，该实例运行的是 {actual}"
            ),
            ContentIncompatibleLoader => ("Not built for {loader}", "不支持 {loader}"),
            ContentModpackNotReady => (
                "Installing modpacks from the library into an instance is not available yet",
                "从内容库一键安装整合包到实例还没做（本地的包可以直接导入）"
            ),
            ContentSourceUnavailable => (
                "CurseForge needs an API key: add one in Settings",
                "CurseForge 需要在设置里配置密钥才能搜索"
            ),
            ContentSourceNotWired => (
                "Installing from CurseForge is not available yet",
                "CurseForge 来源的安装尚未开放"
            ),
            // --- M113: pack import / export（PACKUI）---
            PackTitle => ("Modpack import & export", "整合包导入 / 导出"),
            PackImportTitle => ("Import a modpack", "导入整合包"),
            PackDropHint => (
                "Drop a .mrpack, .zip or a MultiMC/Prism instance folder here",
                "把 .mrpack / .zip 或 MultiMC·Prism 实例目录拖到这里"
            ),
            PackDropActive => ("Release to probe the pack", "松手开始探测"),
            PackNoDialog => (
                "This build has no system file picker: drag the file in, or paste its path below.",
                "本版没有系统文件选择框：请把文件拖进来，或在下面粘贴路径。"
            ),
            PackPathLabel => ("Pack path", "整合包路径"),
            PackPathPlaceholder => (
                "D:\\packs\\Better MC.mrpack",
                "D:\\整合包\\更好的MC.mrpack"
            ),
            PackImportAction => ("Import", "导入"),
            PackCancel => ("Cancel import", "取消导入"),
            PackDismiss => ("Dismiss", "关闭"),
            PackTargetLabel => ("Install into", "装到实例"),
            PackTargetNew => ("New instance", "新建实例"),
            PackInstanceName => ("Instance name", "实例名"),
            PackStageLine => ("Step {index}/{total} · {stage}", "步骤 {index}/{total} · {stage}"),
            PackStageProbe => ("Reading the manifest", "读取清单"),
            PackStageExtract => ("Unpacking overrides", "解包覆盖内容"),
            PackStageFetch => ("Fetching missing files", "补下载文件"),
            PackStageCommit => ("Writing files", "落地写盘"),
            PackStageRegister => ("Registering the instance", "登记实例"),
            PackProbeTitle => (
                "This pack is almost a known format",
                "这个包差一点就是已知格式"
            ),
            PackProbeLooksLike => (
                "Looks like {format}, but missing: {missing}",
                "看起来像 {format}，但缺：{missing}"
            ),
            PackProbeUnknown => (
                "No known format claims this pack",
                "没有任何已知格式认领这个包"
            ),
            PackProbeSaw => ("Found in the pack: {saw}", "包里看到：{saw}"),
            PackProbeNothing => ("nothing recognisable", "没有可识别的东西"),
            PackStatusSuccess => ("Imported", "已导入"),
            PackStatusPartial => ("Imported with gaps", "导入完成，但有缺口"),
            PackStatusFailed => ("Import failed — rolled back", "导入失败 —— 已整包回滚"),
            PackReportWritten => ("Files written: {count} · {bytes}", "{count} 个文件已写盘 · {bytes}"),
            PackReportMissing => (
                "Could not fetch (tolerated): {count}",
                "有 {count} 个文件没搞到（可容忍）"
            ),
            PackReportConflicts => (
                "Conflicts: {count}",
                "冲突：{count} 个"
            ),
            PackReportRejected => (
                "The client does not support these: {count}",
                "这些组件客户端不支持：{count} 个"
            ),
            PackReportRejectedNote => ("not supported by the client", "客户端不支持"),
            PackCancelled => (
                "Import keeps running in the background — this result is discarded. Files already written stay.",
                "导入会在后台继续跑完 —— 本次结果作废；已经写下的文件会留下。"
            ),
            PackOverwriteAction => ("Re-import, the pack wins", "以包为准重新导入"),
            PackOverwriteNote => (
                "Keeping both (renamed) is not supported yet: the import pipeline has one global conflict policy and no per-file decision channel.",
                "「两者都留（改名）」暂不支持：导入管线只有一套全局冲突策略，没有逐文件决策通道。"
            ),
            PackMissingOffline => ("offline import", "离线导入"),
            PackMissingNoSource => (
                "no download source available yet",
                "暂时拿不到下载地址"
            ),
            PackMissingFetchFailed => ("download failed: {detail}", "下载失败：{detail}"),
            PackFatalTitle => (
                "Rolled back — nothing was left behind",
                "已回滚 —— 目标目录零残留"
            ),
            PackFatalHint => ("What to try next", "下一步可以试"),
            PackConflictUserModified => ("you changed it", "你改过"),
            PackConflictUserDeleted => ("you deleted it", "你删过"),
            PackConflictPreExisting => ("was already there", "本来就在"),
            PackChoiceKeepUser => ("kept yours", "保留我的"),
            PackChoiceOverwrite => ("pack version used", "用包里的"),
            PackConflictHashes => (
                "pack {pack} · base {base} · disk {disk}",
                "包内 {pack} · 基线 {base} · 磁盘 {disk}"
            ),
            PackConflictDetail => ("Hashes", "三方哈希"),
            PackFormatModrinth => ("Modrinth .mrpack", "Modrinth 的 .mrpack"),
            PackFormatCurseforge => ("CurseForge manifest", "CurseForge 清单"),
            PackFormatMultimc => ("MultiMC / Prism instance", "MultiMC·Prism 实例"),
            PackExportTitle => ("Export this instance", "导出这个实例"),
            PackExportOpen => ("Export…", "导出…"),
            PackExportFormat => ("Format", "格式"),
            PackExportName => ("Pack name", "包名"),
            PackExportVersion => ("Version", "版本"),
            PackExportOutput => ("Save to", "保存到"),
            PackExportOutputPlaceholder => (
                "empty = suggested path",
                "留空 = 自动给一个路径"
            ),
            PackExportPrecheck => ("Dry run", "预检"),
            PackExportConfirm => ("Write the pack", "确认写盘"),
            PackExportAbort => ("Back", "返回"),
            PackExportMultimcNote => (
                "MultiMC/Prism can only be imported, not exported (by design).",
                "MultiMC·Prism 只能导入、不能导出（有意省略）。"
            ),
            PackExportDone => ("Wrote {path} · {bytes}", "已写出 {path} · {bytes}"),
            PackExportFailed => ("Export failed: {reason}", "导出失败：{reason}"),
            PackExportPending => ("Scanning the instance…", "正在扫描实例…"),
            PackPreviewHead => (
                "Will inline {inline} files · cannot export: {skipped}",
                "将内联 {inline} 个文件 · 无法导出 {skipped} 项"
            ),
            PackPreviewRemote => (
                "{count} files can be referenced remotely",
                "{count} 个文件可走远程引用"
            ),
            PackPreviewInlineBytes => (
                "{bytes} copied into overrides/",
                "{bytes} 会原样拷进 overrides/"
            ),
            PackPreviewInlineTitle => (
                "Copied into overrides/ (first {shown}):",
                "原样拷进 overrides/（只列前 {shown} 个）："
            ),
            PackPreviewSkippedTitle => (
                "Not in the pack: {count}",
                "没进包：{count} 项"
            ),
            PackPreviewNotes => ("Known losses", "已知损失"),
            PackPreviewTarget => ("Will be written to {path}", "将写到 {path}"),
            PackPreviewMore => (
                "…and {count} more files (not shown here)",
                "…还有 {count} 个文件（这里不展开）"
            ),
            PackPreviewComplete => ("Nothing is left out", "没有遗漏任何文件"),
            PackPreviewIncomplete => (
                "Some files will not be in the pack",
                "有文件不会进包"
            ),
            PackReasonModrinthSha1 => ("Modrinth match (SHA-1)", "Modrinth 命中（SHA-1）"),
            PackReasonCurseFingerprint => (
                "CurseForge match (fingerprint)",
                "CurseForge 命中（文件指纹）"
            ),
            PackReasonNoRemoteMatch => (
                "unknown to both catalogs, inlined",
                "两个目录都不认识，已内联"
            ),
            PackReasonCurseOnly => (
                "only CurseForge knows it, this format cannot use it, inlined",
                "只有 CF 认识，目标格式用不上，已内联"
            ),
            PackReasonModrinthOnly => (
                "only Modrinth knows it, the CF manifest cannot use it, inlined",
                "只有 Modrinth 认识，CF 清单用不上，已内联"
            ),
            PackReasonMissingSha512 => (
                "the catalogue entry has no sha512 hash, inlined",
                "命中缺 sha512，已内联"
            ),
            PackReasonMissingCurseIds => (
                "the catalogue entry has no CurseForge ids, inlined",
                "命中缺 projectID/fileID，已内联"
            ),
            PackReasonDisabled => (
                "disabled file, this format cannot express it (.disabled kept)",
                "禁用文件，目标格式表达不了（保留 .disabled）"
            ),
            PackReasonSymlink => ("symbolic link, skipped", "符号链接，已跳过"),
            PackReasonUnsafePath => (
                "path fails the pack path rules, skipped",
                "路径过不了包内规整，已跳过"
            ),
            PackReasonUnreadable => ("cannot read it, skipped", "读不出来，已跳过"),
        } }
    };
}

pub(crate) use content_entries;
