//! 组件目录的自检门禁（`src/components/**`）。
//!
//! 为什么要有这份：`tests/no_hardcoded_tokens.rs` 只扫**色值**和 `shell/` 的尺寸字面量，
//! 组件库目录是后来才开的——如果不管，五条并行线会各自写回 `.width(24.0)` 与
//! `center_x(Length::Fill)`，M68 那两个形态级错误就会被复制五份。
//!
//! 四条判据（每条对应一个已发生过的错误）：
//!
//! | 判据 | 抓的是什么 | 历史 |
//! |---|---|---|
//! | Q8：禁 `center_x(` / `center_y(` / `.center(` | 容器尺寸被 `Fill` 静默覆盖 | 药丸 56×32 → 88×32 |
//! | Q9：图标必须经 `layout::fixed` / `centered_fixed` | 字形被 Button 的 Fill 撑满 | 24 的字形渲染成 46px |
//! | T2：禁尺寸字面量 | 组件里重复定义令牌值 | rubric T2 |
//! | 400 行 | 单文件上限（AGENTS.md §3-1） | `guards.yml` 的 file-size job |
//!
//! 扫描对象是**源码文本**而不是布局结果——这不是偷懒：布局结果要真窗口（CI 无桌面），
//! 而「把尺寸交给 `center_*`」这个错误 100% 出现在源码形态里（M68 就是这样被抓的）。
//! 像素级几何另有 `gallery::probe` 对着真截图量。

use std::path::{Path, PathBuf};

/// 单文件行数上限（AGENTS.md §3-1）。
const MAX_LINES: usize = 400;

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn components_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("components")
}

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

/// 全部组件源文件（`mod.rs` 也要扫：它是共享地基，最容易混进字面量）。
fn sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files(&components_dir(), &mut files);
    files.sort();

    assert!(
        files.len() >= 15,
        "只扫到 {} 个组件源文件：扫描逻辑坏了，门禁等于没跑",
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

/// 去掉 `#[cfg(test)]` 之后的全部内容：测试里写参考值是**应该的**（12b 的 40/24/20
/// 必须在测试里以字面量出现，否则测不出偏离）。
fn product_part(text: &str) -> &str {
    match text.find("#[cfg(test)]") {
        Some(at) => &text[..at],
        None => text,
    }
}

/// 代码行（跳过注释行：文档里写 `.center_x(` 是在解释坑，不该判违规）。
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            None
        } else {
            Some((index + 1, line))
        }
    })
}

#[test]
fn q8_no_center_shortcuts_that_swallow_fixed_sizes() {
    let mut offenders = Vec::new();

    for (path, text) in sources() {
        for (line_no, line) in code_lines(product_part(&text)) {
            for pattern in ["center_x(", "center_y(", ".center("] {
                if line.contains(pattern) {
                    offenders.push(format!(
                        "{}:{line_no} {line}\n    → {pattern} 会把容器尺寸改成 Fill（Q8），\
                         居中请用父级 align_x 或 layout::centered_fixed",
                        path.display()
                    ));
                }
            }
        }
    }

    assert!(offenders.is_empty(), "Q8 违规：\n{}", offenders.join("\n"));
}

#[test]
fn q9_icons_never_reach_a_button_unwrapped() {
    let mut offenders = Vec::new();

    for (path, text) in sources() {
        let product = product_part(&text);
        // 允许两种写法：同一行里已经包在 layout:: 里；或它是 `layout::fixed(` 多行调用的实参
        // （图标常作为第 3 个参数出现在后两行，所以往回看 4 行）。
        let mut window: Vec<&str> = Vec::new();
        for (line_no, line) in code_lines(product) {
            if line.contains("icons::view") {
                let wrapped =
                    line.contains("layout::fixed") || line.contains("layout::centered_fixed");
                let continued = window.iter().rev().any(|prev| {
                    prev.contains("layout::fixed") || prev.contains("layout::centered_fixed")
                });
                if !wrapped && !continued {
                    offenders.push(format!(
                        "{}:{line_no} {line}\n    → 图标必须经 layout::fixed / centered_fixed 包一层（Q9）",
                        path.display()
                    ));
                }
            }
            if !line.trim().is_empty() {
                window.push(line);
                if window.len() > 4 {
                    window.remove(0);
                }
            }
        }
    }

    assert!(offenders.is_empty(), "Q9 违规：\n{}", offenders.join("\n"));
}

/// 布局调用后面直接跟数字 → 判违规（`0` 与 `0.0` 放行：0 = 无内边距/无描边）。
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

#[test]
fn t2_no_size_literals_in_components() {
    let mut offenders = Vec::new();

    for (path, text) in sources() {
        for (line_no, line) in code_lines(product_part(&text)) {
            for pattern in [".width(", ".height(", ".size(", ".spacing(", ".padding("] {
                let mut from = 0;
                while let Some(found) = line[from..].find(pattern) {
                    let after = from + found + pattern.len();
                    if let Some(number) = size_literal(&line[after..]) {
                        offenders.push(format!(
                            "{}:{line_no} {line} -> {pattern} 后面写了数字 {number}",
                            path.display()
                        ));
                    }
                    from = after;
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "组件里不许有尺寸字面量（走 LayoutMetrics / Space / ComponentMetrics）：\n{}",
        offenders.join("\n")
    );
}

/// 单文件 ≤400 行：**扫整个 `src/`**，不只 components。
///
/// M90 r2（P1-2）的教训：这条守门原来只遍历 `components/`，于是 `pages/harness.rs` 悄悄长到
/// 402 行没人拦，直到 `guards.yml` 的 file-size job 在 CI 上判红才发现。`guards.yml` 的口径是
/// **全仓**（`git ls-files '*.rs' '*.md' …`），这里至少把本 crate 的 `src/**` 对齐：
/// 谁在 `app/` / `pages/` / `shell/` 把文件推过线，`cargo test` 就先红，不用等 CI。
#[test]
fn every_source_file_stays_under_the_line_limit() {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    assert!(
        files.len() >= 40,
        "src/ 至少 40 个 .rs，实际 {}：扫描逻辑坏了，这条门禁等于没跑",
        files.len()
    );

    let mut offenders = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("读源文件");
        let lines = text.lines().count();
        if lines > MAX_LINES {
            offenders.push(format!(
                "{}: {lines} 行（上限 {MAX_LINES}）",
                path.display()
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "单文件 ≤400 行（AGENTS.md §3-1，guards.yml 按全仓判红）：\n{}",
        offenders.join("\n")
    );
}

/// 四个家族目录必须存在且结构完整。
///
/// **这里刻意不再限制「pub fn 有几个」**：M76 往 `selection/` 里加 Checkbox/Radio 是它的本职工作，
/// 而家族线被明确要求不许改 `gate.rs`——把「桩还在不在」写成 pub fn 计数，等于给 M76 埋一个
/// 它无权拆的地雷（reviewer P1-C）。越界与否由 merge gate 的 scope 判定（各家族目录只归对应
/// mission），门禁只管「地基还在」：
///
/// - 四个家族目录各有 `mod.rs`；
/// - `components/mod.rs` 仍声明这五个家族 + gallery（谁把声明删了，这里先红）。
#[test]
fn the_five_families_are_declared_and_present() {
    let declared =
        std::fs::read_to_string(components_dir().join("mod.rs")).expect("读 components/mod.rs");

    for family in ["selection", "inputs", "feedback", "navigation"] {
        let dir = components_dir().join(family);
        let file = dir.join("mod.rs");
        assert!(file.is_file(), "家族目录 {} 缺 mod.rs", dir.display());
        assert!(
            declared.contains(&format!("pub mod {family};")),
            "components/mod.rs 少了 `pub mod {family};` 声明"
        );
    }
    assert!(
        declared.contains("pub mod gallery;") && declared.contains("pub mod buttons;"),
        "components/mod.rs 的声明被动了"
    );
}

/// gallery 的五个家族文件必须都在（`mod.rs` 声明了它们，缺一个就编译不过——
/// 这条在并行线删文件/改名时给出人话，而不是一堆 `unresolved module`）。
#[test]
fn gallery_keeps_one_file_per_family() {
    for family in ["buttons", "selection", "inputs", "feedback", "navigation"] {
        let path = components_dir()
            .join("gallery")
            .join(format!("{family}.rs"));
        assert!(
            path.is_file(),
            "gallery 缺 {}（本文件结构是并行地基）",
            path.display()
        );

        let text = std::fs::read_to_string(&path).expect("读 gallery 家族文件");
        assert!(
            text.contains("fn section"),
            "{} 必须提供 section()（gallery/mod.rs 调它）",
            path.display()
        );
    }
}

/// 门禁自身不能空转：造几个违规样本喂给判据。
#[test]
fn the_gate_catches_planted_violations() {
    assert_eq!(
        size_literal("Length::Fixed(40.0))").as_deref(),
        Some("40.0")
    );
    assert_eq!(size_literal("0)").as_deref(), None);
    assert_eq!(
        size_literal("ui.metrics().control_height)").as_deref(),
        None
    );
    assert!(product_part("a\n#[cfg(test)]\n.width(40.0)\n")
        .find(".width(40.0)")
        .is_none());
    assert!(product_part("a\n.width(40.0)\n")
        .find(".width(40.0)")
        .is_some());
    // Q8 判据本身
    for code in ["x.center_x(Length::Fill)", "x.center(Length::Fill)"] {
        assert!(
            code.contains("center_x(") || code.contains(".center("),
            "判据写坏了"
        );
    }
}
