//! 失败提示条的**文案与按钮**（M308：按 `retryable` 分流）。
//!
//! 判据来自 core：`UpdateErrorKind::is_retryable` 只有字节层（摘要 / 长度）、网络、IO 算可重试；
//! 签名五兄弟（漏签 / 签名坏 / 表里没这把钥 / 签名对不上）与归档三兄弟（没有唯一本体 / 本体不是
//! 本平台的可执行文件 / 本体大小不合规）都**不可重试**——成因在发布侧，重放一千次还是同一个坏包。
//!
//! | `retryable` | 正文 | 按钮 |
//! |---|---|---|
//! | `true` | 原因本身（「隔一会儿再试可能就过了」） | 一颗「重试」 |
//! | `false` | 「这次更新已被拒绝（重试没用）：原因」 | **没有**（空表） |
//!
//! 为什么拆到本文件：`pages/update.rs` 贴着 400 行硬线（AGENTS §3-1），而「失败了该说什么、
//! 还能不能点」是独立的一块问题（与 `busy.rs` 同一取舍）。拆开之后正文那一半还能被单测直接喂——
//! `text` 控件在 iced 0.14 里是 stateless 的（`tag() == Tag::stateless()`），真控件树里读不到字符串，
//! 所以「哪句话」只能在取词入口断、「按钮在不在」才在真控件树上断（`tests.rs`）。

use iced::Element;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, Button};
use crate::pages::message::UpdateAction;
use crate::pages::Message;
use crate::theme::Ui;

/// 失败提示条的正文。
#[must_use]
pub fn failed_body(catalog: Catalog, reason: &str, retryable: bool) -> String {
    if retryable {
        return reason.to_string();
    }
    catalog.render(Key::UpdateBannerFailedFinal, &[("reason", reason)])
}

/// 失败提示条的按钮：**只有可重试才画「重试」**。
///
/// 不可重试时返回空表：那颗键根本不在控件树里（M307 抓到的第二处不一致——此前不论
/// `retryable` 都画一颗「重试」，而 `error.rs` 早就把验签类标成不可重试了）。
pub(super) fn failed_actions<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    retryable: bool,
) -> Vec<Element<'a, Message>> {
    if !retryable {
        return Vec::new();
    }
    vec![buttons::text(
        ui,
        Button::new(
            catalog.text(Key::UpdateBannerRetry),
            Message::Update(UpdateAction::CheckNow),
        ),
    )]
}
