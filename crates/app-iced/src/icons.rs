//! 令牌图标（`unlucky_design::icons` 的 24 网格 SVG path）→ iced `svg` 控件。
//!
//! ## 为什么走 SVG 而不是自绘 canvas
//!
//! 令牌里存的是 **SVG path 字符串**（含 `A` 弧线命令），自绘要自己写 path parser + 弧线细分，
//! 那是一份将来只有自己在维护的代码；iced 的 `svg` 控件已经带 usvg 解析与按 id 缓存的位图。
//! 代价是二进制多一份 resvg/usvg（体积基线里会单列出来）。
//!
//! ## 描边语义（`design-令牌约定.md` §4）
//!
//! 令牌的 `Icon::stroke_width(render_px)` 已经把「视觉 1.5px」折算成 **viewBox 单位**
//! （`1.5 × 24 / render_px`），本模块**不重算也不手抄常数**。
//!
//! ## ⚠️ 渲染尺寸**不是**一个全局值（M86 收口）
//!
//! **盒子多宽就按多宽烘焙**，规则只有两条：
//!
//! | 盒尺寸 | 用哪个入口 |
//! |---|---|
//! | **等于** [`RENDER_PX`]（24） | [`view`]（导航、卡片头、表单、IconButton…） |
//! | **不等于** 24（按钮 18 / chip 18 / switch 滑块 16 / 标题栏 caption 20） | **[`view_at`]（必须）** |
//!
//! 尺寸只有一个来源：`unlucky_design::icons` 的令牌（[`IconSize`] /
//! [`unlucky_design::icons::TITLEBAR_CAPTION_ICON_PX`]）——本文件里不许出现 `18.0` / `20.0` / `24.0`
//! 这类字面量，也不许把某个组件的尺寸「凑」进 [`RENDER_PX`]。
//!
//! **为什么必须按盒烘焙**：`stroke-width` 是 viewBox 单位，同一个 viewBox 线宽在更小的盒子里
//! 视觉更细。把 24px 烘焙的位图塞进 18px 盒子，视觉线宽会从 1.5px 掉到 **1.125px**
//! （复审 r1 实测 1.04~1.25px，而同屏 caption 的 20px 走 [`view_at`] 实测正好 1.50px）。
//! 所以：**每个用到的尺寸各缓存一份**，缓存键是 `(图标种类, 渲染尺寸)`——但「缓存支持多尺寸」
//! 不等于「调用点都用上了」，**不等于 24 的盒子一律走 [`view_at`]** 才是这条的落点。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use iced::widget::svg;
use iced::Color;

use unlucky_design::icons::{IconKind, IconPaint, IconSize};

/// 图标缓存键里的尺寸精度（px × 100 取整），避免拿 f32 做哈希键。
const PX_KEY_SCALE: f32 = 100.0;

/// 骨架期图标的默认渲染尺寸（12b：标题栏与导航轨都是 24；`SmallIconButtonTokens.IconSize = 24`）。
pub const RENDER_PX: f32 = IconSize::Toolbar.px();

/// 标题栏 caption 三键（最小化 / 最大化 / 关闭）的图标边长。
///
/// **项目偏离**（用户点名）：官方 `SmallIconButtonTokens.IconSize = 24`，这里 20；容器 46×48 不动。
/// 值只在令牌层定义一次（`unlucky_design::icons::TITLEBAR_CAPTION_ICON_PX`），本文件只是转口。
pub const CAPTION_PX: f32 = unlucky_design::icons::TITLEBAR_CAPTION_ICON_PX;

/// 生成一个图标的 SVG 源码（`viewBox` 24×24，颜色用 `currentColor` 交给 iced 的色滤镜）。
///
/// 之所以用 `currentColor` 而不是把令牌色写进 SVG：色滤镜（`svg::Style::color`）会按
/// **alpha 蒙版**重涂 RGB，这样一份 SVG 位图能被浅/深两套主题复用，缓存不会翻倍。
///
/// `render_px` 决定两件事：SVG 的 `width`/`height` 属性与 `stroke-width` 的 viewBox 折算，
/// 两者必须同源，否则线宽会被二次缩放（见模块文档最后一段）。
pub fn source(kind: IconKind, render_px: f32) -> String {
    let icon = kind.icon();
    let paint = match icon.paint {
        IconPaint::Stroke => format!(
            "fill=\"none\" stroke=\"currentColor\" stroke-width=\"{}\" \
             stroke-linecap=\"round\" stroke-linejoin=\"round\"",
            icon.stroke_width(render_px)
        ),
        IconPaint::Fill => "fill=\"currentColor\" stroke=\"none\"".to_owned(),
    };

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{0}\" height=\"{0}\" \
         viewBox=\"0 0 24 24\"><path d=\"{1}\" {paint}/></svg>",
        render_px, icon.path
    )
}

/// 图标的 SVG 句柄（按 `(种类, 渲染尺寸)` 缓存；句柄 id 是内容哈希，所以 iced 侧也能命中缓存）。
fn handle(kind: IconKind, render_px: f32) -> svg::Handle {
    static HANDLES: OnceLock<Mutex<HashMap<(IconKind, u32), svg::Handle>>> = OnceLock::new();

    // 依赖「枚举下标 == IconKind::ALL 下标」（令牌层用 `GLYPHS[self as usize]` 同一约定）。
    // 这条断言把约定钉在测试与 debug 构建里，`IconKind` 一旦乱序插入就会立刻炸。
    debug_assert_eq!(
        IconKind::ALL[kind as usize],
        kind,
        "IconKind::ALL 必须与枚举声明顺序一致"
    );

    let key = (kind, (render_px * PX_KEY_SCALE).round() as u32);
    let cache = HANDLES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .entry(key)
        .or_insert_with(|| svg::Handle::from_memory(source(kind, render_px).into_bytes()))
        .clone()
}

/// 一个图标控件：按**令牌默认尺寸**（[`RENDER_PX`]）渲染，颜色走令牌角色。
pub fn view<'a, Message: 'a>(kind: IconKind, color: Color) -> iced::Element<'a, Message> {
    view_at(kind, color, RENDER_PX)
}

/// 一个图标控件：按调用方给的**令牌尺寸**渲染（`render_px` 必须来自 `unlucky_design::icons`）。
///
/// 只有需要「组件自己的图标尺寸不同」时才用这个入口（目前只有标题栏 caption 三键）。
/// 别为了摆一个位置而拿它调字面量：位置交给布局（`layout::fixed` / `centered_fixed`）。
pub fn view_at<'a, Message: 'a>(
    kind: IconKind,
    color: Color,
    render_px: f32,
) -> iced::Element<'a, Message> {
    svg(handle(kind, render_px))
        .width(render_px)
        .height(render_px)
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_produces_wellformed_svg() {
        for kind in IconKind::ALL {
            let source = source(kind, RENDER_PX);
            assert!(source.starts_with("<svg"), "{} 的 SVG 头不对", kind.name());
            assert!(source.ends_with("</svg>"), "{} 的 SVG 尾不对", kind.name());
            assert!(source.contains("viewBox=\"0 0 24 24\""), "{}", kind.name());
            assert!(
                source.contains(kind.icon().path),
                "{} 的 path 没写进去",
                kind.name()
            );
            assert!(!source.contains('#'), "{} 里出现了色值字面量", kind.name());
        }
    }

    #[test]
    fn stroke_icons_carry_the_token_stroke_width() {
        let stroke = source(IconKind::Settings, RENDER_PX);
        let expected = IconKind::Settings.icon().stroke_width(RENDER_PX);
        assert!(
            stroke.contains(&format!("stroke-width=\"{expected}\"")),
            "{stroke}"
        );
        assert!(stroke.contains("stroke-linecap=\"round\""));
        // 24px 渲染时的视觉线宽必须是 1.5（M54 结论）
        assert!((expected - 1.5).abs() < f32::EPSILON);
    }

    /// 默认尺寸**只从令牌取**：谁把 `RENDER_PX` 改成字面量或改成别的档，这条会红。
    #[test]
    fn default_render_size_comes_from_the_token() {
        assert_eq!(
            RENDER_PX,
            IconSize::Toolbar.px(),
            "默认 24 来自 IconSize::Toolbar"
        );
        assert_eq!(RENDER_PX, 24.0);
        assert!(
            source(IconKind::Close, RENDER_PX).contains("width=\"24\""),
            "默认渲染尺寸必须是 24"
        );
    }

    /// `view_at` 必须**按请求尺寸烘焙**（复审 r1 P1-2/P2-2 的守门断言）。两条路径的差别就是全部意义：
    /// - **烘焙路径**：`source(kind, 盒)` → SVG `width=盒` + 线宽 `1.5×24/盒` ⇒ 视觉恒 **1.5px**；
    /// - **缩放路径**（错的那条）：24 烘焙（`stroke-width="1.5"`）塞进 18 盒 ⇒ 视觉 **1.125px**。
    #[test]
    fn view_at_bakes_the_requested_size() {
        // (盒, 该盒应有的 viewBox 线宽, 盒子来源)——caption 20 / 按钮 18 / switch 滑块 16。
        let cases = [
            (CAPTION_PX, 1.8, "caption"),
            (IconSize::Button.px(), 2.0, "按钮"),
            (IconSize::SwitchThumb.px(), 2.25, "switch 滑块"),
        ];
        for (px, stroke, what) in cases {
            let svg = source(IconKind::Close, px);
            // `stroke-width` 由 `format!("{}", f32)` 渲染：2.0 会打印成 `2`，所以先比数值再比字符串。
            let baked = unlucky_design::icons::stroke_width_for(px, 1.5);
            assert!(
                (baked - stroke).abs() < 1.0e-6,
                "{what}：{px}px 盒的 viewBox 线宽应为 {stroke}，实际 {baked}"
            );
            assert!(
                svg.contains(&format!("width=\"{px}\""))
                    && svg.contains(&format!("stroke-width=\"{baked}\"")),
                "{what}：svg 必须按 {px} 烘焙：{svg}"
            );
            let visual = unlucky_design::icons::visual_stroke_px(px, baked);
            assert!(
                (visual - 1.5).abs() < 1.0e-6,
                "{what}：烘焙路径视觉线宽应为 1.5，实际 {visual}"
            );
        }

        // 反证：拿 24 烘焙的那份去填 18 盒（= 用 `view` 装进小盒），视觉只剩 1.125px。
        let scaled = source(IconKind::Check, RENDER_PX);
        assert!(scaled.contains("width=\"24\""));
        let scaled_visual = unlucky_design::icons::visual_stroke_px(IconSize::Button.px(), 1.5);
        assert!(
            (scaled_visual - 1.125).abs() < 1.0e-6,
            "缩放路径只该得到 1.125px（这就是为什么盒 ≠ 24 必须走 view_at）"
        );
    }

    #[test]
    fn filled_icons_do_not_get_a_stroke() {
        // `more` 是三个填充圆点
        let filled = source(IconKind::More, RENDER_PX);
        assert!(filled.contains("fill=\"currentColor\""));
        assert!(filled.contains("stroke=\"none\""));
    }

    #[test]
    fn handles_are_cached_per_kind_and_size() {
        let first = handle(IconKind::Close, RENDER_PX);
        let second = handle(IconKind::Close, RENDER_PX);
        assert_eq!(first.id(), second.id(), "同尺寸同图标必须复用句柄");
        assert_ne!(first.id(), handle(IconKind::Minimize, RENDER_PX).id());
        // 同一图标、不同渲染尺寸 = 不同位图（线宽不同），不能共用。
        assert_ne!(first.id(), handle(IconKind::Close, CAPTION_PX).id());
    }

    /// **盒 ≠ `RENDER_PX`(24) 的调用点必须走 `view_at`**（复审 r1 P1-1）。判据 = 调用形状匹配 +
    /// **逐调用点计数**（不做「文件里出现过 view_at 就整文件放行」的短路）：先剥注释，再认
    /// `icons::view(` / `icons::view::<` / `crate::`·`super::` 前缀 / `use …view as v` 的 `v(`。
    /// 每个文件的计数必须**等于**白名单期望（未列出 = 0），所以「同文件改对 3 处、漏掉 1 处」也会红。
    #[test]
    fn sub_24_boxes_go_through_view_at() {
        const VIEW: &[(&str, usize)] = &[
            ("shell/nav.rs", 1),
            ("shell/nav/account.rs", 1), // M114：轨底头像位
            ("components/inputs/field.rs", 2),
            ("components/inputs/menu.rs", 2),
            ("components/inputs/search.rs", 2),
            ("components/buttons/mod.rs", 1), // 24 图标 + 40 外壳
            // 导航族（M79）全是 `IconSize::Toolbar = 24`，`view` 正确；本白名单写于 M79 之前，
            // 「未列出 = 0」会误判它们。补进来后这几族再出现多余 `view` 仍会红。
            ("components/navigation/app_bar.rs", 1),
            ("components/navigation/drawer/item.rs", 1),
            ("components/navigation/list_item.rs", 2),
            ("components/navigation/tabs.rs", 1),
        ];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut views = 0usize;
        let mut bad: Vec<String> = Vec::new();
        for path in walk_rs(&root) {
            let rel = path.strip_prefix(&root).expect("strip_prefix");
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel == "icons.rs" {
                continue; // 实现自身
            }
            let code = strip_comments(&std::fs::read_to_string(&path).expect("读组件源码"));
            let view_alias = icon_view_aliases(&code);
            let view = count_calls(&code, "view", &view_alias);
            views += view;
            let want_view = sites(VIEW, &rel);
            if view != want_view {
                bad.push(format!(
                    "{rel}: {view} 处 `icons::view`，期望 {want_view} —— 盒 ≠ 24 必须 \
                     `icons::view_at(kind, color, 盒尺寸)`（24 烘焙再缩放只有 ~1.1px 线宽）"
                ));
            }
        }
        assert!(bad.is_empty(), "图标入口用法违规：\n{}", bad.join("\n"));
        assert_eq!(views, 14, "`icons::view` 调用点总数变了");

        fn sites(list: &[(&str, usize)], rel: &str) -> usize {
            list.iter().find(|(p, _)| *p == rel).map_or(0, |(_, n)| *n)
        }
        fn walk_rs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
            let mut out = Vec::new();
            for entry in std::fs::read_dir(dir).expect("读目录") {
                let path = entry.expect("目录项").path();
                if path.is_dir() {
                    out.extend(walk_rs(&path));
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
            out
        }
        /// 剥掉 `//` 行注释与 `/* */` 块注释；字符串里的 `//`（URL）不算注释。
        fn strip_comments(text: &str) -> String {
            let mut out = String::with_capacity(text.len());
            let mut it = text.chars().peekable();
            let (mut line, mut block, mut string) = (false, false, false);
            while let Some(c) = it.next() {
                if line {
                    line = c != '\n';
                    if c == '\n' {
                        out.push(c);
                    }
                } else if block {
                    if c == '*' && it.peek() == Some(&'/') {
                        it.next();
                        block = false;
                    }
                } else if string {
                    out.push(c);
                    if c == '\\' {
                        if let Some(esc) = it.next() {
                            out.push(esc);
                        }
                    } else if c == '"' {
                        string = false;
                    }
                } else if c == '"' {
                    string = true;
                    out.push(c);
                } else if c == '/' && it.peek() == Some(&'/') {
                    it.next();
                    line = true;
                } else if c == '/' && it.peek() == Some(&'*') {
                    it.next();
                    block = true;
                } else {
                    out.push(c);
                }
            }
            out
        }
        /// `use …icons…view …` 里 `view` 的本地名（含 `as` 别名与花括号列表）。
        fn icon_view_aliases(code: &str) -> Vec<String> {
            let mut names = Vec::new();
            for line in code.lines() {
                let t = line.trim_start();
                if !t.starts_with("use ") || !t.contains("icons") {
                    continue;
                }
                for (idx, _) in t.match_indices("view") {
                    let end = idx + 4;
                    if !word_start(t, idx) || !word_end(t, end) {
                        continue;
                    }
                    let alias = alias_after(t, end).unwrap_or_else(|| "view".to_owned());
                    if !names.contains(&alias) {
                        names.push(alias);
                    }
                }
            }
            names
        }
        /// `use …::view as v;` 里的 `v`。
        fn alias_after(line: &str, from: usize) -> Option<String> {
            let rest = line[from..].trim_start().strip_prefix("as")?.trim_start();
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        }
        /// 数调用点：路径形状（`icons::<name>`，含 `crate::`/`super::` 前缀）与别名形状，
        /// 后面必须紧跟 `(` 或 `::<`。
        fn count_calls(code: &str, name: &str, aliases: &[String]) -> usize {
            let mut count = 0;
            let path = format!("icons::{name}");
            for (idx, _) in code.match_indices(&path) {
                if word_start(code, idx) && follows_call(code, idx + path.len()) {
                    count += 1;
                }
            }
            for alias in aliases {
                for (idx, _) in code.match_indices(alias.as_str()) {
                    let end = idx + alias.len();
                    // `icons::view(` 里的 `view` 已由路径形状数过，别再数一次。
                    if !(idx >= 7 && code[..idx].ends_with("icons::"))
                        && word_start(code, idx)
                        && word_end(code, end)
                        && follows_call(code, end)
                    {
                        count += 1;
                    }
                }
            }
            count
        }
        fn follows_call(code: &str, from: usize) -> bool {
            let rest = code[from..].trim_start_matches([' ', '\t']);
            rest.starts_with('(') || rest.starts_with("::<")
        }
        fn word_start(code: &str, idx: usize) -> bool {
            code[..idx]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        }
        fn word_end(code: &str, idx: usize) -> bool {
            code[idx..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        }
    }
}
