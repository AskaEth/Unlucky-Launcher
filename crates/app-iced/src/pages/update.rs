//! 自更新（M159）的**页面视图模型**：只放「要画什么」。
//!
//! 与 `presenter::dto::UpdateView` 的分工和本目录其它页一致（见 `pages/model.rs` 模块文档）：
//! DTO 是 IPC 契约（含 core 语义字段），这里是纯展示字段，于是页面可以用演示数据喂同一个渲染函数。
//! 转换点只有一个：`app/view/update.rs`。
//!
//! ## 为什么状态是「一个有载荷的枚举」
//!
//! 「检查失败」与「已是最新」必须是两个变体（mission 硬要求）：把失败折叠成
//! `UpToDate` 会让界面在断网时说「已是最新」，那是撒谎。`MissingArtifact`（有新版但没本平台产物）
//! 同理——它既不是「有新版本可装」，也不是「已是最新」。

/// 一次检查的结论（页面视图）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateStatus {
    /// 还没查过
    #[default]
    Idle,
    /// 正在查
    Checking,
    /// 已是最新
    UpToDate {
        /// 该渠道内的最高版本（`None` = 还没有任何发布）
        latest: Option<String>,
    },
    /// 有可装的更新
    Available {
        /// 新版本号
        version: String,
        /// 包体字节数
        size: u64,
        /// 更新说明（可能为空）
        notes: String,
        /// 是否强制
        forced: bool,
    },
    /// 有更高版本，但没有本平台的产物
    MissingArtifact {
        /// 那个版本
        version: String,
    },
    /// 检查/应用失败（**不是**已是最新）
    Failed {
        /// 人话原因
        reason: String,
        /// 换个时间再试是否可能变好
        retryable: bool,
    },
}

/// 顶部提示条此刻的形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Banner<'a> {
    /// 有新版本
    Available {
        /// 新版本号
        version: &'a str,
        /// 包体字节数
        size: u64,
        /// 说明摘要
        notes: &'a str,
    },
    /// 正在下载/校验
    Applying,
    /// 已就位，等重启
    Ready {
        /// 新版本号
        version: &'a str,
    },
    /// 失败（只在用户主动查之后出现）
    Failed {
        /// 人话原因
        reason: &'a str,
        /// 是否可重试
        retryable: bool,
    },
}

/// 更新页 / 提示条的全部展示状态。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Update {
    /// 当前版本
    pub current_version: String,
    /// 渠道（`stable`）
    pub channel: String,
    /// 启动时是否静默检查
    pub auto_check: bool,
    /// 检查结论
    pub status: UpdateStatus,
    /// 上次检查时间（空串 = 没查过）
    pub checked_at: String,
    /// 正在下载/校验/替换
    pub applying: bool,
    /// 已就位的新版本号（`None` = 没有）
    pub ready: Option<String>,
    /// 用户点了「忽略这个版本」时记下的**版本号**（`None` = 没忽略）。
    ///
    /// 只压住**这一个**版本：0.2.3 被忽略后同一次运行里冒出 0.2.4 仍然提示
    /// （文案是「忽略这个版本」，不是「别再问我」）。
    pub skipped: Option<String>,
    /// 这一份结论来自静默检查（启动时/自动）：失败时**不**弹提示条
    pub silent: bool,
}

mod busy;

impl Update {
    /// 有没有可装的候选（有的话设置页给「下载并安装」）。
    #[must_use]
    pub fn available(&self) -> Option<(&str, u64, &str, bool)> {
        match &self.status {
            UpdateStatus::Available {
                version,
                size,
                notes,
                forced,
            } => Some((version, *size, notes, *forced)),
            _ => None,
        }
    }

    /// 提示条此刻要显示什么（`None` = 不显示）。
    ///
    /// 优先级与 presenter 侧同一条（就位 > 进行中 > 失败（仅主动查） > 有新版本（未被忽略）），
    /// 这里再算一次是因为页面层拿不到 DTO 的那份判据——两处判据的顺序必须一致，
    /// `pages/tests.rs` 有对拍用例守着。
    #[must_use]
    pub fn banner(&self) -> Option<Banner<'_>> {
        if let Some(version) = &self.ready {
            return Some(Banner::Ready { version });
        }
        if self.applying {
            return Some(Banner::Applying);
        }
        if let UpdateStatus::Failed { reason, retryable } = &self.status {
            return (!self.silent).then_some(Banner::Failed {
                reason,
                retryable: *retryable,
            });
        }
        match &self.status {
            UpdateStatus::Available {
                version,
                size,
                notes,
                ..
            } if self.skipped.as_deref() != Some(version.as_str()) => Some(Banner::Available {
                version,
                size: *size,
                notes,
            }),
            _ => None,
        }
    }
}

/// 出图 / 单测用的演示快照（一个可装的新版本 + 一次成功的检查）。
///
/// **不是产品数据**：产品路径的更新状态只来自 presenter（`app/view/update.rs` 转换）。
///
/// 取证开关 `ULL_UPDATE_STATE`（M161，沿用 `ULL_CONTENT_*` 那套范式）：不设 = 缺省那一态；
/// `checking` / `applying` 用来出「按钮置灰 + 状态行说正在干活」的那两张图，
/// `failed` / `up_to_date` 用来出另外两条结论。
#[must_use]
pub fn demo() -> Update {
    let mut snapshot = demo_available();
    match std::env::var("ULL_UPDATE_STATE")
        .unwrap_or_default()
        .as_str()
    {
        "checking" => snapshot.status = UpdateStatus::Checking,
        "applying" => snapshot.applying = true,
        "failed" => {
            snapshot.status = UpdateStatus::Failed {
                reason: "Could not fetch the release manifest: connection reset".to_string(),
                retryable: true,
            }
        }
        "up_to_date" => {
            snapshot.status = UpdateStatus::UpToDate {
                latest: Some("0.2.2".to_string()),
            }
        }
        _ => {}
    }
    snapshot
}

/// 缺省那一态（可装的新版本）。
fn demo_available() -> Update {
    Update {
        current_version: "0.2.2".to_string(),
        channel: "stable".to_string(),
        auto_check: true,
        status: UpdateStatus::Available {
            version: "0.2.3".to_string(),
            size: 21_500_000,
            notes: "修掉启动崩溃；实例页加载更快".to_string(),
            forced: false,
        },
        checked_at: "11-08 12:00".to_string(),
        applying: false,
        ready: None,
        skipped: None,
        silent: false,
    }
}

/// 「有新版本」那条提示条的正文：**体积与更新说明摘要都要给**（mission 明列的三件之二）。
///
/// 拆成函数是为了可断言（`Element` 没有内省接口）：条数与措辞由 `tests` 的两条用例钉住。
#[must_use]
pub fn available_body(catalog: Catalog, size: u64, notes: &str) -> String {
    let size = human_bytes(size);
    let body = catalog.render(Key::UpdateBannerBodyAvailable, &[("size", &size)]);
    if notes.trim().is_empty() {
        body
    } else {
        format!("{body} · {}", notes.trim())
    }
}

// --------------------------------------------------------------- 提示条

use iced::widget::{column, container, row};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::human_bytes;

use crate::app::components::buttons::{self, Button};
use crate::pages::frame::text_role_owned;
use crate::pages::message::UpdateAction;
use crate::pages::Message;
use crate::theme::Ui;

/// 顶部提示条（每一页都在页头之上；`None` = 现在没有要说的）。
///
/// 形态三种：有新版本（可下载 / 可忽略）、正在下载、已就位（可立即重启）。**失败不在这里**
/// ——静默检查失败是常态，弹一条红条等于每次开机先骂网络一次；失败原因在设置页「更新」区看。
pub fn banner<'a>(
    ui: &'a Ui,
    update: &'a Update,
    catalog: Catalog,
) -> Option<Element<'a, Message>> {
    let kind = update.banner()?;

    // 三种形态各自的底色：有新版 = 次级容器（可操作、非错误）；进行中 = 同色但只读；
    // 就位 = 主容器（「已经好了，重启即生效」是正反馈）。
    let (title, body, actions, role): (String, String, Vec<Element<'a, Message>>, Role) = match kind
    {
        Banner::Available {
            version,
            size,
            notes,
        } => {
            let mut actions = vec![
                buttons::filled(
                    ui,
                    Button::new(
                        catalog.text(Key::UpdateBannerInstall),
                        Message::Update(UpdateAction::Apply),
                    ),
                ),
                buttons::text(
                    ui,
                    Button::new(
                        catalog.text(Key::UpdateBannerDismiss),
                        Message::Update(UpdateAction::Dismiss),
                    ),
                ),
            ];
            let _ = &mut actions;
            (
                catalog.render(Key::UpdateBannerTitleAvailable, &[("version", version)]),
                available_body(catalog, size, notes),
                actions,
                Role::SecondaryContainer,
            )
        }
        Banner::Applying => (
            catalog.text(Key::UpdateBannerApplying).to_string(),
            String::new(),
            Vec::new(),
            Role::SecondaryContainer,
        ),
        Banner::Ready { version } => (
            catalog.text(Key::UpdateBannerTitleReady).to_string(),
            catalog.render(Key::UpdateBannerBodyReady, &[("version", version)]),
            vec![buttons::filled(
                ui,
                Button::new(
                    catalog.text(Key::UpdateBannerRestartNow),
                    Message::Update(UpdateAction::RestartNow),
                ),
            )],
            Role::PrimaryContainer,
        ),
        Banner::Failed { reason, .. } => (
            catalog.text(Key::UpdateBannerTitleFailed).to_string(),
            reason.to_string(),
            vec![buttons::text(
                ui,
                Button::new(
                    catalog.text(Key::UpdateBannerRetry),
                    Message::Update(UpdateAction::CheckNow),
                ),
            )],
            Role::ErrorContainer,
        ),
    };

    let on_container = match role {
        Role::PrimaryContainer => Role::OnPrimaryContainer,
        Role::ErrorContainer => Role::OnErrorContainer,
        _ => Role::OnSecondaryContainer,
    };

    let mut text_column = column![text_role_owned(
        ui,
        TypeRole::LabelLarge,
        true,
        title,
        ui.color(on_container),
    )]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);
    if !body.is_empty() {
        text_column = text_column.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            body,
            ui.color(on_container),
        ));
    }

    let mut line = row![text_column]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .align_y(Alignment::Center);
    for action in actions {
        line = line.push(action);
    }

    let padding = Padding::from([ui.space(Space::X3), ui.space(Space::X4)]);
    Some(
        container(line)
            .width(Length::Fill)
            .padding(padding)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(ui.color(role))),
                border: Border {
                    radius: ui.radius(ShapeScale::Medium, 0.0),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into(),
    )
}
#[cfg(test)]
#[path = "update/tests.rs"]
mod tests;
