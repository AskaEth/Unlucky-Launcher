//! 实例页文案：页头、列表列头、详情动作、元信息卡的标签与值、运行状态与启动阶段。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。
//!
//! ## 这一族在装配链的**第 2 位**（M120 钉住）
//!
//! `instances_entries!` 紧跟 `nav_entries!`，所以本族的 key 落在 `Key::ALL` 的前段、
//! `LoaderVanilla` 之前。M120 的新 key 因此**追加在本域末尾**（= `InstancesEmptyBody` 之后），
//! 既没插到别的族中间，也没为了「排在文件末尾」去重排别人的族：
//! 金表（[`crate::catalog::golden`]）是 `Key::ALL` 的逐 index 镜像，位置跟着族走。
//!
//! ## M120 之前这七条全是**死 key**
//!
//! M117 加 `InstancesNew` / `InstancesImport` / `InstancesBack` / `InstancesPlay` /
//! `InstancesLastPlayed` / `InstancesEmptyTitle` / `InstancesEmptyBody` 时，实例页并没有接它们
//! ——页面上还是英文基线字面量（`"New instance"` / `"ALL INSTANCES"` / `"Present on disk"`…）。
//! M120 一次接完，同时：
//!
//! - `InstancesEmptyBody` 的译文换成详情列真正画的那句（原来那句是「看详情」的泛说）；
//! - **`InstancesLastPlayed` 删掉**：它的形状是「上一次游玩 {when}」这种整句，而页面上是
//!   元信息卡（标签 `LAST PLAYED` + 值 `2026-10-29 19:22 UTC`，或值 `Never`）。留着它只会
//!   多出一条没人用的 key，所以拆成 [`Key::InstancesMetaLastPlayed`]（标签）与
//!   [`Key::InstancesNeverPlayed`]（值）。
//!
//! ## 两条口径（复审会盯）
//!
//! 1. **品牌与技术标识不翻译**：`Java`（元信息标签的值域）与 `pid`（进程号）原样保留两套语言；
//! 2. **数字与分隔符不随语言变形**：[`Key::InstancesRunning`] 的中英都写成
//!    `… · pid {pid}`，只有前半句换了；`{pid}` 之外一个字都不许动（`pages/instances/tests/i18n.rs`
//!    用「中英同位置取占位符 + 数字原样」的断言钉这条）。

macro_rules! instances_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 实例页（M117 的七条：其中 `InstancesLastPlayed` 见模块文档，M120 删除）
            InstancesNew => ("New instance", "新建实例"),
            InstancesImport => ("Import", "导入"),
            InstancesBack => ("Back to instances", "返回实例列表"),
            InstancesPlay => ("Play", "启动"),
            InstancesEmptyTitle => ("No instance selected", "未选择实例"),
            InstancesEmptyBody => ("Pick an instance on the left to see its version, loader and launch options.", "在左侧选择一个实例，查看它的版本、加载器与启动选项。"),
            // ── 页头与列表列头（M120）
            InstancesTitle => ("Instances", "实例"),
            InstancesSubtitle => ("Local versions and their mod loaders", "本地版本与它们的模组加载器"),
            InstancesAllTitle => ("ALL INSTANCES", "全部实例"),
            // ── 详情动作（M120）
            InstancesStop => ("Stop", "停止"),
            InstancesDuplicate => ("Duplicate", "复制"),
            InstancesOpenFolder => ("Open folder", "打开文件夹"),
            InstancesDelete => ("Delete", "删除"),
            InstancesNarrowNote => (
                "Below 840 px the list and the detail share one column, so opening an instance replaces this screen (full-page switch). Widen the window to see both panes again.",
                "窗口窄于 840 像素时，列表与详情共用一列：打开一个实例会整页替换这一屏（整页切换）。把窗口拉宽就能重新同时看见两栏。"
            ),
            // ── 元信息卡的标签（M120；`Java` 是品牌名，两套语言都写 Java）
            InstancesMetaVersion => ("VERSION", "版本"),
            InstancesMetaLoader => ("LOADER", "加载器"),
            InstancesMetaMemory => ("MEMORY", "内存"),
            InstancesMetaJava => ("JAVA", "Java"),
            InstancesMetaMods => ("MODS", "模组"),
            InstancesMetaIsolation => ("ISOLATION", "隔离方式"),
            InstancesMetaVersionId => ("VERSION ID", "版本标识"),
            InstancesMetaFiles => ("FILES", "文件"),
            InstancesMetaLastPlayed => ("LAST PLAYED", "上次游玩"),
            // ── 元信息卡的值（M120）
            InstancesPresentOnDisk => ("Present on disk", "已在磁盘上"),
            InstancesMissing => ("Missing", "缺失"),
            InstancesNeverPlayed => ("Never", "从未游玩"),
            // 隔离档位的两个稳定 id（`core::game::IsolationMode::as_str`）→ 人话。
            // 白名单只覆盖真实存在的这两档；`inherit` / `custom` / 未知值原样上屏（不猜）。
            InstancesIsolationIsolated => ("Isolated", "独立目录"),
            InstancesIsolationShared => ("Shared", "共用目录"),
            // ── 运行状态与启动阶段（M120；数字与分隔符不随语言变形）
            InstancesRunning => ("Running · pid {pid}", "运行中 · pid {pid}"),
            // 失败状态行的**框架**：`{reason}` 是「哪一步」或「结局」的短句
            // （M161 复审 P1-1：「起来了又崩」与「没起来」必须能说出不同的话）。
            InstancesFailed => ("Launch failed · {reason}", "启动失败 · {reason}"),
            InstancesCrashed => ("the game crashed (exit {code})", "游戏崩溃（退出码 {code}）"),
            // 单行列表行也要放得下（`Launch failed · ` + 这一句 ≤ 行宽），
            // 「exit 0 却没起来」这一条信息必须留（复审 P1-1 第 ③ 条）。
            InstancesSilentFailed => ("exit 0, but never started", "退出码 0，没起来"),
            InstancesStageDownload => ("Downloading", "下载中"),
            InstancesStageVerify => ("Verifying", "校验中"),
            InstancesStageLaunch => ("Starting", "启动中"),
            InstancesStageWindow => ("Waiting for window", "等待游戏窗口"),
            // ── 启动参数区（M157）：实例配置页的五项 + 三态后端 + 门槛提示
            InstancesLaunchTitle => ("LAUNCH OPTIONS", "启动参数"),
            InstancesLaunchJvm => ("JVM arguments", "JVM 参数"),
            InstancesLaunchJvmHint => ("Space separated, quotes allowed. Empty = follow the global extra JVM arguments (both are sent; on duplicates the instance wins).", "空格分隔、支持引号。留空 = 跟随全局附加 JVM 参数（两者叠加下发，同名选项实例赢）。"),
            InstancesLaunchGame => ("Game arguments", "游戏参数"),
            InstancesLaunchGameHint => ("Appended last, so they override earlier game arguments. Empty = nothing appended.", "追加在命令行最后，可覆盖前面的游戏参数。留空 = 不追加。"),
            InstancesLaunchMemory => ("Max memory (MiB)", "最大内存（MiB）"),
            InstancesLaunchMemoryHint => ("Empty = follow the global default ({default} MiB).", "留空 = 跟随全局默认（{default} MiB）。"),
            InstancesLaunchJava => ("Java executable", "Java 可执行文件"),
            InstancesLaunchJavaHint => ("Empty = follow the global setting, then auto-pick (managed > JAVA_HOME > PATH).", "留空 = 跟随全局设置，再退到自动挑选（托管 > 环境变量 > 系统路径）。"),
            InstancesLaunchBackend => ("Graphics backend", "图形后端"),
            InstancesLaunchBackendGated => ("Requires MC {min}+ (this instance: {version}). The flag is not sent on this version.", "需游戏版本 {min} 及以上（本实例 {version}），这个版本不会下发该参数。"),
            InstancesLaunchEffective => ("Effective: {value}", "生效：{value}"),
            InstancesBackendDefault => ("Default", "默认"),
            InstancesBackendOpengl => ("OpenGL", "OpenGL 后端"),
            InstancesBackendVulkan => ("Vulkan", "Vulkan 后端"),
            InstancesLaunchSave => ("Save", "保存"),
            InstancesLaunchReset => ("Reset to global", "恢复跟随全局"),

            // ── 导入已有版本（M223）：向导的全部文案
            AdoptTitle => ("Import an existing version", "导入已有版本"),
            AdoptSubtitle => ("Register versions another launcher already installed. Nothing is copied: versions, libraries and assets stay where they are.", "把别的启动器已经装好的版本登记成实例。不复制任何东西：versions / libraries / assets 原地不动。"),
            AdoptScanning => ("Scanning versions/…", "正在扫描 versions/…"),
            AdoptEmpty => ("No version found under {dir}. Install one here, or point the game folder at the .minecraft another launcher uses.", "在 {dir} 下一个版本都没找到。可以用本启动器装一个，或者把游戏目录指向别的启动器用的那个 .minecraft。"),
            AdoptSelectLabel => ("Pick a version", "选择版本"),
            AdoptReady => ("Ready", "可导入"),
            AdoptBlocked => ("{count} problem(s)", "{count} 个问题"),
            AdoptRegistered => ("Already used by {name}", "已被 {name} 使用"),
            AdoptTargetLabel => ("Where should the game run?", "游戏在哪个目录里跑"),
            AdoptTargetInstanceDir => ("New folder in ULL data (default)", "ULL 数据区新建目录（缺省）"),
            AdoptTargetInstanceDirHint => ("mods, saves and config stay out of the other launcher's .minecraft", "mods / saves / config 不会落进别人的 .minecraft"),
            AdoptTargetShared => ("Share the game folder", "共用游戏目录"),
            AdoptTargetSharedHint => ("mods, saves and config are written into the other launcher's .minecraft", "mods / saves / config 会写进别人的 .minecraft"),
            AdoptTargetVersionDir => ("Use the version folder", "用版本目录"),
            AdoptTargetVersionDirHint => ("player data grows inside versions/<id>", "玩家数据会长在 versions/<id> 里"),
            AdoptNameLabel => ("Instance name (empty = version id)", "实例名（留空 = 版本 id）"),
            AdoptPreview => ("Check", "预检"),
            AdoptConfirm => ("Import", "导入"),
            AdoptBack => ("Back", "返回"),
            AdoptClose => ("Close", "关闭"),
            AdoptPlanTitle => ("Ready to import", "可以导入"),
            AdoptPlanRunDir => ("Game folder", "运行目录"),
            AdoptPlanIsolation => ("Isolation", "隔离方式"),
            AdoptFailureTitle => ("Import failed", "导入失败"),
            AdoptProblemCount => ("Every reason is listed below:", "全部原因如下："),
            AdoptInstanceName => ("Instance name", "实例名"),
            AdoptDoneTitle => ("Imported", "已导入"),
            AdoptDoneHint => ("It is in the instance list now. Player data will live in the game folder below.", "它已经出现在实例列表里；玩家数据会落在下面这个运行目录。"),
            // ── MODS 区（M224）：本地 mod 列表 + 启停开关。
            // `mods` 是**目录名**（文件系统事实，不翻译），所以中文那栏里也出现它；
            // 页面守门的 ALLOWED 里因此有一条 `mods`（见 `pages/instances/tests/i18n.rs`）。
            InstancesModsTitle => ("MODS", "模组"),
            InstancesModsCount => ("{enabled} enabled · {disabled} disabled", "{enabled} 启用 · {disabled} 禁用"),
            InstancesModsLoading => ("Reading mods/…", "正在读取 mods/…"),
            InstancesModsEmpty => ("No mods here yet — put jars into mods/ and they show up.", "这里还没有模组——把模组文件放进 mods/ 就会出现在这里。"),
            InstancesModsMissingDir => ("This instance has no mods/ directory yet.", "这个实例还没有 mods/ 目录。"),
            InstancesModsRunning => ("The game is running — toggles are read-only until it exits.", "游戏正在运行——退出前开关只读。"),
            InstancesModsOldBadge => ("legacy file", "旧版本残留"),
            InstancesModsSkipped => ("{count} entries skipped", "跳过 {count} 项"),
            InstancesModsFailed => ("Could not read mods/: {reason}", "读取 mods/ 失败：{reason}"),
            InstancesModsToggleFailed => ("Could not toggle {file}: {reason}", "启停 {file} 失败：{reason}"),
            // 启停失败的**具名原因**（`ModFailureKind` → 一句话；`{detail}` 是 OS 原文，只进括号）
            InstancesModsReasonRunning => ("the game is running", "游戏正在运行"),
            InstancesModsReasonInstance => ("the instance is gone", "实例已不存在"),
            InstancesModsReasonName => ("the file name is not usable", "文件名不可用"),
            InstancesModsReasonDirectory => ("there is no usable mods/ directory", "没有可用的 mods/ 目录"),
            InstancesModsReasonMissing => ("the file is gone", "文件已不在"),
            InstancesModsReasonNotFile => ("that is not a regular file", "那不是普通文件"),
            InstancesModsReasonCollision => ("a file with that name already exists", "同名文件已存在"),
            InstancesModsReasonRename => ("the rename failed", "改名失败"),
            // ── 存档区（M235 · STOREMVP 读路径 / M247 · SAVESOPS 写操作）。
            // M235 那条「这一版只读」（`InstancesSavesReadOnly`）**已删**：写操作接上之后它是假的，
            // 边界改由 core 的 `game::saves::write::boundary` 守（越界 / 符号链接 / 撞名都在那里拒）。
            // `saves/` / `screenshots/` 是**目录名**，两套语言都原样保留（与 MODS 区的 `mods` 同一条口径）。
            InstancesSavesTitle => ("SAVES", "存档"),
            InstancesSavesCount => ("{worlds} worlds · {shots} screenshots", "{worlds} 个世界 · {shots} 张截图"),
            InstancesSavesLoading => ("Reading saves/…", "正在读取 saves/…"),
            InstancesSavesEmpty => ("No worlds or screenshots yet — play once and they show up here.", "还没有世界或截图——玩一次就会出现在这里。"),
            InstancesSavesMissingDir => ("This instance has no saves/ or screenshots/ directory yet.", "这个实例还没有 saves/ 或 screenshots/ 目录。"),
            InstancesSavesWorlds => ("WORLDS", "世界"),
            InstancesSavesScreenshots => ("SCREENSHOTS", "截图"),
            InstancesSavesNoWorlds => ("No worlds yet.", "还没有世界。"),
            InstancesSavesNoScreenshots => ("No screenshots yet.", "还没有截图。"),
            InstancesSavesWorldIcon => ("has icon", "有图标"),
            InstancesSavesTimeUnknown => ("time unknown", "时间未知"),
            InstancesSavesSkipped => ("{count} entries skipped", "跳过 {count} 项"),
            InstancesSavesOpenFolder => ("Open saves folder", "打开存档文件夹"),
            InstancesSavesRetry => ("Retry", "重试"),
            InstancesSavesFailed => ("Could not read saves/: {reason}", "读取 saves/ 失败：{reason}"),
            InstancesSavesOpenFailed => ("Could not open the folder: {reason}", "打开文件夹失败：{reason}"),
            InstancesSavesReasonDir => ("the directory is not usable", "目录不可用"),
            InstancesSavesReasonInstance => ("the instance is gone", "实例已不存在"),
            InstancesSavesReasonOpen => ("the file manager did not start", "文件管理器没起来"),
            // ── 存档**写操作**（M247 · SAVESOPS）：重命名 / 删除（二次确认）/ 备份导出。
            // M235 那条「这一版只读」的 `InstancesSavesReadOnly` **已删**：写操作接上之后它就是假的
            // （金表同步少了那一行，不是重排）。删除确认框的三句是**不可逆动作**的兜底，不许省。
            InstancesSavesRename => ("Rename", "重命名"),
            InstancesSavesBackup => ("Back up", "备份"),
            InstancesSavesDelete => ("Delete", "删除"),
            InstancesSavesRenamePlaceholder => ("New name for this world", "这个世界的新名字"),
            InstancesSavesRenameConfirm => ("Rename", "改名"),
            InstancesSavesCancel => ("Cancel", "取消"),
            InstancesSavesBusy => ("Working…", "正在处理…"),
            InstancesSavesDeleteTitle => ("Delete this world?", "删除这个世界？"),
            InstancesSavesDeleteBody => ("{dir} and everything inside it will be deleted. This cannot be undone.", "{dir} 和它里面的全部内容都会被删除，且不能撤销。"),
            InstancesSavesDeleteConfirm => ("Delete world", "删除世界"),
            InstancesSavesRenamed => ("Renamed {from} → {to}.", "已把 {from} 改名为 {to}。"),
            InstancesSavesDeleted => ("Deleted {dir}.", "已删除 {dir}。"),
            InstancesSavesBackedUp => ("Backed up {dir} to {path} ({size}).", "已把 {dir} 备份到 {path}（{size}）。"),
            InstancesSavesBackupSkipped => ("{count} symlinks skipped", "跳过 {count} 个符号链接"),
            InstancesSavesRenameFailed => ("Could not rename {file}: {reason}", "重命名 {file} 失败：{reason}"),
            InstancesSavesDeleteFailed => ("Could not delete {file}: {reason}", "删除 {file} 失败：{reason}"),
            InstancesSavesBackupFailed => ("Could not back up {file}: {reason}", "备份 {file} 失败：{reason}"),
            InstancesSavesReasonUnsafeName => ("the name is not usable", "名字不可用"),
            InstancesSavesReasonSymlink => ("that path is a symbolic link (not followed)", "那个路径是符号链接（不跟随）"),
            InstancesSavesReasonMissing => ("the world is gone", "那个世界已不在"),
            InstancesSavesReasonNotDir => ("that is not a directory", "那不是目录"),
            InstancesSavesReasonConflict => ("a world with that name already exists", "同名世界已存在"),
            InstancesSavesReasonBoundary => ("that path is outside saves/", "那个路径落在 saves/ 之外"),
            InstancesSavesReasonTooLarge => ("the world is too large for one zip", "这个世界太大，一个 zip 装不下"),
            InstancesSavesReasonWrite => ("the write failed", "写盘失败"),
            // 实例在启动/运行中：三个写操作**主动不写盘**（不是"写盘失败"——Linux 上删一个正在跑的
            // 世界是会成功的，所以这条要直说"先退出"，别让用户以为是权限问题）。
            InstancesSavesReasonRunning => ("the game is running (exit it first)", "游戏正在运行（先退出再操作）"),
            // ── 截图**缩略图**（M251 · SAVESHOTS，D54）。
            // 四句都是「这一格为什么没有图」：没轮到（超 60 张预算）/ 太大 / 读不出 / 解不出。
            // 第 1 句与后三句分开是刻意的——"没生成"与"生成了但失败"对用户的含义不同，
            // 前者点一下刷新可能就有了，后者得用户自己去动文件。
            InstancesSavesPreviewNone => ("no preview", "未生成预览"),
            InstancesSavesPreviewTooLarge => ("too large for a preview (over 8 MiB)", "文件过大，未生成预览（超过 8 MiB）"),
            InstancesSavesPreviewUnreadable => ("the file could not be read", "读不出这张图"),
            InstancesSavesPreviewDecode => ("not a decodable image", "解不出这张图（可能已损坏或格式不支持）"),
        } }
    };
}

pub(crate) use instances_entries;
