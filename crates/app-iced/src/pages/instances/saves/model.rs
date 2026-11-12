//! SAVES 区的**视图模型**：快照 → 页面要画的东西（M235 的行 + M251 的缩略图墙）。
//!
//! 从 `saves.rs` 拆出（M251：加完缩略图墙的模型之后那个文件会越过 400 行硬线）。
//! 拆法是「**纯搬移 + 一族新增**」：`Row` / `Editing` / `Saves` / [`from_state`] /
//! [`failure_line`] 一律逐行搬来，只把截图那一族从「一行字」换成 [`Shot`]（缩略图 + 副文案）；
//! `saves.rs` 用 `pub use model::{…}` 保住原有路径，页面与用例的 import 一行不改。
//!
//! # 缩略图的句柄为什么在这里建
//!
//! [`from_state`] 是「快照 → 模型」的**唯一**转换点，而 `iced` 的 `Handle::from_rgba` 会把像素
//! 搬进渲染器（一次上传）。渲染回调（`view()`）每帧都跑、`refresh_view` 只在事件之后跑，
//! 所以句柄只能在这里建一次：建在 `view()` 里等于每帧重传 6MB 纹理。
//!
//! 这也正是 D54「**不许在渲染回调里解码**」的同一条纪律的延伸：解码在 presenter 的后台线程，
//! 句柄在这里，`view()` 只借。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use iced::widget::image::Handle;

use unlucky_i18n::Catalog;
use unlucky_presenter::dto::{
    SavesFailureKind, SavesFailureOp, SavesFailureView, ScreenshotPreview, ScreenshotThumbKind,
    ScreenshotView,
};
use unlucky_presenter::state::SavesState;

use super::rows;
use super::Texts;

/// 列表里的一个**世界**行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 世界目录名（磁盘原样；写操作把它原样带回去当「源」）。
    pub name: String,
    /// 右侧/下方副文案（时间、图标标记，已由这一层拼好）。
    pub detail: String,
}

/// 缩略图墙上的一格（一张截图）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    /// 磁盘文件名（含扩展名）。墙上一格不印它（`160px` 放不下），但失败原因要能指认它。
    pub name: String,
    /// 格子下面的副文案：正常时是「大小 · 时间」，没有预览时是**为什么**没有。
    pub detail: String,
    /// 图本身（或占位）。
    pub preview: ShotPreview,
}

/// 一格截图里的画面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShotPreview {
    /// 缩略图（句柄在 [`from_state`] 里建好；见模块文档）。
    Ready {
        /// 已上传的 RGBA 句柄。
        handle: Handle,
    },
    /// 没有可画的像素：框里一句话（「未生成预览」/ 具名原因）。
    Missing {
        /// 短句（`160×90` 的框里放得下）。
        note: String,
    },
}

/// 正在行内重命名的那一行（来源是 presenter 的 `SavesState::draft`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editing {
    /// 哪个世界。
    pub dir: String,
    /// 输入框里的内容（原样）。
    pub text: String,
}

/// SAVES 区的展示模型（`Model::saves`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saves {
    /// 这份数据属于哪个实例（`None` = 不在详情页 / 还没请求过）。
    pub bound: Option<String>,
    /// 正在读盘（进详情页首帧：请求已发出、快照还没回来）。
    pub loading: bool,
    /// 读盘/打开/写操作失败的人读原因（app 层按 i18n 拼好）。
    pub error: Option<String>,
    /// 这一条失败能不能靠「再列一次」解决（只有读盘失败算）。
    pub retryable: bool,
    /// 手里有没有一份**快照**（`false` = 只拿到了失败，"还没有世界"那几句不许画）。
    pub has_view: bool,
    /// 两个目录都不存在（新实例的常态：还没进过游戏）。
    pub missing: bool,
    /// 世界行。
    pub worlds: Vec<Row>,
    /// 截图墙（新的在前，顺序由 core 给）。
    pub screenshots: Vec<Shot>,
    /// 计数行（`3 个世界 · 12 张截图`）：**在这一层算好**，页面只借 `&str`。
    ///
    /// 为什么不留到页面里 `format!`：iced 的 `Element` 只借这一帧的数据，页面函数返回的是
    /// **借用局部变量的树**，局部 `String` 一出栈就悬空（`E0515`）。与 [`super::Row`]
    /// 之外的 `Mods::count` 同一条做法。
    pub count: String,
    /// 跳过条目数（符号链接 / 非图片 / 读不出）。
    pub skipped: usize,
    /// 跳过提示（`跳过 3 项`）。
    pub skipped_note: String,
    /// 正在行内重命名的那一行（`None` = 没有在改名）。
    pub editing: Option<Editing>,
    /// 有写操作在飞的那个世界（`None` = 空闲）。
    pub busy: Option<String>,
    /// 上一次写操作成功的那句话（`None` = 这一拍没有回执）。
    pub notice: Option<String>,
}

impl Saves {
    /// 有没有拿到一份快照（成功或失败都算；`loading` 中不算）。
    #[must_use]
    pub fn loaded(&self) -> bool {
        self.bound.is_some() && !self.loading
    }

    /// 两个目录都在、但里面什么都没有（**必须先有一份快照**：纯失败态不是"这里空的"）。
    #[must_use]
    pub fn nothing_yet(&self) -> bool {
        self.has_view && !self.missing && self.worlds.is_empty() && self.screenshots.is_empty()
    }

    /// 这个世界的写操作在飞吗（那一行画「正在处理…」并把按钮拿掉）。
    #[must_use]
    pub fn busy_with(&self, dir: &str) -> bool {
        self.busy.as_deref() == Some(dir)
    }

    /// 截图墙里有多少张**真出图**了（出图与用例的抓手：`0` 就是一张都没解出来）。
    #[must_use]
    pub fn thumbnails(&self) -> usize {
        self.screenshots
            .iter()
            .filter(|shot| matches!(shot.preview, ShotPreview::Ready { .. }))
            .count()
    }
}

/// presenter 的视图态 → 页面要画的东西（**唯一**的转换点，`app/view/instances.rs` 调）。
///
/// 三条口径：
///
/// 1. **失败不清列表**：list 失败时保留上一份 `worlds`/`screenshots`（用户还能看见旧列表，
///    只是多一条红字），只有"从来没有过快照"时才是一条纯错误态；
/// 2. **不做乐观更新**：行上的名字只来自快照，写操作的"在飞"只用来置灰那一行；
/// 3. **回执只画一次**：`view.last_op` 是 host 写在快照里的，下一次普通 List 就没了。
#[must_use]
pub fn from_state(state: &SavesState, catalog: Catalog) -> Saves {
    let texts = Texts::of(catalog);
    let mut out = Saves {
        bound: state.bound.clone(),
        loading: state.bound.is_some() && state.view.is_none() && state.failed.is_none(),
        editing: state.draft.as_ref().map(|draft| Editing {
            dir: draft.dir.clone(),
            text: draft.text.clone(),
        }),
        busy: state.pending.as_ref().map(|op| op.dir().to_string()),
        ..Saves::default()
    };
    if let Some(view) = &state.view {
        out.has_view = true;
        out.missing = !view.exists && !view.screenshots_exists;
        // 计数直接问快照（`view.world_count()`），不从刚装好的 `out.worlds` 再数一遍——
        // M224 出图实测过"先算计数后装列表"画出 `0 启用 · 0 禁用` 的那种顺序错，这里从根上绕开。
        out.worlds = view
            .worlds
            .iter()
            .map(|world| Row {
                name: world.dir.clone(),
                detail: rows::world_detail(world.icon, world.modified_ms, texts),
            })
            .collect();
        out.screenshots = view
            .screenshots
            .iter()
            .map(|shot| shot_cell(&view.screenshots_dir, shot, texts))
            .collect();
        out.count = texts.saves_count(view.world_count(), view.screenshot_count());
        out.skipped = view.skipped_count();
        out.skipped_note = texts.saves_skipped(out.skipped);
        out.notice = view.last_op.as_ref().map(|op| rows::op_line(op, texts));
    }
    if let Some(failure) = &state.failed {
        out.loading = false;
        out.error = Some(failure_line(failure, texts));
        // 只有「读盘 / 列目录」失败才值得重试；写操作的重试就是再点一次那个按钮。
        out.retryable = failure.op.is_none() && failure.kind != SavesFailureKind::OpenFailed;
    }
    out
}

/// 句柄缓存的条目上限（超过就整表清空）。
///
/// 一张 160×160 的 RGBA 最坏 ≈ 100KB ⇒ 120 条 ≈ **12MB 上限**；正常一屏 ≤60 张（≈6MB）。
/// 与 presenter 的 `MAX_CACHE_ENTRIES`（也是 120）同量级：两层各自有天花板，不叠成无界。
/// 清空而不是 LRU：代价是下一次 `refresh_view` 重传一遍（罕见），收益是这段逻辑没有顺序状态。
const MAX_HANDLES: usize = 120;

/// 句柄缓存：`(截图目录, 文件名, mtime, 字节数) → Handle`。
///
/// ## 为什么必须缓存（而不是每次 `from_state` 建一个新的）
///
/// `Handle::from_rgba` 每次都生成一个**新 id**，而 `iced` 的渲染器按 id 缓存纹理：id 一变就
/// 重新上传一遍像素。`from_state` 挂在 `refresh_view` 上，而 `refresh_view` 每个 UI 事件都跑
/// （点一下、打一个字）——一屏 60 张就是「每次事件重传 ≈6MB」。M251 复审要求把这条成本消掉：
/// 缓存之后同一张图永远是同一个 id，纹理只上传一次。
///
/// ## 键为什么与 presenter 的缓存键同源
///
/// presenter 那边是 `(完整路径, mtime, 字节数)`（`host::saves::thumbs`），这里换成
/// `(目录, 文件, mtime, 字节数)`——同一组事实，只是目录单独放一格，免得每次拼长串。
/// 任一字段变了（文件被覆盖）就是另一张图 ⇒ 新 id ⇒ 重传，这也正是我们要的。
fn handles() -> &'static Mutex<HashMap<String, Handle>> {
    static HANDLES: OnceLock<Mutex<HashMap<String, Handle>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 句柄键的分隔符（`0x01`：文件名里不可能出现，也避免 `<a|b>` 与 `<ab|>` 撞键）。
///
/// 用 `push` 拼而不是 `format!`：`format!` 的模板串里带 `dir`/`u` 这类 ASCII 词，会被
/// `pages/instances/tests/i18n.rs` 的「页面里不许写死英文文案」守门当成界面字面量抓走。
const KEY_SEP: char = '\u{1}';

/// 取这张截图的句柄：命中就复用（id 不变 ⇒ 不重传），未命中才建一次。
fn handle_for(
    dir: &str,
    shot: &ScreenshotView,
    thumb: &unlucky_presenter::dto::ThumbnailView,
) -> Handle {
    let mut key = String::with_capacity(dir.len() + shot.file.len() + 24);
    key.push_str(dir);
    key.push(KEY_SEP);
    key.push_str(&shot.file);
    key.push(KEY_SEP);
    match shot.modified_ms {
        Some(ms) => key.push_str(&ms.to_string()),
        // 取不到时间：占一个不含数字的记号（不会与任何毫秒数撞上）。
        None => key.push('-'),
    }
    key.push(KEY_SEP);
    key.push_str(&shot.bytes.to_string());
    let mut store = handles()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(cached) = store.get(&key) {
        return cached.clone();
    }
    let handle = Handle::from_rgba(thumb.width, thumb.height, thumb.pixels.clone());
    if store.len() >= MAX_HANDLES {
        store.clear();
    }
    store.insert(key, handle.clone());
    handle
}

/// 清空句柄缓存（**只给用例**：这是进程级 memo，用例之间必须能从已知状态起跑）。
#[cfg(test)]
pub(in crate::pages::instances) fn clear_handles() {
    handles()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}

/// 一张截图 → 墙上的一格（缩略图句柄 / 占位 + 副文案）。
///
/// **三种状态各有各的话**（D54 第 6 条：失败逐条可见）：
///
/// | 快照里的状态 | 框里 | 副文案 |
/// |---|---|---|
/// | `Ready`（像素自洽） | 真图 | `大小 · 时间` |
/// | `Ready`（像素长度不对） | 「解不出这张图」 | `大小 · 时间`（防御性：壳层不该拿到坏像素） |
/// | `NotGenerated` | 「未生成预览」 | `大小 · 时间`（超预算没轮到） |
/// | `Failed` | 具名原因（太大 / 读不出 / 解不出） | `大小 · 时间`（原文留在 DTO 里，不上墙） |
fn shot_cell(dir: &str, shot: &ScreenshotView, texts: Texts) -> Shot {
    let stamp = rows::shot_detail(shot.bytes, shot.modified_ms, texts);
    match &shot.preview {
        ScreenshotPreview::Ready(thumb) if thumb.is_consistent() => Shot {
            name: shot.file.clone(),
            detail: stamp,
            // 句柄按 (目录, 文件, mtime, 大小) 复用：同一张图在后续每一次 `refresh_view` 里
            // 拿到的都是**同一个 id** ⇒ 渲染器不会重传纹理（见 [`handle_for`]）。
            preview: ShotPreview::Ready {
                handle: handle_for(dir, shot, thumb),
            },
        },
        // 像素长度与宽高对不上：壳层不该拿到这种数据，但拿到了也不能画成花屏。
        ScreenshotPreview::Ready(_) => missing(
            shot,
            texts.saves_preview_reason(ScreenshotThumbKind::Decode),
            stamp,
        ),
        ScreenshotPreview::NotGenerated => missing(shot, texts.saves_preview_none(), stamp),
        ScreenshotPreview::Failed(failure) => {
            missing(shot, texts.saves_preview_reason(failure.kind), stamp)
        }
    }
}

/// 「没有可画的像素」那一格：**框里说为什么**（短句），**格子下面照旧是大小与时间**。
///
/// 为什么原因不进副文案：`160px` 宽的格子里，「原因 + OS 原文」会摊成三行，把墙变得比图还吵；
/// 而 `ScreenshotThumbFailure::detail`（`9437184 > 8388608 字节` / `Access is denied.`）留在
/// DTO 里，是排障用的原文，不是给墙看的正文。D54 第 6 条要的是「逐条、结构化、不是静默」——
/// 那一句在框里，每条各说各的。
fn missing(shot: &ScreenshotView, note: &str, stamp: String) -> Shot {
    Shot {
        name: shot.file.clone(),
        detail: stamp,
        preview: ShotPreview::Missing {
            note: note.to_string(),
        },
    }
}

/// 失败的一句话（**唯一产地**：页面内红字用它，本版没有 Toast）。
#[must_use]
pub fn failure_line(failure: &SavesFailureView, texts: Texts) -> String {
    let reason = texts.saves_reason(failure.kind);
    let sentence = match (failure.op, failure.kind) {
        (Some(SavesFailureOp::Rename), _) => texts.saves_rename_failed(&failure.file, reason),
        (Some(SavesFailureOp::Delete), _) => texts.saves_delete_failed(&failure.file, reason),
        (Some(SavesFailureOp::Backup), _) => texts.saves_backup_failed(&failure.file, reason),
        (None, SavesFailureKind::OpenFailed) => texts.saves_open_failed(reason),
        (None, _) => texts.saves_failed(reason),
    };
    if failure.detail.is_empty() {
        return sentence;
    }
    format!("{sentence}（{}）", failure.detail)
}
