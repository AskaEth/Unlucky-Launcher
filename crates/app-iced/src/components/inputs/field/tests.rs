//! field 的单测（从 `field.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;
use unlucky_design::color::Role;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

fn build(field: Field<'_, u8>, flavor: SchemeFlavor) -> Element<'_, u8> {
    let ui = Box::leak(Box::new(ui(flavor)));
    text_field(ui, field, |_| 0_u8)
}

#[test]
fn every_variant_and_state_builds() {
    for variant in FieldVariant::ALL {
        for state in Interaction::ALL {
            for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
                let field = Field::new("实例一")
                    .variant(variant)
                    .label("名称")
                    .placeholder("请输入")
                    .supporting("辅助说明")
                    .leading(IconKind::Folder)
                    .trailing(IconKind::Close)
                    .state(state);
                let _: Element<'_, u8> = build(field, flavor);
            }
        }
    }
}

/// 9.4：聚焦 2dp `Primary`、hover 1dp `OnSurface`、静息 Outlined 1dp `Outline`（Filled 用 OnSurfaceVariant）。
#[test]
fn outline_colors_and_widths_follow_the_state() {
    let ui = ui(SchemeFlavor::Light);
    let rest = palette(
        &ui,
        Base::CANVAS,
        FieldVariant::Outlined,
        Interaction::Enabled,
        false,
    );
    assert_eq!(rest.line, ui.color(Role::Outline));
    assert_eq!(rest.stroke, 1.0);
    assert_eq!(rest.fill, None, "Outlined 无容器色");

    let hover = palette(
        &ui,
        Base::CANVAS,
        FieldVariant::Outlined,
        Interaction::Hovered,
        false,
    );
    assert_eq!(hover.line, ui.color(Role::OnSurface));
    assert_eq!(hover.stroke, 1.0);

    let focus = palette(
        &ui,
        Base::CANVAS,
        FieldVariant::Outlined,
        Interaction::Focused,
        false,
    );
    assert_eq!(focus.line, ui.color(Role::Primary));
    assert_eq!(focus.stroke, 2.0, "FocusOutlineWidth = 2dp");
    assert_eq!(focus.label, ui.color(Role::Primary), "FocusLabelColor");

    // A1 没有 Pressed token：按 hover 处理（D11），不是漏做
    assert_eq!(
        palette(
            &ui,
            Base::CANVAS,
            FieldVariant::Outlined,
            Interaction::Pressed,
            false
        ),
        hover
    );
}

/// 9.7：错误态线/标签/辅助全 `Error`，线宽 2dp；错误优先于聚焦。
#[test]
fn error_beats_focus_and_paints_everything_error() {
    let ui = ui(SchemeFlavor::Light);
    let error = palette(
        &ui,
        Base::CANVAS,
        FieldVariant::Outlined,
        Interaction::Focused,
        true,
    );
    assert_eq!(error.line, ui.color(Role::Error));
    assert_eq!(error.label, ui.color(Role::Error));
    assert_eq!(error.supporting, ui.color(Role::Error));
    assert_eq!(error.stroke, 2.0, "错误线宽 2dp");
    assert_eq!(error.input, ui.color(Role::OnSurface), "ErrorInputColor");
}

/// 9.8：禁用内容 38%、Outlined 描边 12%、Filled 容器 12%（官方 4%，令牌缺口 → D10）。
#[test]
fn disabled_uses_the_token_alphas() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        let container = interaction::disabled_colors(&ui, Base::CANVAS).0;

        let filled = palette(
            &ui,
            Base::CANVAS,
            FieldVariant::Filled,
            Interaction::Disabled,
            false,
        );
        assert_eq!(filled.fill, Some(container));
        assert_eq!(
            filled.input,
            interaction::disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurface, true)
        );

        let outlined = palette(
            &ui,
            Base::CANVAS,
            FieldVariant::Outlined,
            Interaction::Disabled,
            false,
        );
        assert_eq!(outlined.fill, None);
        assert_eq!(outlined.line, container, "DisabledOutlineOpacity = 0.12");
        assert_eq!(
            outlined.input,
            interaction::disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurface, false)
        );
        assert_ne!(outlined.input, ui.color(Role::OnSurface), "禁用不许满档");
    }
}

/// Filled 有容器色、Outlined 没有；两者圆角都取 `Component::TextField`（4）。
#[test]
fn filled_carries_the_container_role() {
    let ui = ui(SchemeFlavor::Light);
    assert_eq!(
        palette(
            &ui,
            Base::CANVAS,
            FieldVariant::Filled,
            Interaction::Enabled,
            false
        )
        .fill,
        Some(ui.color(Role::SurfaceContainerHighest)),
        "FilledTextFieldTokens.ContainerColor"
    );
    assert_eq!(FieldGeometry::of(&ui, true, None).shape.dp(), 4.0);
}

/// 本文件所在模块的源码（源码结构断言用，`pages::layout::source` 的同一条纪律）。
const FIELD_SOURCE: &str = include_str!("../field.rs");

/// P18 / M252 判据 ④-a：`on_submit` 是**可选**的——不接就没有提交消息。
///
/// 抓得住：`Field::new(..)` 这条链（其余构造器全接上）给出 `None` ⇒ `text_field` 交给
/// `text_input` 的是 `on_submit_maybe(None)`，回车**什么消息都不发**（全站既有调用点因此逐位
/// 不变）；接了之后取到的正是那条消息（回调没被构造器吞掉）。
/// 抓不住：回车键真的走到控件（iced 的键路径在 `Widget::update` 里，`Element` 没有内省接口，
/// 见 `pages::layout::source` 的模块文档；本仓也造不出 `iced::Renderer` 的测试替身）。
#[test]
fn submit_is_opt_in() {
    let plain: Field<'_, u8> = Field::new("实例一")
        .variant(FieldVariant::Outlined)
        .label("名称")
        .placeholder("请输入")
        .supporting("辅助说明")
        .leading(IconKind::Folder)
        .trailing(IconKind::Close)
        .error()
        .state(Interaction::Enabled)
        .secure(false)
        .base(Base::CANVAS);
    assert_eq!(
        plain.submit_message(Interaction::Enabled),
        None,
        "没调 `.on_submit` ⇒ 回车不发消息（既有调用点的行为）"
    );

    let wired: Field<'_, u8> = Field::new("实例一").on_submit(|| 7_u8);
    assert_eq!(
        wired.submit_message(Interaction::Enabled),
        Some(7),
        "接上的回调要被原样取出来"
    );
    // 禁用态里回车不提交（与 `on_input` 同一条规矩：点不动的框也不该能提交）
    assert_eq!(
        wired.submit_message(Interaction::Disabled),
        None,
        "禁用字段不许回车提交"
    );
}

/// 回归（T27 反例 ①）：`text_field` 必须把 `Field` 上的提交回调**转发**给 `text_input`。
///
/// iced 的 `Element` 没有内省接口，所以这条守的是**源码结构**（本仓既有同一条纪律，见
/// `pages::layout::source` 的模块文档）。三条断言合起来才等于「转发」：有一**行代码**在调
/// `on_submit_maybe`、它的实参不是写死的 `None`、消息确实来自 `Field::submit_message`（唯一读取口）。
///
/// **判据是「代码行」，不是「文本里还有那个字符串」**（M252 r1 的 P1-1）：断言走
/// `source::code_line`，注释先被剥掉。所以下面这些改法都会红：
/// - 删掉 `.on_submit_maybe(..)` 那一行；
/// - 把它**注释掉**（`//` 行注释，或 `/*` 独占一行的块注释）；
/// - 改成 `.on_submit_maybe(None)` ——**即使同时**把 `let submit` 写成 `let _submit` 把 rustc 的
///   unused-variable 兜底也绕掉（复审实测过：那样测试与 lint 两条线会同时静默）。
///
/// 抓不住：`text_input` 内部的键处理（iced 的实现）；等价重构（把值先赋给别的名字再传）——
/// 那要同时改 needle，见 `pages::layout::source::code` 的边界一节。
#[test]
fn text_field_forwards_the_submit_callback() {
    let body = crate::pages::layout::source::function_body(FIELD_SOURCE, "pub fn text_field");
    let call =
        crate::pages::layout::source::code_line(body, ".on_submit_maybe(").unwrap_or_else(|| {
            panic!(
                "`text_field` 里没有一行**代码**在调 `on_submit_maybe`（注释掉也算没有）：\n{body}"
            )
        });
    assert!(
        !call.contains("None"),
        "提交消息被写死成 `None`（回车什么都不发，且功能静默失效）：{call}"
    );
    assert!(
        crate::pages::layout::source::code_text(body).contains("submit_message("),
        "提交消息必须从 `Field::submit_message()` 取（唯一读取口；注释里写不算），实际函数体：\n{body}"
    );
}

/// 回归（T27 反例 ②）：全站**只有那两处**（存档改名 / 强调色）接 `Field::on_submit`。
///
/// 为什么要扫源码：`text_field` 是全站共用控件，加了一个可选参数之后最容易出的错是
/// **顺手把别的调用点也接上**——那会让一个「回车本来什么都不做」的输入框开始提交。
/// 这条不靠「我看了一遍所有调用点」：白名单对不上就红；新增一处接线必须来这里登记，
/// 这份摩擦是故意的。
///
/// 抓得住：任何一处**新增/删除/注释掉**接在构建链上的 `.on_submit(..)`（本仓的链一律一行一条；
/// 注释先被 [`scan`] 剥掉，所以 `/*` 独占一行的块注释也算删除）。
/// 抓不住：写在别处的同名文本（注释/字符串里的例子）、写成同一行的接线，也不管那两处接的
/// 消息对不对（由 `pages` 侧各自的用例钉）。
#[test]
fn submit_is_wired_only_at_the_two_call_sites() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut wired = Vec::new();
    scan(&root, &root, &mut wired);
    wired.sort();
    assert_eq!(
        wired,
        vec![
            "pages/appearance/section/accent.rs".to_owned(),
            "pages/instances/saves/rows.rs".to_owned(),
        ],
        "`Field::on_submit` 的调用点白名单变了（新接线要在这里登记）"
    );
}

/// 递归找 `dir` 下**有一行代码以 `.on_submit(` 开头**的 `.rs`，记下相对 `root` 的路径（`/` 分隔）。
///
/// 谓词与三处接线守卫同一个（`source::has_code_line`）：注释与字符串里出现的 `.on_submit(`
/// 例子不算接线。代价是**写成同一行的接线扫不到**——本仓的构建链一律一行一条。
///
/// **测试文件跳过**（[`is_test_path`]）：守卫自己的 fixture 里有 `.on_submit(` 的样本（本文件、
/// `pages/layout/source/code/tests.rs`），那不是调用点；测试也不会去接生产 UI。
fn scan(root: &std::path::Path, dir: &std::path::Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan(root, &path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") && !is_test_path(&path) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if crate::pages::layout::source::has_code_line(&text, ".on_submit(") {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                found.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// 这个路径是不是**测试模块**的文件：任一组件叫 `tests` / `tests.rs`，或文件名以 `_tests.rs` 收尾。
///
/// 本仓的测试文件就这几种形状（`*/tests.rs`、`*/tests/*.rs`、`*_tests.rs`），全部由
/// `#[cfg(test)]` 或 `tests/` 目标挂载；生产文件的 inline `mod tests` 不在这里判（那些文件本身
/// 也可能是真调用点，例如 `section/accent.rs`）。
fn is_test_path(path: &std::path::Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "tests" || name == "tests.rs" || name.ends_with("_tests.rs")
    })
}
