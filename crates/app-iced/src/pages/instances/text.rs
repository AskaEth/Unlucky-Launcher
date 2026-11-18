//! 实例页的**文案入口**：页面上一处可见英文字面量都不留，全部从目录取。
//!
//! ## 为什么要有这一层（而不是在控件树里直接 `catalog.text(..)`）
//!
//! 页面要的文案有 20 多条，直接写的话每个调用点都是「key 名 + 这一处画什么」的重复劳动，
//! 而且**守门测试没法取样**：iced 0.14 的 `Element` 没有内省接口（`instances/tests.rs` 里
//! 「读源码」那条注释解释过同一件事），所以「上屏文本」只能从**页面取词的那一个入口**拿。
//! 于是所有取词收进 [`Texts`]，页面用 `texts.xxx()` 画，测试用同一个 `Texts` 断言——
//! 一处定义、两处消费，不存在「测试抄了一份平行实现」。
//!
//! ## 它抓得住什么 / 抓不住什么
//!
//! - **抓得住**：目录里某条中文忘了翻（「无 ASCII 英文单词」）、某条中英写成一样（「切换语言文本真的变」）、
//!   `Running · pid {pid}` 的数字与分隔符被翻译走样、页面里写死**英文**字面量
//!   （`every_english_literal_in_the_page_sources_is_whitelisted`，白名单判定）；
//! - **抓不住**：页面里写死的**中文**字面量（没有 ASCII 词，扫不到）、扫描范围**之外**的东西
//!   ——夹具 `instances/data.rs` 明确排除（`tests/i18n/sources.rs` 里有排除 + 存在性检查），
//!   以及本族以外的文件；译文翻得顺不顺也只能靠出图人工看（`DevDocs/artifacts/m120/`）。
//!
//! 参加者**目录派生**（M338）：`pages/instances.rs` + `pages/instances/**`（**非测试文件**），
//! 新加 / 搬进来的文件自动参加、**不用回来登记**；`app/view/instances.rs` 在 `pages/` 树外，由
//! `tests/i18n/sources.rs` 单独点名。
//!
//! ## 口径
//!
//! 品牌名不翻译（`Java` / `Fabric` / `Forge` 原样）；`pid` 是进程号的技术标识，两套语言都写
//! `pid`（改成「进程」会丢掉「拿它去任务管理器里搜」这条可操作性）。
//!
//! **启动阶段（下载中 / 校验中 / …）不在这张表里**：它在 `app/view/instances.rs` 拼进
//! `Row.status`（「阶段 · 百分比」是一整句），与元信息卡同一层——页面只画模型给的字符串。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::ModFailureKind;

/// 存档 / 截图一族的文案（M235 读路径 + M247 写操作 + M251 缩略图）
mod saves;

/// 实例页 + 详情列的文案（一个 `Catalog` 的薄包装，`Copy`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Texts(Catalog);

impl Texts {
    /// 从目录取一套（调用方给 `model.lang.catalog()`）。
    pub(super) fn of(catalog: Catalog) -> Self {
        Self(catalog)
    }

    // ------------------------------------------------------------- 页头
    pub(super) fn title(self) -> &'static str {
        self.text(Key::InstancesTitle)
    }
    pub(super) fn subtitle(self) -> &'static str {
        self.text(Key::InstancesSubtitle)
    }
    pub(super) fn new_instance(self) -> &'static str {
        self.text(Key::InstancesNew)
    }
    pub(super) fn import(self) -> &'static str {
        self.text(Key::InstancesImport)
    }

    // --------------------------------------------------------------- 列表列
    pub(super) fn all_title(self) -> &'static str {
        self.text(Key::InstancesAllTitle)
    }

    // --------------------------------------------------------------- 详情列
    pub(super) fn back(self) -> &'static str {
        self.text(Key::InstancesBack)
    }
    pub(super) fn play(self) -> &'static str {
        self.text(Key::InstancesPlay)
    }
    pub(super) fn stop(self) -> &'static str {
        self.text(Key::InstancesStop)
    }
    pub(super) fn duplicate(self) -> &'static str {
        self.text(Key::InstancesDuplicate)
    }
    pub(super) fn open_folder(self) -> &'static str {
        self.text(Key::InstancesOpenFolder)
    }
    pub(super) fn delete(self) -> &'static str {
        self.text(Key::InstancesDelete)
    }
    pub(super) fn empty_title(self) -> &'static str {
        self.text(Key::InstancesEmptyTitle)
    }
    pub(super) fn empty_body(self) -> &'static str {
        self.text(Key::InstancesEmptyBody)
    }
    /// 一个实例都没有时的标题（与「有实例但没选」分开，见目录里 M273 那条注释）。
    pub(super) fn empty_none_title(self) -> &'static str {
        self.text(Key::InstancesEmptyNoneTitle)
    }
    /// 一个实例都没有时的说明。
    pub(super) fn empty_none_body(self) -> &'static str {
        self.text(Key::InstancesEmptyNoneBody)
    }
    /// 空态的主行动：「导入已有版本」。
    pub(super) fn adopt_cta(self) -> &'static str {
        self.text(Key::InstancesAdoptCta)
    }
    /// 首屏占位（M302）：首个实例快照还没到。
    ///
    /// 与 [`Self::empty_none_title`] 是**两句话**：那句是「确实一个都没有」，这句是「还在读盘」。
    pub(super) fn scanning(self) -> &'static str {
        self.text(Key::InstancesScanning)
    }
    /// 首屏占位（M302）：扫描超时（线程没回来），配一个可点的重试。
    pub(super) fn scan_stalled(self) -> &'static str {
        self.text(Key::InstancesScanStalled)
    }
    /// 超时那支的主行动（重新扫一遍实例目录）。
    pub(super) fn scan_retry(self) -> &'static str {
        self.text(Key::InstancesScanRetry)
    }
    /// 自动发现实例的运行目录规则（详情页常驻的一句话）。
    pub(super) fn discovered_note(self) -> &'static str {
        self.text(Key::InstancesDiscoveredNote)
    }
    /// 自动发现实例**这一次**按哪一边跑（启动时弹的那一句）。
    ///
    /// 判据只有 `isolation` 这一档：`shared` = 共用游戏根，其余（`isolated`）= 它自己的版本目录。
    pub(super) fn discovered_run(self, isolation: &str) -> &'static str {
        if isolation == "shared" {
            self.text(Key::InstancesDiscoveredRunShared)
        } else {
            self.text(Key::InstancesDiscoveredRunIsolated)
        }
    }
    pub(super) fn narrow_note(self) -> &'static str {
        self.text(Key::InstancesNarrowNote)
    }

    // ------------------------------------------------------------- 启动参数（M157）
    pub(super) fn launch_title(self) -> &'static str {
        self.text(Key::InstancesLaunchTitle)
    }
    pub(super) fn launch_jvm(self) -> &'static str {
        self.text(Key::InstancesLaunchJvm)
    }
    pub(super) fn launch_jvm_hint(self) -> &'static str {
        self.text(Key::InstancesLaunchJvmHint)
    }
    pub(super) fn launch_game(self) -> &'static str {
        self.text(Key::InstancesLaunchGame)
    }
    pub(super) fn launch_game_hint(self) -> &'static str {
        self.text(Key::InstancesLaunchGameHint)
    }
    pub(super) fn launch_memory(self) -> &'static str {
        self.text(Key::InstancesLaunchMemory)
    }
    /// 「留空 = 跟随全局默认（{default} MiB）」，`default` 是生效值（三级回落之后）。
    pub(super) fn launch_memory_hint(self, default_mb: u32) -> String {
        self.0.render(
            Key::InstancesLaunchMemoryHint,
            &[("default", &default_mb.to_string())],
        )
    }
    pub(super) fn launch_java(self) -> &'static str {
        self.text(Key::InstancesLaunchJava)
    }
    pub(super) fn launch_java_hint(self) -> &'static str {
        self.text(Key::InstancesLaunchJavaHint)
    }
    pub(super) fn launch_backend(self) -> &'static str {
        self.text(Key::InstancesLaunchBackend)
    }
    /// 「需游戏版本 {min} 及以上（本实例 {version}）」。
    ///
    /// `min` 来自 DTO（`LaunchFacts::min_version` ← core 的 `GRAPHICS_BACKEND_MIN_VERSION`），
    /// **不在目录里硬编码版本号**——否则 core 一改门槛，文案会继续骗人（r4 复审 P3-c）。
    pub(super) fn launch_backend_gated(self, version: &str, min: &str) -> String {
        self.0.render(
            Key::InstancesLaunchBackendGated,
            &[("version", version), ("min", min)],
        )
    }
    /// 「生效：vulkan」。
    pub(super) fn launch_effective(self, value: &str) -> String {
        self.0
            .render(Key::InstancesLaunchEffective, &[("value", value)])
    }
    /// 三态后端的显示名（稳定 id → 文案）；**未知 id 返回 `None`**（调用方原样上屏，不猜）。
    pub(super) fn backend_label(self, id: &str) -> Option<&'static str> {
        Some(self.text(match id {
            "default" => Key::InstancesBackendDefault,
            "opengl" => Key::InstancesBackendOpengl,
            "vulkan" => Key::InstancesBackendVulkan,
            _ => return None,
        }))
    }
    pub(super) fn launch_save(self) -> &'static str {
        self.text(Key::InstancesLaunchSave)
    }
    pub(super) fn launch_reset(self) -> &'static str {
        self.text(Key::InstancesLaunchReset)
    }

    // ------------------------------------------------------------- MODS 区（M224）
    pub(super) fn mods_title(self) -> &'static str {
        self.text(Key::InstancesModsTitle)
    }
    /// 「12 启用 · 3 禁用」。
    pub(super) fn mods_count(self, enabled: usize, disabled: usize) -> String {
        self.0.render(
            Key::InstancesModsCount,
            &[
                ("enabled", &enabled.to_string()),
                ("disabled", &disabled.to_string()),
            ],
        )
    }
    pub(super) fn mods_loading(self) -> &'static str {
        self.text(Key::InstancesModsLoading)
    }
    pub(super) fn mods_empty(self) -> &'static str {
        self.text(Key::InstancesModsEmpty)
    }
    pub(super) fn mods_missing_dir(self) -> &'static str {
        self.text(Key::InstancesModsMissingDir)
    }
    pub(super) fn mods_running(self) -> &'static str {
        self.text(Key::InstancesModsRunning)
    }
    pub(super) fn mods_old_badge(self) -> &'static str {
        self.text(Key::InstancesModsOldBadge)
    }
    /// 「跳过 3 项」。
    pub(super) fn mods_skipped(self, count: usize) -> String {
        self.0
            .render(Key::InstancesModsSkipped, &[("count", &count.to_string())])
    }
    /// 读目录失败的主句：`读取 mods/ 失败：没有可用的 mods/ 目录`。
    pub(super) fn mods_failed(self, reason: &str) -> String {
        self.0
            .render(Key::InstancesModsFailed, &[("reason", reason)])
    }
    /// 启停失败的主句：`启停 x.jar 失败：同名文件已存在`。
    pub(super) fn mods_toggle_failed(self, file: &str, reason: &str) -> String {
        self.0.render(
            Key::InstancesModsToggleFailed,
            &[("file", file), ("reason", reason)],
        )
    }
    /// 启停失败大类的短句（`ModFailureKind` → 一句话；未知取值不猜，原样上屏）。
    pub(super) fn mods_reason(self, kind: ModFailureKind) -> &'static str {
        self.text(match kind {
            ModFailureKind::Running => Key::InstancesModsReasonRunning,
            ModFailureKind::InstanceUnavailable => Key::InstancesModsReasonInstance,
            ModFailureKind::UnsafeName => Key::InstancesModsReasonName,
            ModFailureKind::ModsDirUnusable => Key::InstancesModsReasonDirectory,
            ModFailureKind::NotFound => Key::InstancesModsReasonMissing,
            ModFailureKind::NotARegularFile => Key::InstancesModsReasonNotFile,
            ModFailureKind::Collision => Key::InstancesModsReasonCollision,
            ModFailureKind::Rename => Key::InstancesModsReasonRename,
        })
    }

    fn text(self, key: Key) -> &'static str {
        self.0.text(key)
    }
    /// 这一页的全部 key（守门测试的取样口：**从 `Key::ALL` 过滤出来**，不是手抄一份清单）。
    ///
    /// 域判据是前缀 `Instances`——本族的 key 名是契约（`catalog/instances.rs` 里一条一条写着），
    /// 新加的 key 只要还在这个族里就自动进守门范围，不需要有人记得来登记。
    #[cfg(test)]
    pub(super) fn keys() -> impl Iterator<Item = Key> {
        Key::ALL
            .iter()
            .copied()
            .filter(|key| key.name().starts_with("Instances"))
    }
}
