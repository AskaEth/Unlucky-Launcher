//! 预检那一屏的夹具（M160 / M341）：`count` 条待决冲突 + **预检阶段带级别的注记**。
//!
//! 从 `demo.rs` 搬出来是守 400 行（那份文件本来就贴着线）；M341 的两段注记也在这里造，
//! 于是出图与用例看到的是同一份数据形状。

use std::path::PathBuf;

use unlucky_presenter::dto::{
    ConflictReasonView, ConflictSourceView, ImportConflictView, ImportIssueView, ImportScanView,
    PackFormatView,
};

/// 预检阶段的注记（M341）：两档都有，**照 core 真产出的那几句抄**。
///
/// - `Warning`：`pipeline::load_baseline` 读不出基线时那两句（`NOTE_BASELINE_UNREADABLE` /
///   `NOTE_BASELINE_PACK_MISMATCH`）——预检与报告搬的是同一批 `ImportIssue`，措辞就是产线那两句；
/// - `Note`：`complete::Completion::note("预检前")` 那句 `NOTE_REMOTE_COMPLETED`（按 sha512 补全了
///   几条 sha1）。**故意不用 `NOTE_REMOTE_PATH_UNKNOWN`**：那句话与这一屏本来就有的
///   `PackPickUnknownRemote`（`还有 N 条远端条目要下载后才知道路径`）说的是同一件事，
///   照抄进夹具会让截图把一笔**与 M341 无关的重复**当成新界面（已记进 mission 报告）。
///
/// 漂移风险（与 `demo.rs` 里报告注入那条注释同一条）：core 改了措辞这里不会红（跨 crate 抄写），
/// 改那句就得改这里。
pub(super) fn notes() -> Vec<ImportIssueView> {
    vec![
        ImportIssueView::warning(
            "基线读不出来，本次按「没有基线」处理（磁盘上同名文件一律算用户已有内容）：baseline.json 不是合法 JSON",
        ),
        ImportIssueView::warning(
            "基线属于另一个整合包（modrinth / Some(\"Better MC\")），本次按「没有基线」处理：磁盘上同名文件一律算用户已有内容",
        ),
        ImportIssueView::note(
            "预检前按 sha512 从 Modrinth 补全了 12 条远端条目的 sha1（补不出 4 条）",
        ),
    ]
}

/// **多注记**那一态（M345）：13 条（4 告警 + 9 注记），**总数越过收口阈值**。
///
/// 为什么常驻：M341 的「>3 条收口」只有一张临时夹具的图（`DevDocs/artifacts/m341/
/// NOT-A-STATE-pick-20-notes-temp-fixture-*.png`），跑完即弃 ⇒ 以后改这一屏没有可比对的图。
/// 这一态把它做成 `ULL_CONTENT_PACK=pick_many`，出图清单见 `DevDocs/artifacts/pickshots/`。
///
/// `.with_path(..)` 那份路径不进 DTO（`convert::issue_view` 只搬 `message`），所以同一 key 的多条
/// 在屏上是一模一样的行——夹具因此**一句只用一次**，用户不会在图上看到重复行。逐句出处：
///
/// | 句 | core 落点 | 级别 |
/// |---|---|---|
/// | 基线读不出来 | `import/pipeline.rs:231`（`NOTE_BASELINE_UNREADABLE`） | `Warning` |
/// | 基线属于另一个整合包 | `import/pipeline.rs:216`（`NOTE_BASELINE_PACK_MISMATCH`） | `Warning` |
/// | 认不出的加载器/依赖键 | `import/plan.rs:251`（`NOTE_UNKNOWN_DEPENDENCY`） | `Warning` |
/// | 清单里有多个可识别的加载器 | `import/plan.rs:260`（`NOTE_LOADER_MULTIPLE`） | `Warning` |
/// | 预检前按 sha512 补全 sha1 | `import/complete.rs:59`（`NOTE_REMOTE_COMPLETED`）+ 前缀「预检前」取自 `import/scan.rs:208` | `Note` |
/// | env.client=optional | `import/plan.rs:180`（`NOTE_OPTIONAL_FILE`） | `Note` |
/// | env.client=unsupported | `import/plan.rs:166`（`NOTE_ENV_UNSUPPORTED`） | `Note` |
/// | `.disabled` 后缀 | `import/plan/paths.rs:30`（`NOTE_DISABLED_SUFFIX`） | `Note` |
/// | `.old` 后缀 | `import/plan/paths.rs:37`（`NOTE_OLD_SUFFIX`） | `Note` |
/// | 服务端文件不解压 | `import/plan.rs:150`（`NOTE_SERVER_OVERRIDES_SKIPPED`） | `Note` |
/// | `instance.cfg` 内存不采用 | `import/plan.rs:340`（`NOTE_INSTANCE_CFG_MEMORY`） | `Note` |
/// | 同名路径被叠层遮蔽（不同内容 / 逐字节相同各一条） | `import/plan/layer.rs:82`（`NOTE_OVERRIDE_SHADOWED`） | `Note` |
///
/// 第 1 句的 `{reason}` 是**示例参数**，不是 core 的字面：core 那四种理由都**带绝对路径前缀**
/// （`addon/import/baseline.rs:126` 读失败 / `:130` 不是 UTF-8 / `:132` 形状不符 / `:135` schema 比本进程新），
/// 真机上会长成「C:\ULL\instances\survival\.minecraft\…\baseline.json 形状不符：expected value …」。
/// 夹具这里为了排版短、且不把本机路径写进常驻图，只写「baseline.json 不是合法 JSON」——**句式是
/// core 的，参数是示例**。同理「2 个可识别的加载器」「7 个文件」「12 条远端条目」也都是示例参数。
///
/// 一句 core 也产不出的「loader 库 … 缺下载地址，已降级」在 r2 按复审 P2-6 **删掉**了
/// （它只存在于 core 的测试与 M321 那个夹具里，不是产品会画的话；`NOTE_LOADER_WARNING` 只原样
/// 搬运加载器解析期的字符串，形态说得通、字面没有出处）。
///
/// **这一态是压力形态，不是「一次真导入的样子」**，两处如实登记：
///
/// 1. 第 1、2 句在真机上**互斥**（`pipeline.rs::load_baseline` 三条路只回一条），这里两条都放
///    是为了把告警段撑到「占满视口」的压力形态；
/// 2. **今日的预检入口只搬三类注记**：`import/scan.rs:217` 搬 `load_baseline` 那条、`:208` 搬
///    「预检前补全」那条、`:269` 搬 `unknown_remote` 那条 ⇒ 真机这一屏**最多 3 条**，计划期注记
///    （optional / unsupported / 后缀 / 遮蔽 / 内存 / 服务端）还没接进预检。所以这一屏的
///    「>3 条收口」今天在真机上**走不到**；这一态给的是「把计划期注记接进预检之后会是什么样子」
///    的常驻压力图（接线缺口另记 finding，不在本 mission 的 scope）。
pub(super) fn notes_many() -> Vec<ImportIssueView> {
    vec![
        ImportIssueView::warning(
            "基线读不出来，本次按「没有基线」处理（磁盘上同名文件一律算用户已有内容）：baseline.json 不是合法 JSON",
        ),
        ImportIssueView::warning(
            "基线属于另一个整合包（modrinth / Some(\"Better MC\")），本次按「没有基线」处理：磁盘上同名文件一律算用户已有内容",
        ),
        ImportIssueView::warning(
            "认不出的加载器/依赖键 \"optifine-custom\"：本启动器不安装它",
        ),
        ImportIssueView::warning(
            "清单里有 2 个可识别的加载器，只按 primary/第一条组计划：fabric-loader, quilt-loader",
        ),
        ImportIssueView::note(
            "预检前按 sha512 从 Modrinth 补全了 12 条远端条目的 sha1（补不出 4 条）",
        ),
        ImportIssueView::note("env.client=optional：可选条目照常安装（「可选」不等于「禁用」）"),
        ImportIssueView::note("env.client=unsupported：客户端不需要这个文件，跳过"),
        ImportIssueView::note("路径以 .disabled 结尾：按禁用态安装，后缀原样保留"),
        ImportIssueView::note("路径以 .old 结尾：识别并原样保留（不清除、不改名）"),
        ImportIssueView::note(
            "server-overrides/ 里的 7 个文件是服务端内容，客户端导入不解压（Modrinth 官方语义）",
        ),
        ImportIssueView::note(
            "instance.cfg 写了 MaxMemAlloc=4096，但 OverrideMemory 未开：不采用",
        ),
        ImportIssueView::note(
            "overrides/config/sodium-options.json 与 client-overrides/config/sodium-options.json 声明同一实例路径：叠层顺序 overrides/ 在前、client-overrides/ 在后（Modrinth 官方客户端语义），两份内容不同，以这一条为准",
        ),
        ImportIssueView::note(
            "overrides/config/jei/jei-client.ini 与 client-overrides/config/jei/jei-client.ini 声明同一实例路径：叠层顺序 overrides/ 在前、client-overrides/ 在后（Modrinth 官方客户端语义），两份内容逐字节相同，只落地一次",
        ),
    ]
}

/// 一条待决冲突（M160 预览清单的一行）。
pub(super) fn pending(index: usize) -> ImportConflictView {
    // 前几条用真实形态的路径，其余按序号生成：截图里既要有可读的真实感，也要有"很多条"的证据。
    let path = match index {
        0 => "config/sodium-options.json".to_string(),
        1 => "mods/create-1.2.3.jar".to_string(),
        2 => "config/jei/jei-client.ini".to_string(),
        3 => "options.txt".to_string(),
        _ => format!("config/generated/module-{index:03}.toml"),
    };
    ImportConflictView {
        path,
        source: if index.is_multiple_of(3) {
            ConflictSourceView::ClientOverrides
        } else {
            ConflictSourceView::Overrides
        },
        reason: if index.is_multiple_of(2) {
            ConflictReasonView::PreExisting
        } else {
            ConflictReasonView::UserModified
        },
        disabled: false,
        pack_sha1: "9f2c41ab77de3350aa1b8c0d5e6f708192a3b4c5".to_string(),
        pack_size: 3_800 + (index as u64 * 37) % 900,
        disk_sha1: "c1d2e3f4050617283940a5b6c7d8e9f0a1b2c3d4".to_string(),
        disk_size: 4_100 + (index as u64 * 53) % 1_200,
        base_sha1: (!index.is_multiple_of(2))
            .then(|| "3a7d90ff11bc22445566ef7788990011aabbccdd".to_string()),
    }
}

/// 预检结果（`count` 条冲突 + 给定的注记；图上靠分页只画当前这一页）。
pub(super) fn scan(count: usize, notes: Vec<ImportIssueView>) -> ImportScanView {
    let conflicts: Vec<ImportConflictView> = (0..count).map(pending).collect();
    let lost_bytes = conflicts.iter().map(|each| each.disk_size).sum();
    let incoming_bytes = conflicts.iter().map(|each| each.pack_size).sum();
    ImportScanView {
        target: PathBuf::from("C:/ULL/instances/survival/.minecraft"),
        format: Some(PackFormatView::Modrinth),
        pack_name: Some("Better MC".to_string()),
        pack_version: Some("1.2.0".to_string()),
        game_version: Some("1.20.1".to_string()),
        conflicts,
        unknown_remote: 4,
        lost_bytes,
        incoming_bytes,
        notes,
        error: None,
    }
}
