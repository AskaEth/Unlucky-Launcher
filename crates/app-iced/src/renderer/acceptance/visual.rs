//! `[font]` 的回退链与 `[caps]` 的平台能力——两件「看一眼就知道对不对」的事。
//!
//! 这里的两个函数都是**纯格式化**（除了 `[caps]` 读一次 `current_exe()`，那是调用方传进来的）：

use std::path::Path;

use unlucky_design::typography::{FontFamily, Platform as FontPlatform};

// ---------------------------------------------------------------- [font]

/// `[font] chain`：令牌里三条栈的**声明**顺序（首选 → 兜底 → 通用族名）。
///
/// 与 `fonts` 打的 `[font] platform=…` 互补：那行是「实际落到哪个族」，这行是「本来想落哪些」。
/// 缺字体（方块字）时要看的是**两者的差**：链里有 `"PingFang SC"` 但 `text=` 落到别的族，
/// 就是这台机器没有苹方。
#[must_use]
pub fn font_chain_line(platform: FontPlatform) -> String {
    let stacks = [
        ("display", FontFamily::Display),
        ("text", FontFamily::Text),
        ("mono", FontFamily::Mono),
    ]
    .map(|(name, family)| {
        let chain = platform
            .font_stack(family)
            .iter()
            .map(|face| format!("\"{}\"", face.family))
            .collect::<Vec<_>>()
            .join(">");
        format!("{name}={chain}")
    })
    .join(" ");

    format!(
        "[font] chain platform={} {stacks}（令牌声明的回退链；实际落点见上面 [font] platform=…）",
        platform.name()
    )
}

/// 打一行 [`font_chain_line`]（调用点只留一句）。
pub fn report_font_chain(platform: FontPlatform) {
    eprintln!("{}", font_chain_line(platform));
}

// ---------------------------------------------------------------- [caps]

/// `[caps]`：平台能力里**本进程能自己回答**的那几条。
///
/// 半观测半请求，字段名照实说：
/// - `bundle` / `menu_bar_name`：观测（从可执行文件路径读）；
/// - `transparent` / `decorations` / `resizable`：本进程给窗口的设置（**请求值**，即终端用户
///   实际跑的那个形态；`decorations=false` ⇒ 三键是自绘的）；
/// - `controls=self-drawn`：由 `decorations=false` 推出来的结论（自绘三键在**右上角**，
///   与 macOS 惯例的左上相反——这条正是「三键位置怪不怪」的机器答案，位置本身由脚本在截图上
///   做像素定位）。
#[must_use]
pub fn caps_line(
    exe: Option<&Path>,
    transparent: bool,
    resizable: bool,
    decorations: bool,
) -> String {
    let (is_bundle, name) = bundle_of(exe);
    format!(
        "[caps] bundle={} menu_bar_name=\"{name}\" menu_bar_source={} controls={} \
         transparent={transparent} resizable={resizable} decorations={decorations}",
        if is_bundle { "yes" } else { "no" },
        if is_bundle { "bundle" } else { "exe-stem" },
        if decorations { "native" } else { "self-drawn" },
    )
}

/// 可执行文件 → (是不是 `.app` 包, 菜单栏会显示的名字)。
///
/// macOS 上非 bundle 的二进制，菜单栏 App 名就是**可执行文件名**；打成 `.app` 时是
/// `Foo.app` 里的 `Foo`。这条从路径读得出来，所以「菜单栏名字对不对」**不需要问人**。
#[must_use]
pub fn bundle_of(exe: Option<&Path>) -> (bool, String) {
    let Some(exe) = exe else {
        return (false, "<未知>".to_owned());
    };

    // `Foo.app/Contents/MacOS/foo` 里的 `Foo`：路径里出现以 `.app` 结尾的一段就是 bundle。
    for component in exe
        .components()
        .filter_map(|part| part.as_os_str().to_str())
    {
        if let Some(name) = component.strip_suffix(".app") {
            return (true, name.to_owned());
        }
    }

    let stem = exe
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("<未知>");
    (false, stem.to_owned())
}

/// 打一行 [`caps_line`]（调用点只留一句）。
pub fn report_caps(exe: Option<&Path>, transparent: bool, resizable: bool, decorations: bool) {
    eprintln!("{}", caps_line(exe, transparent, resizable, decorations));
}
