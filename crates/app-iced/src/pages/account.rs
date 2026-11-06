//! 账户页（M114）：**三轨**登录（微软正版 / 第三方皮肤站 / 本地档案）+ 账户列表 + 皮肤区。
//!
//! ## 这一页回答什么
//!
//! 用户的原话是「用户管理页面还没做吗？包括不限于正版登录 离线登录 第三方皮肤站登录」。
//! Slint 版曾有这个页面（`DevDocs/ops/screens-m43/account-data.png`），换 iced 时没搬过来，
//! 于是 iced 版**一个账户入口都没有**——本页就是把它补回来，并按 M114 的口径做成三轨。
//!
//! ## 四条红线（都在本页的断言里）
//!
//! | 红线 | 落点 |
//! |---|---|
//! | 页面不得出现任何 token / refresh_token | 上屏文案只来自 [`model`] 的公开字段；`tests` 里有「把 JWT 塞进事件、断言模型干净」的用例 |
//! | 离线档案的说明必须带「仅本地/隔离网络测试用，无法连接启用正版验证的服务器」 | `Key::AccountTrackOfflineBody` / `Key::AccountOfflineWarning` 两处都写全 |
//! | 设备码不得只转圈 | [`login`]：用户码 + 链接 + 倒计时 + 「已过期，重试」 |
//! | 没有皮肤不给假皮肤 | 头像位 = 首字母块（`profile_id` 稳定取色）；第三方轨才可能出真图 |
//!
//! ## 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`model`] | 状态 → 视图模型（列表行 / 当前账户 / 三条轨道的面板态） |
//! | [`avatar`] | 首字母块 + 皮肤图（解码/裁剪缓存；失败一律退回首字母块，**绝不白块**） |
//! | [`widgets`] | 本页的小件（头像块 / 章节标题 / 徽标），样式全部走令牌 |
//! | [`rows`] | 当前账户卡片 + 账户列表行 |
//! | [`forms`] | 三条轨道的入口卡 + 本地档案/第三方站表单 |
//! | [`login`] | 微软登录面板（设备码倒计时 / 浏览器授权 / 失败态） |
//! | `tests` | 视图模型的判据（凭据红线、空态、倒计时、皮肤回落） |

pub mod avatar;
pub mod forms;
pub mod login;
pub mod model;
pub mod panels;
pub mod rows;
pub mod widgets;

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "account/tests_text.rs"]
mod tests_text;

pub use model::{Form, Hint, Notice, Page, Panel, Track};

use iced::widget::{column, container, scrollable};
use iced::{Element, Length, Padding};

use unlucky_design::spacing::Space;
use unlucky_i18n::{Key, Lang};

use crate::app::components::buttons::{self, Button};
use crate::pages::frame::{self, Header};
use crate::pages::layout::FrameMetrics;
use crate::theme::Ui;

use super::Message as PageMessage;

/// 账户页的页面消息（app 层把它翻成 `AppCommand` / 状态写入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 打开某条轨道的表单
    Open(Track),
    /// 关掉表单/登录面板
    Close,
    /// 第三方站地址输入
    Server(String),
    /// 登录名输入
    Login(String),
    /// 密码输入（只存在内存里，随请求交给 host，用完即弃）
    Password(String),
    /// 本地档案名输入
    OfflineName(String),
    /// 校验服务器地址（服务发现）
    Probe,
    /// 表单提交（本地档案 → 建档案；第三方站 → 登录）
    Submit,
    /// 微软：设备码流
    StartDevice,
    /// 微软：授权码流（拉起系统浏览器）
    StartBrowser,
    /// 取消/关掉登录面板
    Cancel,
    /// 复制一段文本（用户码 / 链接）
    Copy(String),
    /// 切换当前启动账户
    Switch(String),
    /// 删除账户
    Remove(String),
    /// 重新登录（按账户类型分派：微软 → 授权码流；第三方 → 预填表单；离线 → 明确说明）
    Relogin(String),
    /// 展开/收起「Add an account」的三张轨道卡（复审 r1 P2-4）
    ToggleAdd,
    /// 关掉横幅
    DismissNotice,
}

impl Message {
    /// 包成页面的消息（本模块内部与 app 层都用它，保证只有一处包装点）。
    pub fn into_page(self) -> PageMessage {
        PageMessage::Account(self)
    }
}

/// 整页。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    page: &'a Page,
    lang: Lang,
) -> Element<'a, PageMessage> {
    let text = lang.catalog();
    let subtitle = text.text(Key::AccountSubtitle);
    let add = text.text(Key::AccountEmptyAction);
    let header = Header::new(text.text(Key::AccountTitle))
        .subtitle(subtitle)
        .action(buttons::tonal(
            ui,
            Button::new(add, Message::ToggleAdd.into_page()),
        ));

    let top = if page.rows.is_empty() {
        forms::empty(ui, text, lang)
    } else {
        column![
            rows::active_card(ui, page, text, lang),
            rows::list(ui, page, text, lang),
        ]
        .spacing(ui.space(Space::X6))
        .into()
    };

    // 三条轨道入口 + 表单/登录面板：面板存在时**只画面板**，且**置顶**——
    // 否则在 1180×760 下设备码的「复制用户码 / 取消」会掉到折线以下（D30 复审实测）。
    let flows = match page.panel {
        Panel::None => forms::tracks(ui, text, lang),
        _ => forms::panel(ui, page, text, lang),
    };

    let mut content = column![].spacing(ui.space(Space::X6)).width(Length::Fill);
    if let Some(notice) = &page.notice {
        content = content.push(widgets::banner(ui, notice, text));
    }
    if let Some(hint) = page.hint {
        content = content.push(widgets::hint(ui, hint, page, text));
    }
    // 三张轨道卡：空态**总是**画（那就是空态的入口）；有账号时收在「Add an account」后面
    let show_tracks = page.panel == Panel::None && (page.rows.is_empty() || page.form.section_open);
    content = if page.panel == Panel::None {
        if show_tracks {
            content.push(top).push(flows)
        } else {
            content.push(top)
        }
    } else {
        content.push(flows).push(top)
    };

    // 右缘留 8px gutter：全局 `frame::scroll_style` 的滚动条是 6px + 2px 内缩，
    // 满宽卡片会被它压住（tower 看图意见 ②）。页面级留白是这条的最小修法。
    let padded = container(content).width(Length::Fill).padding(Padding {
        right: ui.space(Space::X2),
        ..Padding::default()
    });

    frame::page(
        ui,
        metrics,
        header,
        scrollable(padded)
            .style(frame::scroll_style(ui))
            .width(Length::Fill)
            .height(Length::Fill),
    )
}
