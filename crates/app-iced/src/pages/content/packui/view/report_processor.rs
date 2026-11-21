//! 加载器安装步骤那一段的画法（M328）。
//!
//! 版面：段头一行（**中性**：无图标 + 正文色，见 [`section_title`]）→ Java 来源行 → 汇总行
//! → 逐步骤列表。
//!
//! ## 为什么逐步骤一定要 `bounded`
//!
//! Forge / NeoForge 的处理器链常见 8~20 步，而报告块长在内容库页正列的**上方**，整页没有
//! 整页滚动（只有结果列表自己滚，见 `parts::bounded` 的文档）。不限高的话，一个 20 步的包
//! 会把「关闭 / 以包为准重新导入」两个动作顶出屏幕——这正是 M160 出图时实测到的坏法。
//! 现在按 `bounded(ui, rows, len, 3, 132.0)` 收口：3 步以内直接铺开（不套空框），
//! 多出来的给一个 132 px 的滚动区。
//!
//! ## 段头为什么**不**用容忍域那套（M333 P33-5）
//!
//! 这一段的段头原来直接复用 `parts::tolerable_title`（`Warn` 三角 + `Tertiary` 琥珀），与它下面
//! 两条**真告警**（「有 3 个文件没搞到」「这些组件客户端不支持」）长得一模一样 ⇒ 用户把信息段读成
//! 问题段（证据图 `DevDocs/artifacts/m328/processors-zh.png`）。现在改成 [`section_title`]：
//! **不带任何警示字形、用正文前景色**。于是一条通道定了句：琥珀三角只在**容忍域**出现，
//! 「有没有三角」本身就是「是不是问题」——关掉颜色也分得开（没有图标，也没有琥珀色）。
//!
//! ## 这一段还挂一条自证状态（M333 P33-1）
//!
//! 段头那句话走真控件树读不回来（`text` 的状态是 `paragraph::Plain`，字段私有且没有访问器），
//! 所以整段包一层 [`marker::tagged`]：段名与步数进 `Tree::state`，用例顺着 `Tree::new(..)` 就能
//! 读回「这一段真的被 `push` 进结果块了」。抓得住/抓不住见 `packui/marker.rs` 的表。

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::marker::{self, SectionMark};
use super::super::model::ProcessorSection;
use super::parts::bounded;

/// 处理器段（调用方已判过 `Some`）。
pub(super) fn processor_block<'a>(
    ui: &'a Ui,
    section: &'a ProcessorSection,
) -> Element<'a, PageMessage> {
    marker::tagged(
        body(ui, section),
        SectionMark {
            name: marker::PROCESSORS,
            // 0 = 链跑了却一步都没列（下面走 `empty` 那一句）：仍然是「这一段画出来了」。
            rows: section.steps.len(),
        },
    )
}

/// 段头：**中性**（无图标 + 正文色），与容忍域标题故意长得不一样。
///
/// 不复用 `parts::tolerable_title`：那是「这块东西有问题、但还能往下走」的标记（`Warn` + 琥珀）。
/// 这一段讲的是「加载器安装器跑了哪几步」——**信息**，不是问题。
fn section_title(ui: &Ui, title: String) -> Element<'_, PageMessage> {
    frame::text_role_owned(
        ui,
        TypeRole::LabelLarge,
        true,
        title,
        ui.color(Role::OnSurface),
    )
}

fn body<'a>(ui: &'a Ui, section: &'a ProcessorSection) -> Element<'a, PageMessage> {
    let mut block = column![
        section_title(ui, section.title.clone()),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            section.java_line.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            section.summary.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(empty) = &section.empty {
        // 链跑了却一步都没列：说一句，别画成空白（空白看着像渲染坏了）。
        block = block.push(frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            empty.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ));
        return block.into();
    }

    let rows = column(
        section
            .steps
            .iter()
            .map(|step| step_row(ui, step))
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    // 阈值 3 / 高 132 px：与缺文件段（3 / 120）、冲突段（2 / 160）同一套口径。
    block = block.push(bounded(ui, rows, section.steps.len(), 3, 132.0));
    block.into()
}

/// 一条步骤：左列 `1. net.minecraftforge:installertools:1.3.0`，右列结局。
fn step_row<'a>(
    ui: &'a Ui,
    step: &'a super::super::model::ProcessorStepRow,
) -> Element<'a, PageMessage> {
    iced::widget::row![
        // `format!` 出来的串是临时的，必须走 `text_role_owned`（借用版只收 `&'a str`）。
        frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            format!("{}. {}", step.index, step.label),
            ui.color(Role::OnSurface),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            step.note.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .into()
}
