//! 视图模型：**presenter 快照 → 页面要画的东西**的唯一转换产物。
//!
//! ## 为什么页面不直接吃 presenter 的快照
//!
//! 页面层（`pages/*`）要能用**两套数据源**喂同一个渲染函数：
//!
//! | 数据源 | 谁转 | 用途 |
//! |---|---|---|
//! | presenter 的 `AppState` 快照 | `app/view.rs` 的自由函数 `model(&AppState, width, forced)` | 产品运行时的真页面 |
//! | `pages/instances/data.rs` 的演示条目 | [`crate::pages::demo`] | M84 骨架图的可复现基线、`pages_demo` example |
//!
//! 如果页面直接读 `InstanceSummary`，演示数据就得先伪造成 `InstanceSummary`（含 core 语义字段），
//! 基线图会跟着业务 DTO 的每次改动漂。所以中间加一层**纯展示字段**的模型：它只含「要画什么」。
//!
//! 文本都是 `String`/`&'static str`，没有框架类型——这一层可以纯单测（见 `pages/tests.rs`）。

use unlucky_i18n::Lang;
use unlucky_presenter::router::NavSection;

use crate::app::components::interaction::Interaction;

use super::content::Content;
use super::logs::Logs;
use super::{Download, Screen};

/// 实例列表的一行（`ListItem` 的槽位：overline = 运行状态，headline = 名字，supporting = 版本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 实例 id（点行 → `Route::Detail(id)`）
    pub id: String,
    /// 显示名
    pub name: String,
    /// 副文案（`1.20.4 · Fabric 0.16.5`）
    pub supporting: String,
    /// 运行状态（`None` = 空闲，不占 overline 槽位）
    pub status: Option<String>,
}

/// 元信息卡：标签 + 值（`VERSION` / `Fabric 0.16.5`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// 卡片上排的标签（`LabelMedium`）
    pub label: String,
    /// 卡片下排的值（`BodyMedium`）
    pub value: String,
}

/// 详情列要画的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    /// 实例 id（启动/停止命令要用它）
    pub id: String,
    /// 标题（`TitleLarge`）
    pub name: String,
    /// 标题下的一行（`BodySmall`）
    pub subtitle: String,
    /// 元信息网格（顺序 = 展示顺序；列数仍由 `layout::meta_columns` 决定）
    pub meta: Vec<Meta>,
    /// 正文档（演示数据有；真实快照没有对应字段时为 `None`）
    pub note: Option<String>,
    /// 运行状态行（`None` = 空闲，不占版面）
    pub status: Option<String>,
    /// 「Play」可用
    pub can_launch: bool,
    /// 「Stop」可用
    pub can_stop: bool,
}

/// 实例页要画的东西。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Instances {
    /// 列表列（顺序稳定 = 实例列表顺序）
    pub rows: Vec<Row>,
    /// 当前选中的实例详情（`None` = 没选中 → 空态）
    pub detail: Option<Detail>,
}

/// 一屏的视图模型。
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// 当前路由落在哪个页面上
    pub screen: Screen,
    /// 窗口宽度（**含外壳**：布局档位判据就是它）
    pub window_width: f32,
    /// 导航轨高亮项（`None` = 没有高亮，如 AI 独立页）
    pub active: Option<NavSection>,
    /// 实例页的数据（其它三页还是占位视觉，不读）
    pub instances: Instances,
    /// 下载页的数据（M94：版本列表 / 选中 / 安装进度，全部来自 presenter 快照）
    pub download: Download,
    /// 日志页的数据（M95：presenter 快照 → 日志行 + 诊断结论，见 [`super::logs`]）。
    pub logs: Logs,
    /// 内容库页的数据（M109：检索条件 / 命中项 / 装入闸门，见 [`super::content`]）。
    pub content: Content,
    /// 界面语言（M106）：页面文案走 i18n key，语言由 app 层决定后灌进来。
    ///
    /// 谁填：`app/state.rs` 的 `App::refresh_view`（`LangState::lang()` → `view::model(..)`），
    /// 即「用户档位 / 系统 locale → 生效语言」这条链的结论。**不是**基线值：M95 以前这里是
    /// `Lang::En` 占位，M106 之后中文系统上灌进来的是 `Lang::ZhCn`。
    pub lang: Lang,
    /// 取证用的强制交互态（`--force-state`）：`Some` 时页面里的控件按该态画。
    ///
    /// M90 r2（P3-1）之前这个开关只作用到 `shell` 回退路径，产品页是**静默失效**的；
    /// 现在由 `app/view.rs` 把 `cli::ForcedState` 翻成 `Interaction` 灌进模型。
    pub forced: Option<Interaction>,
}

impl Model {
    /// 空模型（只有外壳参数，页面数据全空）——窄窗退化与空态的构造入口。
    pub fn new(screen: Screen, window_width: f32, active: Option<NavSection>) -> Self {
        Self {
            screen,
            window_width,
            active,
            instances: Instances::default(),
            download: Download::loading(),
            logs: Logs::default(),
            // 演示模型也从这个空壳起步（`demo_model` 再按页面填数据）
            content: super::content::demo(unlucky_presenter::dto::ContentKind::Mod, Lang::En),
            lang: Lang::En,
            forced: None,
        }
    }
}
