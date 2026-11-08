//! 金表的**表数据**（`Key::ALL` 顺序 × (en, zh-CN) 逐字快照，第一列是 `Key::name()`）。「为什么
//! 有这张表、它抓得住什么」全在 `catalog.rs` 的 `mod golden;` 里；本文件只放数据。
pub(super) const GOLDEN: &[(&str, &str, &str)] = &[
    ("NavInstances", "Instances", "实例"),
    ("NavDownload", "Versions", "版本管理"),
    ("NavDownloadExpanded", "Version management", "版本管理"),
    ("NavLogs", "Logs", "日志"),
    ("NavSettings", "Settings", "设置"),
    ("InstancesNew", "New instance", "新建实例"),
    ("InstancesImport", "Import", "导入"),
    ("InstancesBack", "Back to instances", "返回实例列表"),
    ("InstancesPlay", "Play", "启动"),
    ("InstancesEmptyTitle", "No instance selected", "未选择实例"),
    ("InstancesEmptyBody", "Pick an instance on the left to see its version, loader and launch options.", "在左侧选择一个实例，查看它的版本、加载器与启动选项。"),
    ("InstancesTitle", "Instances", "实例"),
    ("InstancesSubtitle", "Local versions and their mod loaders", "本地版本与它们的模组加载器"),
    ("InstancesAllTitle", "ALL INSTANCES", "全部实例"),
    ("InstancesStop", "Stop", "停止"),
    ("InstancesDuplicate", "Duplicate", "复制"),
    ("InstancesOpenFolder", "Open folder", "打开文件夹"),
    ("InstancesDelete", "Delete", "删除"),
    ("InstancesNarrowNote", "Below 840 px the list and the detail share one column, so opening an instance replaces this screen (full-page switch). Widen the window to see both panes again.", "窗口窄于 840 像素时，列表与详情共用一列：打开一个实例会整页替换这一屏（整页切换）。把窗口拉宽就能重新同时看见两栏。"),
    ("InstancesMetaVersion", "VERSION", "版本"),
    ("InstancesMetaLoader", "LOADER", "加载器"),
    ("InstancesMetaMemory", "MEMORY", "内存"),
    ("InstancesMetaJava", "JAVA", "Java"),
    ("InstancesMetaMods", "MODS", "模组"),
    ("InstancesMetaIsolation", "ISOLATION", "隔离方式"),
    ("InstancesMetaVersionId", "VERSION ID", "版本标识"),
    ("InstancesMetaFiles", "FILES", "文件"),
    ("InstancesMetaLastPlayed", "LAST PLAYED", "上次游玩"),
    ("InstancesPresentOnDisk", "Present on disk", "已在磁盘上"),
    ("InstancesMissing", "Missing", "缺失"),
    ("InstancesNeverPlayed", "Never", "从未游玩"),
    ("InstancesIsolationIsolated", "Isolated", "独立目录"),
    ("InstancesIsolationShared", "Shared", "共用目录"),
    ("InstancesRunning", "Running · pid {pid}", "运行中 · pid {pid}"),
    ("InstancesFailed", "Launch failed · {reason}", "启动失败 · {reason}"),
    ("InstancesCrashed", "the game crashed (exit {code})", "游戏崩溃（退出码 {code}）"),
    ("InstancesSilentFailed", "exit 0, but never started", "退出码 0，没起来"),
    ("InstancesStageDownload", "Downloading", "下载中"),
    ("InstancesStageVerify", "Verifying", "校验中"),
    ("InstancesStageLaunch", "Starting", "启动中"),
    ("InstancesStageWindow", "Waiting for window", "等待游戏窗口"),
    ("InstancesLaunchTitle", "LAUNCH OPTIONS", "启动参数"),
    ("InstancesLaunchJvm", "JVM arguments", "JVM 参数"),
    ("InstancesLaunchJvmHint", "Space separated, quotes allowed. Empty = follow the global extra JVM arguments (both are sent; on duplicates the instance wins).", "空格分隔、支持引号。留空 = 跟随全局附加 JVM 参数（两者叠加下发，同名选项实例赢）。"),
    ("InstancesLaunchGame", "Game arguments", "游戏参数"),
    ("InstancesLaunchGameHint", "Appended last, so they override earlier game arguments. Empty = nothing appended.", "追加在命令行最后，可覆盖前面的游戏参数。留空 = 不追加。"),
    ("InstancesLaunchMemory", "Max memory (MiB)", "最大内存（MiB）"),
    ("InstancesLaunchMemoryHint", "Empty = follow the global default ({default} MiB).", "留空 = 跟随全局默认（{default} MiB）。"),
    ("InstancesLaunchJava", "Java executable", "Java 可执行文件"),
    ("InstancesLaunchJavaHint", "Empty = follow the global setting, then auto-pick (managed > JAVA_HOME > PATH).", "留空 = 跟随全局设置，再退到自动挑选（托管 > 环境变量 > 系统路径）。"),
    ("InstancesLaunchBackend", "Graphics backend", "图形后端"),
    ("InstancesLaunchBackendGated", "Requires MC {min}+ (this instance: {version}). The flag is not sent on this version.", "需游戏版本 {min} 及以上（本实例 {version}），这个版本不会下发该参数。"),
    ("InstancesLaunchEffective", "Effective: {value}", "生效：{value}"),
    ("InstancesBackendDefault", "Default", "默认"),
    ("InstancesBackendOpengl", "OpenGL", "OpenGL 后端"),
    ("InstancesBackendVulkan", "Vulkan", "Vulkan 后端"),
    ("InstancesLaunchSave", "Save", "保存"),
    ("InstancesLaunchReset", "Reset to global", "恢复跟随全局"),
];

/// 金表的**全量**行（本文件 → [`tail`] → [`tail2`]，顺序即 `Key::ALL`）。
///
/// 拆成三段只为守 400 行硬线（M157/M159 往金表追加文案后越线）；对调用方是**一条**序列，
/// `catalog/tests.rs` 的逐字节断言不用关心分段。
///
/// ⚠️ 加段时**必须**在这里也 chain 上：条数与内容都只能从这一条序列导出（见 [`len`]）。
pub(super) fn rows() -> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    GOLDEN
        .iter()
        .chain(tail::ROWS.iter())
        .chain(tail2::ROWS.iter())
}

/// 金表总行数（`Key::ALL` 的条数）。
///
/// **定义成 `rows().count()`，不是各段 `len()` 相加**：r3 复审实测过一条假绿——把
/// [`rows`] 里的 `chain(tail2::ROWS)` 删掉，`catalog/tests.rs` 仍全绿，因为条数来自一处手工
/// 聚合（仍有 tail2）而内容只遍历 `rows()`（少了 tail2），于是 213/459 行无人看守。
/// 现在条数与内容同源：漏挂一段 ⇒ 条数当场对不上 ⇒ 用例必红。
pub(super) fn len() -> usize {
    rows().count()
}

mod tail;
mod tail2;
