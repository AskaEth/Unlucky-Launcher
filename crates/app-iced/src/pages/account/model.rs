//! 状态 → 账户页视图模型（页面层不认识 presenter 的其它类型，只认这里）。
//!
//! 三条轨道在 UI 上只有三种「面板」：微软登录面板 / 第三方站表单 / 本地档案表单。
//! 表单里那些**输入串**是纯 UI 状态，归 [`Form`]（app 层持有）；从 presenter 来的只有
//! 账户列表、登录进度与倒计时。

use iced::widget::image;

use unlucky_i18n::{Key, Lang};
use unlucky_presenter::dto::{
    AccountKind, AccountNotice, AccountNoticeCode, AccountView, BrowserLoginNote,
};
use unlucky_presenter::state::AppState;

use super::avatar::{Avatar, Cache};
use super::panels::{authlib_panel, device_panel};

pub use super::panels::{AuthlibPanel, DevicePanel};

/// 三条轨道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    /// 微软正版（设备码 / 授权码两条流）
    Microsoft,
    /// 第三方皮肤站（authlib-injector）
    Authlib,
    /// 本地档案（离线）
    Offline,
}

impl Track {
    /// 卡片标题的 key。
    #[must_use]
    pub const fn title_key(self) -> Key {
        match self {
            Self::Microsoft => Key::AccountTrackMicrosoftTitle,
            Self::Authlib => Key::AccountTrackAuthlibTitle,
            Self::Offline => Key::AccountTrackOfflineTitle,
        }
    }

    /// 卡片说明的 key。
    #[must_use]
    pub const fn body_key(self) -> Key {
        match self {
            Self::Microsoft => Key::AccountTrackMicrosoftBody,
            Self::Authlib => Key::AccountTrackAuthlibBody,
            Self::Offline => Key::AccountTrackOfflineBody,
        }
    }

    /// 卡片主按钮的 key（微软/第三方是「登录」，本地是「创建档案」）。
    #[must_use]
    pub const fn action_key(self) -> Key {
        match self {
            Self::Microsoft => Key::AccountTrackOpen,
            Self::Authlib => Key::AccountTrackOpen,
            Self::Offline => Key::AccountTrackCreate,
        }
    }

    /// 三条轨道（卡片顺序）。
    pub const ALL: [Track; 3] = [Track::Microsoft, Track::Authlib, Track::Offline];
}

/// 当前打开的面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    /// 只画三条轨道入口
    #[default]
    None,
    /// 微软登录面板
    Microsoft,
    /// 第三方站表单
    Authlib,
    /// 本地档案表单
    Offline,
}

/// 表单的**纯 UI 状态**（app 层持有，随消息改动）。
///
/// 密码只存在这里和「一次 `AppCommand::AddAuthlibAccount` 的载荷」里：不进日志、不进快照、
/// 不进任何回程 DTO（M114 凭据红线）。
/// **手写 `Debug`**：`Form` 里持着一次性密码，任何 `{:?}` 都不许带出明文（凭据红线）。
impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form")
            .field("panel", &self.panel)
            .field("server", &self.server)
            .field("login", &self.login)
            .field("password", &"⟪redacted⟫")
            .field("offline_name", &self.offline_name)
            .field("section_open", &self.section_open)
            .field("hint", &self.hint)
            .finish()
    }
}

/// 三条轨道的表单输入（纯 UI 状态；密码是**一次性请求值**）。
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Form {
    /// 当前面板
    pub panel: Panel,
    /// 第三方站地址
    pub server: String,
    /// 登录名
    pub login: String,
    /// 密码（一次性的请求值）
    pub password: String,
    /// 本地档案名
    pub offline_name: String,
    /// 「Add an account」展开态（有账号时三张轨道卡默认收起，复审 r1 P2-4）
    pub section_open: bool,
    /// 一次性引导（重新登录时告诉用户下一步；`None` = 没有引导）
    pub hint: Option<Hint>,
}

/// 一次性引导（由 app 层在「重新登录」时设置，页面画一行 i18n）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// 第三方轨重登：重新输入密码
    AuthlibReauth,
    /// 本地档案没有可重登的服务器账户
    OfflineNoRelogin,
}

impl Form {
    /// 空表单（只画三条轨道）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 打开某条轨道的面板（已开着同一个就用原值，避免输入被清掉）。
    pub fn open(&mut self, track: Track) {
        self.panel = match track {
            Track::Microsoft => Panel::Microsoft,
            Track::Authlib => Panel::Authlib,
            Track::Offline => Panel::Offline,
        };
    }

    /// 关掉面板（**密码一起清掉**：留着没有意义，只是多留一份凭据在内存里）。
    pub fn close(&mut self) {
        self.password.clear();
        self.hint = None;
        self.panel = Panel::None;
    }

    /// 提交之后清掉一次性凭据（成功与否都清：重试用新输入）。
    pub fn clear_secret(&mut self) {
        self.password.clear();
    }

    /// 本地档案名是否已填（`trim` 后非空才算）。
    #[must_use]
    pub fn offline_ready(&self) -> bool {
        // 与 host 写盘前用的是**同一份判据**（`unlucky_presenter::dto::valid_offline_name`）：
        // 按钮亮着却必然失败，比按钮灰着更糟（复审 r1 P2-2 的另一半）。
        unlucky_presenter::dto::valid_offline_name(self.offline_name.trim())
    }

    /// 第三方站表单是否已填（三项都非空；地址的语法问题由 host 的规范化兜）。
    #[must_use]
    pub fn authlib_ready(&self) -> bool {
        !self.server.trim().is_empty() && !self.login.trim().is_empty() && !self.password.is_empty()
    }
}

/// 回退说明码 → i18n key（**唯一的「码→文案」映射点**；原文永远不上屏）。
///
/// 与 [`Track::title_key`] 同一种写法：页面只认 key，后端只给码，两边独立演进。
#[must_use]
pub const fn note_key(note: BrowserLoginNote) -> Key {
    match note {
        BrowserLoginNote::RedirectNotRegistered => Key::AccountBrowserFallbackNotRegistered,
        BrowserLoginNote::PreflightUnavailable => Key::AccountBrowserFallbackUnverified,
        BrowserLoginNote::AttemptFailed => Key::AccountBrowserFallbackFailed,
    }
}

/// 列表行 / 当前账户卡片用的数据。
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// 稳定 id（切/删/重登都按它派命令）
    pub id: String,
    /// 展示名
    pub name: String,
    /// 首字母块的字
    pub monogram: String,
    /// 首字母块色号（0..6，`profile_id` 稳定推导）
    pub tone: u8,
    /// 轨道
    pub kind: AccountKind,
    /// 轨道文案（i18n）
    pub kind_label: String,
    /// 「添加于 {date}」
    pub added: String,
    /// 副行：服务器 / 登录名（没有就空）
    pub detail: String,
    /// 是否是当前启动账户
    pub active: bool,
    /// 裁好的头像（`None` = 画首字母块）
    pub head: Option<image::Handle>,
    /// 皮肤原图（皮肤预览区）
    pub skin: Option<image::Handle>,
}

/// 整页视图模型。
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// 账户行
    pub rows: Vec<Row>,
    /// 当前启动账户在 `rows` 里的下标
    pub active: Option<usize>,
    /// 打开的面板
    pub panel: Panel,
    /// 表单值（渲染要读）
    pub form: Form,
    /// 微软登录面板
    pub device: DevicePanel,
    /// 第三方轨面板
    pub authlib: AuthlibPanel,
    /// 校验通过的服务器名（表单上显示）
    pub checked_server: String,
    /// 版面宽度（**中栏可用宽**，M104 的 `AgentUi::content_width`；账户页自己的
    /// `layout::frame_metrics` 拿它算列宽，与其它页同源）
    pub width: f32,
    /// 账户页横幅（添加/删除/切换的结果；`None` = 不画）
    pub notice: Option<Notice>,
    /// 一次性引导（重新登录的下一步）
    pub hint: Option<Hint>,
}

/// 横幅的渲染数据（文案在视图层按 `code` 查 i18n）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// 结果码
    pub code: AccountNoticeCode,
    /// 主题（账户名/服务器地址）
    pub subject: String,
    /// 是否失败（决定配色）
    pub failure: bool,
}

impl Notice {
    fn of(notice: &AccountNotice) -> Self {
        Self {
            code: notice.code,
            subject: notice.subject.clone(),
            failure: notice.code.is_failure(),
        }
    }
}

impl Page {
    /// 从状态与表单构造（`avatars` 是解码缓存，见 [`Cache`]；`width` 是中栏可用宽）。
    pub fn build(
        state: &AppState,
        form: &Form,
        avatars: &mut Cache,
        lang: Lang,
        width: f32,
    ) -> Self {
        let rows: Vec<Row> = state
            .accounts
            .iter()
            .map(|account| row_of(account, avatars, lang))
            .collect();
        let active = rows.iter().position(|row| row.active);
        let remaining = state.device_code_remaining();
        let device = device_panel(&state.device_login, remaining, lang.catalog());
        // 登录在进行（或刚失败/刚成功）时，面板**必须**是微软那条轨：否则用户点了登录、
        // 后台已经在等设备码，页面上却只剩三张入口卡——那正是「只转圈」的另一种坏法。
        let panel = if matches!(device, DevicePanel::Hidden) {
            form.panel
        } else {
            Panel::Microsoft
        };
        let (authlib, checked_server) = authlib_panel(&state.authlib_login);
        // 「校验通过」的服务器名只在该面板打开且地址一致时显示（换了地址就不再是它）
        let checked_server = if form.server.trim().is_empty()
            || state.authlib_login.server.trim().is_empty()
            || form.server.trim() != state.authlib_login.server.trim()
        {
            String::new()
        } else {
            checked_server
        };
        Page {
            rows,
            active,
            panel,
            form: form.clone(),
            device,
            authlib,
            checked_server,
            notice: state.account_notice.as_ref().map(Notice::of),
            hint: form.hint,
            width,
        }
    }

    /// 当前账户行。
    #[must_use]
    pub fn active_row(&self) -> Option<&Row> {
        self.active.and_then(|index| self.rows.get(index))
    }

    /// 有没有账户（空态判据）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// 皮肤预览区要画的那张图（当前账户有皮肤才画）。
    #[must_use]
    pub fn active_skin(&self) -> Option<&image::Handle> {
        self.active_row().and_then(|row| row.skin.as_ref())
    }
}

/// 单行。
fn row_of(account: &AccountView, avatars: &mut Cache, lang: Lang) -> Row {
    let text = lang.catalog();
    let avatar: Avatar = avatars.resolve(&account.id, account.skin.as_ref());
    let kind_label = match account.kind {
        AccountKind::Microsoft => text.text(Key::AccountKindMicrosoft),
        AccountKind::Authlib => text.text(Key::AccountKindAuthlib),
        AccountKind::Offline => text.text(Key::AccountKindOffline),
    };
    let detail = if !account.server_url.is_empty() {
        text.render(
            Key::AccountServer,
            &[("server", account.server_url.as_str())],
        )
    } else {
        account.login_name.clone()
    };
    Row {
        id: account.id.clone(),
        name: account.name.clone(),
        monogram: account.monogram(),
        tone: account.monogram_tone(),
        kind: account.kind,
        kind_label: kind_label.to_string(),
        added: text.render(Key::AccountAddedOn, &[("date", account.created.as_str())]),
        detail,
        active: account.active,
        head: avatar.head,
        skin: avatar.skin,
    }
}
