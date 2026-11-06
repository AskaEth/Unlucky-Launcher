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
            InstancesFailed => ("Launch failed · {reason}", "启动失败 · {reason}"),
            InstancesStageDownload => ("Downloading", "下载中"),
            InstancesStageVerify => ("Verifying", "校验中"),
            InstancesStageLaunch => ("Starting", "启动中"),
            InstancesStageWindow => ("Waiting for window", "等待游戏窗口"),
        } }
    };
}

pub(crate) use instances_entries;
