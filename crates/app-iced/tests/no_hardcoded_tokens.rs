//! 门禁：**组件层零硬编码**（色值 / 尺寸）与「不依赖 slint」。
//!
//! 这三条测试会扫本 crate 的 `src/**` 与 `Cargo.toml`，故意写得「宁可错杀」：
//! 将来 reviewer 复跑 `cargo test -p unlucky-app-iced` 就能挡住「顺手写个 #RRGGBB」的提交。
//!
//! 判定口径（与 `component-fidelity-rubric.md` T1/T2、`visual-acceptance-protocol.md` §1 对齐）：
//!
//! - **色值字面量**：`#` + 6 位十六进制（且**至少含一个数字**，这样文档里的 `#RRGGBB` 占位不会被误杀）
//!   或 `Color::from_rgb(0.1, …)` / `color!(0x…)` 这类带数值参数的构造。
//! - **令牌优先**：唯一允许的是 `theme.rs`（适配层），且它也只做 `token → iced` 的搬运。
//! - **尺寸字面量**：`shell.rs` 里 `.width(12.0)` / `.height(34)` / `.size(22)` 一律不许出现
//!   （必须走 `LayoutMetrics` / `TypeStyle` / `Space`）。

use std::path::{Path, PathBuf};

/// 本 crate 根目录（编译期注入，换机器也成立）。
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 递归收集 `dir` 下的 `.rs` 文件。
fn rust_files(dir: &Path, collected: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("读不动 {}：{error}", dir.display()));

    for entry in entries {
        let path = entry.expect("目录项读失败").path();
        if path.is_dir() {
            rust_files(&path, collected);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            collected.push(path);
        }
    }
}

fn sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files(&manifest_dir().join("src"), &mut files);

    assert!(
        files.len() >= 8,
        "只扫到 {} 个源文件：扫描逻辑坏了，这条门禁等于没跑",
        files.len()
    );

    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("读不动 {}：{error}", path.display()));
            (path, text)
        })
        .collect()
}

/// `#RRGGBB` / `#AARRGGBB` 形式的色值字面量（至少含一个数字，避开 `#RRGGBB` 这种占位写法）。
fn hex_color_at(bytes: &[u8], start: usize) -> Option<&str> {
    let digits = bytes.get(start + 1..start + 7)?;
    let all_hex = digits
        .iter()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_whitespace());
    let has_number = digits.iter().any(u8::is_ascii_digit);

    if all_hex && has_number {
        // 后面不能再跟着十六进制字符（否则是更长的 token，不是 6 位色值）
        let next = bytes.get(start + 7).copied();
        if next.is_none_or(|byte| !byte.is_ascii_hexdigit()) {
            return Some("hex 色值字面量（#RRGGBB）");
        }
    }

    None
}

/// 带数值参数的取色构造（`Color::from_rgb(0.1, …)`、`Color::from_rgb8(103, 80, 164)`、`color!(0x12, …)`）。
///
/// **子串匹配要注意前后缀**：`from_rgb(` 匹配不到 `from_rgb8(`，所以 8 bit 那个必须单独列出来
/// （reviewer 造样就是这么绕过去的）。
const NUMERIC_COLOR_CALLS: [&str; 8] = [
    "Color::from_rgb(",
    "Color::from_rgb8(",
    "Color::from_rgba(",
    "Color::from_rgba8(",
    "Color::from_hex(",
    "Color::rgba(",
    "Color::rgb8(",
    "color!(",
];

/// 取色构造后面是不是跟着数值字面量（允许 `Color::from_rgba8(color.r, …)` 这类搬运）。
fn numeric_color_call(text: &str) -> Option<&'static str> {
    for pattern in NUMERIC_COLOR_CALLS {
        let mut from = 0;
        while let Some(found) = text[from..].find(pattern) {
            let after = from + found + pattern.len();
            let next = text[after..].trim_start().chars().next();

            // 参数以数字或 `0x`/`#` 开头才算字面量；`Color::from_rgba8(color.r, …)` 是搬运，放行。
            if next.is_some_and(|ch| ch.is_ascii_digit() || ch == '#') {
                return Some(pattern);
            }

            from = after;
        }
    }

    None
}

/// 结构体字面量取色：`Color { r: 0.4, g: 0.3, b: 0.6, a: 1.0 }`。
///
/// 判据：`Color {` 之后 64 个字符内同时出现 `r:` 与 `g:` 且有数字（`Color { a: 0.0, .. }` 这种
/// 只改 alpha 的写法不会命中，因为它没有 `r:`）。
fn color_struct_literal(text: &str) -> bool {
    let mut from = 0;
    while let Some(found) = text[from..].find("Color {") {
        let after = from + found + "Color {".len();
        let window: String = text[after..].chars().take(64).collect();
        let has_channels = window.contains("r:") && window.contains("g:");
        let has_number = window.chars().any(|ch| ch.is_ascii_digit());

        if has_channels && has_number {
            return true;
        }

        from = after;
    }

    false
}

#[test]
fn no_hardcoded_colors_in_sources() {
    let mut offenders = Vec::new();

    for (path, text) in sources() {
        for (index, line) in text.lines().enumerate() {
            let bytes = line.as_bytes();
            let mut position = 0;

            while let Some(found) = line[position..].find('#') {
                let at = position + found;
                if let Some(kind) = hex_color_at(bytes, at) {
                    offenders.push(format!(
                        "{}:{} {line:?} -> {kind}",
                        path.display(),
                        index + 1
                    ));
                }
                position = at + 1;
            }

            if let Some(pattern) = numeric_color_call(line) {
                offenders.push(format!(
                    "{}:{} {line:?} -> {pattern}",
                    path.display(),
                    index + 1
                ));
            }

            if color_struct_literal(line) {
                offenders.push(format!(
                    "{}:{} {line:?} -> Color {{ r: … }} 结构体字面量",
                    path.display(),
                    index + 1
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "组件层不许出现色值字面量（一律走 unlucky_design 的 Role）：\n{}",
        offenders.join("\n")
    );
}

/// 判断某个布局调用后面是不是直接跟了尺寸字面量；`0`（= 无内边距）放行。
///
/// iced 里尺寸经常被包一层：`.width(Length::Fixed(123.0))`、`.padding(Padding::from([12.0, 8.0]))`、
/// `.size(Size::new(10.0, 10.0))` —— 所以遇到这些包装要**往里剥一层**再判。
fn size_literal(argument: &str) -> Option<String> {
    let mut rest = argument.trim_start();

    for wrapper in ["Length::Fixed(", "Padding::from(", "Size::new("] {
        if let Some(after) = rest.strip_prefix(wrapper) {
            rest = after.trim_start().trim_start_matches(['[', '(']);
            break;
        }
    }

    let number: String = rest
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();

    if number.is_empty() || number.trim_end_matches('.').parse::<f64>() == Ok(0.0) {
        return None;
    }

    Some(number)
}

/// 尺寸字面量守门：`src/shell` + `src/pages` 都扫。
///
/// P2-3（M84 复审）：原先只扫 `shell`，新增的 `pages/` **没被覆盖**（页面里写 `.width(24)` 没人发现）。
/// `pages/layout.rs` 允许有布局常量（它是布局系统的定义点），控件调用点上不许再出现字面量。
#[test]
fn layout_sources_take_every_size_from_tokens() {
    let mut files = Vec::new();
    // P3-3（M90 r2）：`app` 是 M90 新拆出来的模块，也是唯一没被这条守门看过的控件树。
    for dir in ["shell", "pages", "app"] {
        rust_files(&manifest_dir().join("src").join(dir), &mut files);
    }
    assert!(
        files.len() >= 20,
        "shell + pages + app 至少 20 个文件，实际扫到 {}：扫描逻辑坏了，这条门禁等于没跑",
        files.len()
    );

    let mut offenders = Vec::new();

    for path in files {
        let text = std::fs::read_to_string(&path).expect("读布局源文件");

        for (index, line) in text.lines().enumerate() {
            // 文档注释是散文不是代码：里面写 `.width(24)` 是在解释坑，不该判违规
            if line.trim_start().starts_with("//") {
                continue;
            }

            for pattern in [".width(", ".height(", ".size(", ".spacing(", ".padding("] {
                let mut from = 0;
                while let Some(found) = line[from..].find(pattern) {
                    let after = from + found + pattern.len();
                    if let Some(number) = size_literal(&line[after..]) {
                        offenders.push(format!(
                            "{}:{} {line:?} -> {pattern} 后面直接写了数字 {number}",
                            path.display(),
                            index + 1
                        ));
                    }
                    from = after;
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "骨架尺寸必须来自 LayoutMetrics / TypeStyle / Space：\n{}",
        offenders.join("\n")
    );
}

/// 图标语汇守门（视觉复审 P2-3）：`pages/` 里只用**描边**字形。
///
/// 令牌层把 `Play` / `Stop` / `More` 定义成填充字形，其余是描边；页面骨架里混用会让同屏线宽
/// 语汇打架。将来要支持「主操作允许实心图标」，应由 `components/` 统一定义并只留一处。
#[test]
fn pages_use_only_stroked_icons() {
    const FILLED: [&str; 3] = ["IconKind::Play", "IconKind::Stop", "IconKind::More"];

    let mut files = Vec::new();
    rust_files(&manifest_dir().join("src").join("pages"), &mut files);
    assert!(
        files.len() >= 8,
        "pages 至少 8 个文件，实扫 {}",
        files.len()
    );

    let mut offenders = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("读 pages 源文件");
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for kind in FILLED {
                if line.contains(kind) {
                    offenders.push(format!("{}:{} {line:?}", path.display(), index + 1));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "pages 里出现了填充字形图标（令牌层 Play/Stop/More），与同屏描边语汇冲突：
{}",
        offenders.join(
            "
"
        )
    );
}

/// 依赖行里出现 `slint` 就算违规（注释行放过：说明并存期要提到 slint）。
fn manifest_has_slint_dependency(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            !trimmed.starts_with('#') && trimmed.contains("slint")
        })
        .map(|(index, line)| format!("Cargo.toml:{} {line}", index + 1))
        .collect()
}

#[test]
fn this_crate_never_depends_on_slint() {
    let manifest =
        std::fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("读 Cargo.toml");

    let offenders = manifest_has_slint_dependency(&manifest);
    assert!(
        offenders.is_empty(),
        "P4 的 iced 版 crate 不许依赖 slint（D26）：\n{}",
        offenders.join("\n")
    );

    for (path, text) in sources() {
        for (index, line) in text.lines().enumerate() {
            // 只查真代码里的 use/路径，文档里提 slint 是允许的（要解释并存期）
            let code = line.trim_start();
            let is_import = code.starts_with("use ") || code.starts_with("extern crate");
            if is_import && code.contains("slint") {
                panic!("{}:{} 引入了 slint：{line}", path.display(), index + 1);
            }
        }
    }
}

#[test]
fn the_gate_itself_catches_a_planted_violation() {
    // 自检：把违规样本喂给同一套判定，必须被抓住（否则门禁是空转）。
    // **reviewer r1 造样验证过的 5 个漏网样本逐条钉在这里**，退化成「放行」立刻红。

    // 1. `Color::from_rgb8(103, 80, 164)` —— 8bit 版曾因 `from_rgb(` 匹配不到而漏
    assert!(numeric_color_call("Color::from_rgb8(103, 80, 164)").is_some());
    // 2. `iced::Color::from_rgb8(...)` —— 带路径前缀的写法同样要拦
    assert!(numeric_color_call("iced::Color::from_rgb8(103, 80, 164)").is_some());
    // 3. `Color { r: 0.4, g: 0.3, b: 0.6, a: 1.0 }` —— 结构体字面量
    assert!(color_struct_literal(
        "Color { r: 0.4, g: 0.3, b: 0.6, a: 1.0 }"
    ));
    // 4. `.width(Length::Fixed(123.0))` —— 尺寸被 iced 包装一层
    assert_eq!(
        size_literal("Length::Fixed(123.0))").as_deref(),
        Some("123.0")
    );
    // 5. `.padding(Padding::from([12.0, 8.0]))` —— 数组字面量
    assert_eq!(
        size_literal("Padding::from([12.0, 8.0]))").as_deref(),
        Some("12.0")
    );

    // 原有规则不能退化
    assert!(hex_color_at(b"    color: #6750A4,", 11).is_some());
    assert!(hex_color_at(b"// placeholder #RRGGBB here", 15).is_none());
    assert!(numeric_color_call("Color::from_rgb(0.1, 0.2, 0.3)").is_some());
    assert!(numeric_color_call("Color::from_rgba8(color.r, color.g, color.b, color.a)").is_none());
    assert!(numeric_color_call("color!(0x5865F2)").is_some());
    assert!(size_literal("40.0)").is_some());
    assert!(size_literal("0)").is_none());
    assert!(size_literal("metrics.titlebar_height)").is_none());
    assert!(size_literal("Length::Fixed(thickness))").is_none());
    assert!(size_literal("Padding::from([0.0, ui.space(Space::X4)]))").is_none());
    assert!(!color_struct_literal(
        "Color::from_rgba8(color.r, color.g, color.b, 1.0)"
    ));
    assert!(!color_struct_literal("let color = Color { a: 0.5 };"));
    assert!(!manifest_has_slint_dependency("slint = \"1.18\"").is_empty());
    assert!(manifest_has_slint_dependency("# 不得依赖 slint").is_empty());
}

/// 渲染层回归守门（视觉复审 P2-1 的教训）：`indicator_matches_the_reference_56x32` 只测纯函数，
/// 门禁全绿但图上的药丸实际是 88×32 —— 因为 `.center_x(Length::Fill)` 会把容器 width 覆盖成 Fill。
/// 这里直接扫 `nav.rs`：药丸容器**不许**出现 `center_x(` / `center_y(`。
#[test]
fn pill_never_overrides_its_size_with_fill() {
    let nav = manifest_dir().join("src").join("shell").join("nav.rs");
    let text = std::fs::read_to_string(&nav).expect("读 nav.rs");

    for (index, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        for pattern in ["center_x(", "center_y("] {
            assert!(
                !code.contains(pattern),
                "nav.rs:{} 用了 {pattern}——`center_*(Length::Fill)` 会把容器尺寸改成 Fill，                 药丸会被父级宽度覆盖（视觉复审 P2-1）。居中请用父级 `align_x`：{line}",
                index + 1
            );
        }
    }
}
