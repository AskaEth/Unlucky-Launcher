//! 实例详情页的「启动参数」区（M157）。
//!
//! | 控件 | 语义 | 消息 |
//! |---|---|---|
//! | JVM 参数（多行） | 实例层，与全局 `java.extra_args` **叠加**（实例段最高优先级） | [`Message::Jvm`] |
//! | 游戏参数（多行） | 实例层，追加在命令行最后 | [`Message::Game`] |
//! | 最大内存（单行数字） | 实例层；留空 = 跟随全局默认 | [`Message::Memory`] |
//! | Java 可执行文件（单行） | 实例层；留空 = 跟随全局 → 自动挑 | [`Message::Java`] |
//! | 图形后端（三态 chips） | `default` / `opengl` / `vulkan`；**版本不够时只灰化后两档**（`default` 始终可点）+ 提示 | [`Message::Backend`] |
//!
//! 两条判据刻意不在这里重算：
//! 1. **能不能用 `--graphicsBackend`** 来自 [`LaunchFacts::allowed`]（core 的 `graphics_backend_allowed`，
//!    presenter 的 host 已经算好）——UI 不复制版本号比较；
//! 2. **生效值**（内存/Java/后端）同样来自 facts，表单里画的是实例层原文。
//!
//! 文案全走 [`Texts`]（守门见 `instances/tests/i18n.rs` 的白名单与死 key 扫描）。

use iced::widget::{column, row};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::app::components::buttons::{self, Button};
use crate::app::components::inputs::{text_field, Field};
use crate::app::components::selection::{chip, Chip};

use crate::theme::Ui;

use super::super::frame::BASE;
use super::super::model::LaunchFacts;
use super::super::Message as PageMessage;
use super::state::{LaunchForm, Message};
use super::Texts;

/// 页内消息 → 页面消息（本文件的控件树统一画 `pages::Message`）。
fn page(message: Message) -> PageMessage {
    PageMessage::Instances(message)
}

/// 「启动参数」区（在详情列的正文段之后、动作行之前）。
pub(super) fn section<'a>(
    ui: &'a Ui,
    facts: &'a LaunchFacts,
    form: &'a LaunchForm,
    texts: Texts,
) -> Element<'a, PageMessage> {
    // 段间距 X2（8dp）：卡内 5 个字段 + 三态 + 动作行在 424 宽下已经很长，
    // 用 X3 会让详情列更长一屏；行内间距仍是 X1（4dp），读起来不挤。
    let mut body = column![frame_title(ui, texts.launch_title())]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill);

    body = body.push(args_field(
        ui,
        texts.launch_jvm(),
        texts.launch_jvm_hint(),
        form.jvm.as_str(),
        |value| page(Message::Jvm(value)),
    ));
    body = body.push(args_field(
        ui,
        texts.launch_game(),
        texts.launch_game_hint(),
        form.game.as_str(),
        |value| page(Message::Game(value)),
    ));
    body = body.push(text_field_with_hint(
        ui,
        texts.launch_memory(),
        form.memory.as_str(),
        super::super::frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            texts.launch_memory_hint(facts.memory_effective_mb),
            ui.color(Role::OnSurfaceVariant),
        ),
        |value| page(Message::Memory(value)),
    ));
    body = body.push(text_field_with_hint(
        ui,
        texts.launch_java(),
        form.java.as_str(),
        super::super::frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            texts.launch_java_hint().to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
        |value| page(Message::Java(value)),
    ));
    body = body.push(backend_row(ui, facts, form, texts));
    body = body.push(actions_row(ui, texts));

    // 与相邻**表单卡**同一套卡壳（`frame::card`：内距 `card_padding` 20 + 卡片圆角 16 + 1px 描边）。
    // 注意元信息卡不在这条线上：它们现在是 ELEVATED CARD（无描边、内距 20、值 12px，见 `surfaces::elevated`）。
    super::super::frame::card(ui, body)
}

/// 卡片标题：**与信息卡的标签同款**（`LabelMedium` 不加粗、`OnSurfaceVariant`），
/// 复审 ③⑤ 抓的就是这里（原来用了 `emphasized = true`，比同屏其它标签重一档）。
fn frame_title<'a>(ui: &'a Ui, title: &'a str) -> Element<'a, PageMessage> {
    super::super::frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        title,
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 一段参数输入 + 一行提示（单行受控输入；参数按空白切分，与 core 的 `tokenize` 同口径）。
fn args_field<'a>(
    ui: &'a Ui,
    label: &'a str,
    hint: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> PageMessage + 'a,
) -> Element<'a, PageMessage> {
    column![
        text_field(ui, Field::new(value).label(label).base(BASE), on_input),
        super::super::frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            hint,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// 单行输入 + 一行提示（提示是**拥有的**字符串，所以不能走 `Field::supporting` 的借用口径）。
fn text_field_with_hint<'a>(
    ui: &'a Ui,
    label: &'a str,
    value: &'a str,
    hint: Element<'a, PageMessage>,
    on_input: impl Fn(String) -> PageMessage + 'a,
) -> Element<'a, PageMessage> {
    column![
        text_field(ui, Field::new(value).label(label).base(BASE), on_input),
        hint,
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// 三态后端的「不下发」档（`GraphicsBackend::Default` 的稳定 id）。
///
/// UI 侧不依赖 core（分层：UI → presenter → core），所以在页面里落一个常量；
/// 词表本身仍来自 presenter 快照（[`LaunchFacts::options`]），这里只认「哪一个是安全档」。
const BACKEND_DEFAULT: &str = "default";

/// 某一档 chip 在门限不满足时**是否仍可点**。
///
/// 抽成纯函数是为了让它可断言（`Element` 没有内省接口，画出来的禁用态测不到；
/// 见 `instances/tests/launch_args.rs`）。
pub(crate) fn backend_chip_enabled(allowed: bool, id: &str) -> bool {
    allowed || id == BACKEND_DEFAULT
}

/// 三态后端：**版本不够时只灰掉 `opengl` / `vulkan`**，`default` 始终可点。
///
/// 为什么 `default` 不许一起灰：它是「不下发 `--graphicsBackend`」的安全档，写它永远不会被
/// core 的版本门限拒绝。一起灰掉的话，用户手改实例文件、或在 CLI 里给旧版本实例写下
/// `graphicsBackend: vulkan` 之后，UI 上就**回不到 `default`**——唯一的出路是「恢复跟随全局」，
/// 而那会连带清空 JVM / 游戏参数 / 内存 / Java 四项（M157 复审 P3）。
fn backend_row<'a>(
    ui: &'a Ui,
    facts: &'a LaunchFacts,
    form: &'a LaunchForm,
    texts: Texts,
) -> Element<'a, PageMessage> {
    let allowed = facts.allowed;
    let mut chips = row![].spacing(ui.space(Space::X2));
    for id in &facts.options {
        let label = texts.backend_label(id).unwrap_or(id.as_str());
        let selected = form.backend == *id;
        let spec = Chip::filter(label, selected, page(Message::Backend(id.clone())));
        let spec = if backend_chip_enabled(allowed, id) {
            spec
        } else {
            spec.disabled()
        };
        // M237：启动参数区是一张 `frame::card`，chip 的禁用配色（12% 容器 / 38% 内容）叠卡底。
        chips = chips.push(chip(ui, spec.base(BASE)));
    }

    let mut meta = column![
        super::super::frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            texts.launch_backend(),
            ui.color(Role::OnSurface),
        ),
        chips,
        super::super::frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            texts.launch_effective(&facts.backend_effective),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if !allowed {
        // 灰化 + 明确提示：既不说「已生效」，也不静默吞掉用户的选择。
        meta = meta.push(super::super::frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            texts.launch_backend_gated(&facts.mc_version, &facts.min_version),
            ui.color(Role::Error),
        ));
    }
    meta.into()
}

fn actions_row<'a>(ui: &'a Ui, texts: Texts) -> Element<'a, PageMessage> {
    row![
        buttons::filled(
            ui,
            Button::new(texts.launch_save(), page(Message::Save)).base(BASE)
        ),
        buttons::text(
            ui,
            Button::new(texts.launch_reset(), page(Message::Reset)).base(BASE)
        ),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}
