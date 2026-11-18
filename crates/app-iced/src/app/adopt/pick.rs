//! Pick 段：候选列表 + 目标档 + 实例名。
//!
//! **可导入的候选才可点**：不可导入的行把 `problems` 逐条列在下面但不接点击——点它只会换来
//! 一条注定被 core 拒绝的命令（`adopt_not_adoptable`），而用户从行上已经看到了原因。
//!
//! 目标档三枚 chip 的**短提示**在选中时立刻换（共享/版本目录档用 `Error` 色）；
//! **后果原文**不在这里——它在 Preview 段逐条画 `plan.notes`（core `target_notes()` 的原文）。

use iced::widget::{button, column, container, row};
use iced::{Alignment, Background, Border, Color, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space as Sp;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{AdoptCandidateView, AdoptPlanView, AdoptScanView, AdoptTargetView};

use crate::app::components::inputs::{text_field, Field};
use crate::app::components::interaction::{self, Interaction};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame;
use crate::theme::Ui;

use super::super::message::Message;
use super::text::{label, problem_line, target_label, wrapped};
use super::{adopt, AdoptMessage, Form};

/// Pick 段整体。
pub(super) fn pick<'a>(
    ui: &'a Ui,
    form: &'a Form,
    scan: Option<&'a AdoptScanView>,
    plan: Option<&'a AdoptPlanView>,
    catalog: Catalog,
) -> Element<'a, Message> {
    let Some(scan) = scan else {
        // 扫描还没回来：一行进度文案（不做假进度条）
        return wrapped(
            ui,
            catalog.text(Key::AdoptScanning).to_string(),
            ui.color(Role::OnSurfaceVariant),
        );
    };

    let mut body = column![].spacing(ui.space(Sp::X3)).width(Length::Fill);
    if scan.is_empty() {
        let dir = scan.versions_dir.display().to_string();
        body = body.push(wrapped(
            ui,
            catalog.render(Key::AdoptEmpty, &[("dir", &dir)]),
            ui.color(Role::OnSurfaceVariant),
        ));
    } else {
        body = body.push(label(ui, catalog.text(Key::AdoptSelectLabel)));
        body = body.push(candidate_list(ui, form, scan, catalog));
    }

    body = body.push(frame::divider(ui).map(Message::Page));
    body = body.push(label(ui, catalog.text(Key::AdoptTargetLabel)));
    body = body.push(target_row(ui, form, catalog));
    // 后果原文（契约 §5.2）：共享档 / 版本目录档一被点选就显示 core `target_notes()` 的原文——
    // 这一条来自**零写盘**的自动预检（见 `AdoptMessage::Target` 的落点），不是界面另写的一句。
    if let Some(plan) = plan {
        for note in &plan.notes {
            body = body.push(wrapped(ui, note.clone(), ui.color(Role::OnSurfaceVariant)));
        }
    }
    body = body.push(text_field(
        ui,
        Field::new(form.name.as_str()).label(catalog.text(Key::AdoptNameLabel)),
        |value| adopt(AdoptMessage::Name(value)),
    ));
    body.into()
}

/// 候选列表（可滚，定高：列表再长页脚按钮也留在卡里）。
fn candidate_list<'a>(
    ui: &'a Ui,
    form: &'a Form,
    scan: &'a AdoptScanView,
    catalog: Catalog,
) -> Element<'a, Message> {
    let rows = column(
        scan.candidates
            .iter()
            .map(|candidate| candidate_row(ui, form, candidate, catalog))
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Sp::X1))
    .width(Length::Fill);

    rows.into()
}

/// 一条候选。
fn candidate_row<'a>(
    ui: &'a Ui,
    form: &'a Form,
    candidate: &'a AdoptCandidateView,
    catalog: Catalog,
) -> Element<'a, Message> {
    let selected = form.selected.as_deref() == Some(candidate.version_id.as_str());

    let mut items = column![row![
        frame::text_role(
            ui,
            TypeRole::BodyLarge,
            selected,
            candidate.version_id.as_str(),
            ui.color(Role::OnSurface),
        )
        .map(Message::Page),
        container(status(ui, candidate, selected, catalog))
            .width(Length::Fill)
            .align_x(Alignment::End),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)]
    .spacing(ui.space(Sp::X1))
    .width(Length::Fill);

    // 问题逐条（不折叠）：候选阶段就把「为什么不能导入」说清楚
    for problem in &candidate.problems {
        items = items.push(problem_line(ui, problem));
    }

    // 行体：底色**不在这里**（选中行要跟 hover/pressed 走，见下面 `button` 的样式闭包）。
    // 不可点的行没有交互，静息实色照旧画在这个容器上。
    let radius = ui.radius_dp(ShapeScale::Medium);
    let body = container(items)
        .width(Length::Fill)
        .padding(ui.space(Sp::X2));

    if !candidate.adoptable() {
        // 不可导入的行**不可点**（也就不存在「点了才被告知不行」这条路）
        return body
            .style(move |_theme| container::Style {
                background: row_fill(ui, Interaction::Enabled, selected).map(Background::Color),
                border: Border {
                    radius,
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into();
    }
    // 选中行的底色 = `SecondaryContainer` **叠**状态层（M296）：hover 8% / pressed 10%，
    // 与菜单项 / 列表行 / 导航项同一个入口（`interaction::selected_fill`）——「选中了就不响应」
    // 是死控件，不是 Material。未选中的行背景恒 `None`（逐字节等于 M296 之前）。
    let style = move |_theme: &iced::Theme, status: button::Status| button::Style {
        background: row_fill(ui, Interaction::from_status(status), selected).map(Background::Color),
        text_color: Color::TRANSPARENT,
        border: Border {
            radius,
            ..Border::default()
        },
        shadow: iced::Shadow::default(),
        snap: true,
    };
    button(body)
        .width(Length::Fill)
        .padding(0)
        .style(style)
        .on_press(adopt(AdoptMessage::Select(candidate.version_id.clone())))
        .into()
}

/// 一条候选行的底色（`None` = 不画，透出向导卡片的底）。
///
/// - 未选中：恒 `None`——M296 之前行为**逐字节不变**（那时也是 `selected.then(..)`）；
/// - 选中：`SecondaryContainer` 叠状态层（唯一入口 [`interaction::selected_fill`]），
///   静息就是实色、hover 8%、pressed 10%。
///
/// 抽成一个函数是为了让它**可测**：样式闭包拿不到（`Element` 没有内省），而
/// 「选中项在 hover/pressed 下必须变色」是 M296 的判据之一。
fn row_fill(ui: &Ui, state: Interaction, selected: bool) -> Option<Color> {
    selected.then(|| {
        interaction::selected_fill(
            ui,
            state,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer,
        )
    })
}

/// 候选右侧那枚状态（可导入 / N 个问题 / 已被某实例使用）。
fn status<'a>(
    ui: &'a Ui,
    candidate: &AdoptCandidateView,
    selected: bool,
    catalog: Catalog,
) -> Element<'a, Message> {
    if !candidate.adoptable() {
        let count = candidate.problems.len().to_string();
        return wrapped(
            ui,
            catalog.render(Key::AdoptBlocked, &[("count", &count)]),
            ui.color(Role::Error),
        );
    }
    if let Some(name) = candidate.registered.first() {
        return wrapped(
            ui,
            catalog.render(Key::AdoptRegistered, &[("name", name)]),
            ui.color(Role::OnSurfaceVariant),
        );
    }
    if selected {
        return chip(ui, Chip::current(catalog.text(Key::AdoptReady)));
    }
    wrapped(
        ui,
        catalog.text(Key::AdoptReady).to_string(),
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 三枚目标 chip（缺省档在最前；共享 / 版本目录档必须**明确点选**）。
fn target_row<'a>(ui: &'a Ui, form: &'a Form, catalog: Catalog) -> Element<'a, Message> {
    let chips = AdoptTargetView::ALL
        .into_iter()
        .map(|target| {
            chip(
                ui,
                Chip::filter(
                    target_label(target, catalog),
                    form.target == target,
                    adopt(AdoptMessage::Target(target)),
                ),
            )
        })
        .collect::<Vec<_>>();

    column![
        row(chips).spacing(ui.space(Sp::X2)).width(Length::Fill),
        target_hint(ui, form.target, catalog),
    ]
    .spacing(ui.space(Sp::X1))
    .width(Length::Fill)
    .into()
}

/// 目标档的一句话说明（点选后立刻换）。**不是后果原文**：原文在 Preview 段逐条画
/// `plan.notes`（core `target_notes()`）。
fn target_hint<'a>(ui: &'a Ui, target: AdoptTargetView, catalog: Catalog) -> Element<'a, Message> {
    let key = match target {
        AdoptTargetView::InstanceDir => Key::AdoptTargetInstanceDirHint,
        AdoptTargetView::Shared => Key::AdoptTargetSharedHint,
        AdoptTargetView::VersionDir => Key::AdoptTargetVersionDirHint,
    };
    let color = if target.touches_foreign_dir() {
        ui.color(Role::Error)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };
    wrapped(ui, catalog.text(key).to_string(), color)
}

#[cfg(test)]
mod tests {
    //! 候选行底色的判据（M296）：选中行**也**吃 hover / pressed。

    use super::*;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::theme::Families;

    fn ui(flavor: SchemeFlavor) -> Ui {
        Ui::with_platform(
            flavor,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        )
    }

    /// 未选中的行**永远**不画底（M296 之前也是 `selected.then(..)`）。
    ///
    /// 抓得住：有人为了让「未选中也有反馈」把底色给到未选中行——那会整列变色，
    /// `--screenshot` 的 rest 图 md5 与「未选中态逐字节不变」的判据立刻红。
    #[test]
    fn an_unselected_candidate_row_never_paints_a_background() {
        let ui = ui(SchemeFlavor::Light);
        for state in Interaction::ALL {
            assert_eq!(row_fill(&ui, state, false), None, "{state:?} 未选中不画底");
        }
    }

    /// 选中行：静息实色，hover / pressed 叠状态层，三态两两不同。
    ///
    /// 抓得住：选中分支写回「选中就不叠层」（`hover == pressed == rest` 一起红）。
    /// 抓不住：真画出来的像素——向导没有 `--force-state` 通道（见 M296 报告的「没验的部分」），
    /// 所以这一条只钉颜色求值，图上的证据只有 rest 一张。
    #[test]
    fn a_selected_candidate_row_reacts_to_hover_and_press() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let ui = ui(flavor);
            let fill = |state| row_fill(&ui, state, true).expect("选中行有底");
            let rest = fill(Interaction::Enabled);
            let hover = fill(Interaction::Hovered);
            let pressed = fill(Interaction::Pressed);
            assert_eq!(rest, ui.color(Role::SecondaryContainer));
            assert_ne!(hover, rest, "{flavor:?}：选中 + hover 必须与静息不同");
            assert_ne!(pressed, rest, "{flavor:?}：选中 + pressed 必须与静息不同");
            assert_ne!(hover, pressed, "{flavor:?}：8% 与 10% 必须能区分");
            assert_eq!(hover.a, 1.0, "{flavor:?}：结果必须不透明（D27）");
            assert_eq!(
                hover,
                interaction::selected_fill(
                    &ui,
                    Interaction::Hovered,
                    Role::OnSecondaryContainer,
                    Role::SecondaryContainer
                ),
                "{flavor:?}：叠层必须经唯一入口求值"
            );
        }
    }
}
