//! `IconKind` 枚举：**API 已冻结，只允许在末尾追加**（序号即持久化 id，插到中间会静默换图）。

use super::glyphs::GLYPHS;
use super::Icon;

/// 图标种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum IconKind {
    /// 实例（2×2 网格）。
    Instances,
    /// 下载。
    Download,
    /// 日志（文档 + 正文）。
    Logs,
    /// 设置（齿轮）。
    Settings,
    /// AI（四角星）。
    Ai,
    /// 播放（实心三角）。
    Play,
    /// 停止（实心圆角方）。
    Stop,
    /// 搜索（放大镜）。
    Search,
    /// 加号。
    Plus,
    /// 关闭（叉）。
    Close,
    /// 最小化（横线）。
    Minimize,
    /// 最大化（方框）。
    Maximize,
    /// 左尖角。
    ChevronLeft,
    /// 右尖角。
    ChevronRight,
    /// 下尖角。
    ChevronDown,
    /// 展开为独立页（双对角箭头）。
    Expand,
    /// 发送（纸飞机）。
    Send,
    /// 勾选。
    Check,
    /// 警告（三角感叹）。
    Warn,
    /// 错误（圆形感叹）。
    Error,
    /// 更多（三点，实心）。
    More,
    /// 侧栏开关。
    Sidebar,
    /// 还原窗口（双层方框）。
    Restore,
    /// 账户占位（头像剪影）。
    User,
    /// 刷新（M61 新增）。
    Refresh,
    /// 筛选（M61 新增）。
    Filter,
    /// 目录（M61 新增）。
    Folder,
    /// 删除（M61 新增）。
    Trash,
}

impl IconKind {
    /// 全部图标，按枚举声明顺序。
    pub const ALL: [IconKind; 28] = [
        IconKind::Instances,
        IconKind::Download,
        IconKind::Logs,
        IconKind::Settings,
        IconKind::Ai,
        IconKind::Play,
        IconKind::Stop,
        IconKind::Search,
        IconKind::Plus,
        IconKind::Close,
        IconKind::Minimize,
        IconKind::Maximize,
        IconKind::ChevronLeft,
        IconKind::ChevronRight,
        IconKind::ChevronDown,
        IconKind::Expand,
        IconKind::Send,
        IconKind::Check,
        IconKind::Warn,
        IconKind::Error,
        IconKind::More,
        IconKind::Sidebar,
        IconKind::Restore,
        IconKind::User,
        IconKind::Refresh,
        IconKind::Filter,
        IconKind::Folder,
        IconKind::Trash,
    ];

    /// 取几何数据。
    pub fn icon(self) -> Icon {
        GLYPHS[self as usize]
    }

    /// 稳定字符串名（kebab-case）：写配置、日志、测试快照都用它，不要用 `Debug` 输出。
    pub const fn name(self) -> &'static str {
        match self {
            IconKind::Instances => "instances",
            IconKind::Download => "download",
            IconKind::Logs => "logs",
            IconKind::Settings => "settings",
            IconKind::Ai => "ai",
            IconKind::Play => "play",
            IconKind::Stop => "stop",
            IconKind::Search => "search",
            IconKind::Plus => "plus",
            IconKind::Close => "close",
            IconKind::Minimize => "minimize",
            IconKind::Maximize => "maximize",
            IconKind::ChevronLeft => "chevron-left",
            IconKind::ChevronRight => "chevron-right",
            IconKind::ChevronDown => "chevron-down",
            IconKind::Expand => "expand",
            IconKind::Send => "send",
            IconKind::Check => "check",
            IconKind::Warn => "warn",
            IconKind::Error => "error",
            IconKind::More => "more",
            IconKind::Sidebar => "sidebar",
            IconKind::Restore => "restore",
            IconKind::User => "user",
            IconKind::Refresh => "refresh",
            IconKind::Filter => "filter",
            IconKind::Folder => "folder",
            IconKind::Trash => "trash",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::IconPaint;

    #[test]
    fn glyph_table_matches_enum_order() {
        for (i, icon) in GLYPHS.iter().enumerate() {
            assert_eq!(
                icon.kind as usize,
                i,
                "GLYPHS 第 {i} 项是 {}，与 IconKind 顺序不一致",
                icon.kind.name()
            );
        }
    }

    #[test]
    fn all_kinds_resolve() {
        assert_eq!(IconKind::ALL.len(), GLYPHS.len());
        for kind in IconKind::ALL {
            let icon = kind.icon();
            assert_eq!(icon.kind, kind);
            assert!(!icon.path.is_empty());
        }
    }

    #[test]
    fn names_are_unique_and_kebab() {
        let mut seen = std::collections::BTreeSet::new();
        for kind in IconKind::ALL {
            let name = kind.name();
            assert!(seen.insert(name), "图标名重复：{name}");
            assert!(name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        }
    }

    #[test]
    fn migrated_kinds_keep_their_positions() {
        // 前 24 个是 Slint 版迁移来的（次序必须原样），后 4 个是 M61 追加的。
        assert_eq!(IconKind::ALL[0], IconKind::Instances);
        assert_eq!(IconKind::ALL[4], IconKind::Ai);
        assert_eq!(IconKind::ALL[20], IconKind::More);
        assert_eq!(IconKind::ALL[23], IconKind::User);
        assert_eq!(IconKind::ALL[24], IconKind::Refresh);
        assert_eq!(IconKind::ALL[27], IconKind::Trash);
    }

    #[test]
    fn only_solid_icons_are_filled() {
        for kind in IconKind::ALL {
            let icon = kind.icon();
            let expect_fill = matches!(kind, IconKind::Play | IconKind::Stop | IconKind::More);
            assert_eq!(
                icon.paint,
                if expect_fill {
                    IconPaint::Fill
                } else {
                    IconPaint::Stroke
                },
                "{} 的 fill/stroke 判定不对",
                kind.name()
            );
        }
    }
}
