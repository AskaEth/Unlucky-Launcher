//! 页面消息与动作表（M101 r2 从 `pages/mod.rs` **原样搬出**）。
//!
//! 为什么拆：合并 M100/M101 之后 `pages/mod.rs` 是 397 行，只剩 3 行余量，下一条线往枚举里加
//! 一个变体（含文档）就顶破 400 硬线。拆法与 M99 的 `pages/nav.rs` 同一种范式：**纯搬移**、
//! 一行不改、路径靠 `pages/mod.rs` 的 `pub use` 再导出保持不变（`pages::Message` / `pages::Action`）。
//!
//! `pages::Message` 是页面层的自有消息类型（**不** import `crate::app::Message`，组件契约 §2 的
//! 同一条纪律）；app 层只做一次 `.map`。

use unlucky_i18n::LangChoice;
use unlucky_presenter::router::Route;

use super::appearance;
use super::content;
use super::download::{Category, Loader};
use super::settings;

/// 页面消息。
///
/// **故意不用 `crate::app::Message`**（组件库契约 §2 的同一条理由）：页面不绑 app 的状态层，
/// 接线时由 app 层把这里的三类意图翻成 presenter 的 `AppCommand` / `Route`。
#[derive(Debug, Clone)]
pub enum Message {
    /// 骨架期动作（三页的占位按钮 + 实例页上还没接后端的两个页头动作）。
    Action(Action),
    /// 路由跳转：导航轨、列表行、窄窗「返回」都走这一条（栈语义归 `presenter::router`）。
    Navigate(Route),
    /// 启动某实例（app 层翻成 `AppCommand::LaunchInstance`）。
    Launch(String),
    /// 停止某实例（app 层翻成 `AppCommand::StopInstance`）。
    Stop(String),
    /// 外观项改动（设置页「外观」分组 → `App` 的状态与落盘）。
    ///
    /// 载荷是**用户请求值**（未钳位）：钳位与 CPU 档降级在 `appearance::alpha` 里做，
    /// 页面只表达意图（与组件库「不持有业务状态」同一条纪律）。
    Appearance(appearance::Change),
    /// 选中下载页的一个版本（M94）：**页面内选中**，app 层只改视图模型，不 dispatch。
    SelectVersion(String),
    /// **确认安装**选中的版本（用当前安装选项）。
    ///
    /// M105 起语义变了：以前是「立刻装原版」（`AppCommand::InstallVersion`），现在是
    /// 「用选项区里选好的加载器 / 加载器版本 / 实例名开工」（`AppCommand::CreateInstance`）。
    /// 载荷仍是版本 id，app 层拿它去草稿里取其余三项。
    InstallVersion(String),
    /// 改安装选项的加载器（M105；app 层改草稿，必要时派 `AppCommand::ListLoaderVersions`）。
    SetLoader(Loader),
    /// 改加载器版本（M105；空串 = 用官方最新）。
    SetLoaderVersion(String),
    /// 改实例名（M105；空串 = 用版本号）。
    SetInstanceName(String),
    /// 取消正在跑的安装（M94；`AppCommand::CancelInstall`）。
    CancelInstall,
    /// 切换下载页分类（M94；app 层翻成 `AppCommand::ListVersions`，`All` 只画现有快照）。
    SetCategory(Category),
    /// 搜索版本号（**M107**）：查询词写进 `AppState.download_query`，页面走同一条重建路径过滤。
    ///
    /// 为什么不是「页面自己存一份查询」：下载页是**无状态视图**（每帧由快照重建），页面持有的
    /// 字段活不过下一次事件；查询词只有落在 presenter 的快照里才能跨帧存活，而
    /// `Download::from_snapshot` 本来就读 `download_query` —— 复用这条路，过滤只有一份真相。
    SetVersionQuery(String),
    /// 清空搜索框（**M107**）。与 [`Self::SetVersionQuery`] 同一支的 `""` 特例，
    /// 单独一个变体是为了让「清空」这个意图在日志与测试里一眼可辨。
    ClearVersionQuery,
    /// 选中第二级侧栏的一个主版本桶（**M107**；`None` = 全部版本）。
    ///
    /// **纯视图态**（与 [`Self::SelectVersion`] 同一类）：不派业务命令，app 层只改视图模型。
    SelectMajor(Option<String>),
    /// 折叠 / 展开第一级（版本类型）侧栏（**M107**）。纯视图态。
    ToggleTypeRail,
    /// 把现成文本塞进剪贴板（**M95**：诊断卡片的一行摘要，文本由页面造，app 只搬）。
    CopySummary(String),
    /// 设置页的一组消息（**M101 `SETTINGSMSG`**）：设置页的**唯一**投递口。
    ///
    /// 四条填页线（java / download / language+appearance / ai+about）各自在自己的文件里定义
    /// 消息枚举，本枚举**不再**为设置页增加第二个变体——「加一支设置页消息」从此不碰本文件，
    /// 也不碰 `app/update/page_messages.rs` 那条不许 `_ =>` 的 match。加变体与落点见
    /// `pages/settings/message.rs`（`app/update/settings.rs` 对它做穷尽分发，漏接线报 E0004）。
    Settings(settings::GroupMessage),
    /// 换界面语言（**M106**：设置页 General 分组的语言行 → `App::set_language`）。
    ///
    /// 载荷是**用户意图**（跟随系统 / 手选某一门语言），不是「生效语言」：生效语言由
    /// `LangChoice::resolve` 按 D28 的顺序算出来，页面不参与计算。
    ///
    /// 为什么不折进 [`Message::Settings`] 那个投递口：语言档位不是「某一组的状态」——它要**落盘**
    /// 到 `ui.language` 并**重建整棵视图模型**（目录是 `pages::Model` 的一部分），而那条落点
    /// （`app/update/settings/language_appearance.rs`）是 language+appearance 线的文件。
    /// 代价是本文件多一个变体；收口方向写在 `settings/general.rs` 的文档里。
    Language(LangChoice),
    /// 内容库页的一组消息（**M109**）：与设置页同一个范式（页面自己的枚举 + 一个投递口）。
    ///
    /// 加一条内容库消息只动 `pages/content/message.rs`，不动本枚举的正文，也不动
    /// `app/update/page_messages.rs` 那条不许 `_ =>` 的 match。
    Content(content::Message),
    /// 账户页消息（**M114**：三轨登录 / 列表动作 / 表单输入；app 层翻成 `AppCommand`）。
    ///
    /// 与 `Settings` 同一种做法：账户页自己的消息枚举在 `pages/account.rs`，这里只加一支入口。
    Account(crate::pages::account::Message),
}

/// 页面动作清单（骨架期够用就行；接线时按业务语义重命名/扩充）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 新建实例（**还没有后端**，见模块文档偏离 #8）。
    NewInstance,
    /// 导入（同上）。
    Import,
    /// 返回实例列表（窄窗整页切换）。**已由 `Message::Navigate(Route::Instances)` 取代**，
    /// 这里保留枚举项是为了别把「返回」这件意图从动作表里抹掉（外壳回退路径仍可能用到）。
    Back,
    /// 启动（占位动作表里的「启动」；真启动走 `Message::Launch`）。
    Play,
    /// 更多。
    More,
    /// 复制实例。
    Duplicate,
    /// 打开实例目录。
    OpenFolder,
    /// 删除实例。
    Delete,
    /// 重新探测下载源。
    RefreshSources,
    /// 浏览版本列表。
    BrowseVersions,
    /// 导入整合包。
    ImportModpack,
    /// 检查下载源。
    CheckSources,
    /// 打开日志目录。
    OpenLogs,
    /// 复制诊断信息。
    CopyDiagnostics,
    /// 清空日志。
    ClearLogs,
    /// 重置当前设置分组。
    ResetSection,
    /// 保存设置。
    SaveSettings,
    /// 新增逐实例覆盖。
    AddOverride,
}
