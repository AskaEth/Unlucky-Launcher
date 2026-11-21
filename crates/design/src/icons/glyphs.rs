//! 图标几何数据：24 网格、1.5px 视觉描边的 SVG path 字符串。
//!
//! 源流：`DevDocs/ops/slint-reference/crates/app/ui/components/icons.slint`（M54 重画过一轮的
//! Fluent 2 线性几何）。命令集沿用 `M/L/H/V/A/C/Q/Z`，与 SVG 路径语法一致，可直接交给
//! 任何 SVG path 解析器（iced 侧是 `iced::widget::canvas` 的 `Path`/`lyon`）。
//!
//! 这里只有数据：不许在数据里写颜色、尺寸、dpi——那些由渲染层决定。

use super::{Icon, IconKind, IconPaint};

/// 全部图标几何，**下标必须等于 `IconKind as usize`**（由测试钉死）。
pub(super) const GLYPHS: [Icon; 28] = [
    // 实例：2×2 圆角方块（原 Slint 版是 4 个 Rectangle 描边，这里折成一条 path）
    Icon::data(
        IconKind::Instances,
        "M 5 3 H 8 A 2 2 0 0 1 10 5 V 8 A 2 2 0 0 1 8 10 H 5 A 2 2 0 0 1 3 8 V 5 A 2 2 0 0 1 5 3 Z \
         M 16 3 H 19 A 2 2 0 0 1 21 5 V 8 A 2 2 0 0 1 19 10 H 16 A 2 2 0 0 1 14 8 V 5 A 2 2 0 0 1 16 3 Z \
         M 5 14 H 8 A 2 2 0 0 1 10 16 V 19 A 2 2 0 0 1 8 21 H 5 A 2 2 0 0 1 3 19 V 16 A 2 2 0 0 1 5 14 Z \
         M 16 14 H 19 A 2 2 0 0 1 21 16 V 19 A 2 2 0 0 1 19 21 H 16 A 2 2 0 0 1 14 19 V 16 A 2 2 0 0 1 16 14 Z",
        IconPaint::Stroke,
    ),
    // 下载：箭头入托盘
    Icon::data(
        IconKind::Download,
        "M 12 3.5 V 15 M 7 10.5 L 12 15.5 L 17 10.5 M 4.5 20.5 H 19.5",
        IconPaint::Stroke,
    ),
    // 日志：卷角文档 + 两行正文（不是三横——三横读作「菜单」）
    Icon::data(
        IconKind::Logs,
        "M 6 3.5 H 14 L 18.5 8 V 20.5 H 6 Z M 14 3.5 V 8 H 18.5 M 9 12.5 H 15.5 M 9 16.5 H 15.5",
        IconPaint::Stroke,
    ),
    // 设置：八齿齿轮 + 中心孔
    Icon::data(
        IconKind::Settings,
        "M 18.89 14.11 L 21.51 13.34 L 21.51 10.66 L 18.89 9.89 A 7.2 7.2 0 0 1 18.36 8.62 \
         L 19.67 6.22 L 17.78 4.33 L 15.38 5.64 A 7.2 7.2 0 0 1 14.11 5.11 L 13.34 2.49 L 10.66 2.49 \
         L 9.89 5.11 A 7.2 7.2 0 0 1 8.62 5.64 L 6.22 4.33 L 4.33 6.22 L 5.64 8.62 A 7.2 7.2 0 0 1 5.11 9.89 \
         L 2.49 10.66 L 2.49 13.34 L 5.11 14.11 A 7.2 7.2 0 0 1 5.64 15.38 L 4.33 17.78 L 6.22 19.67 \
         L 8.62 18.36 A 7.2 7.2 0 0 1 9.89 18.89 L 10.66 21.51 L 13.34 21.51 L 14.11 18.89 A 7.2 7.2 0 0 1 15.38 18.36 \
         L 17.78 19.67 L 19.67 17.78 L 18.36 15.38 A 7.2 7.2 0 0 1 18.89 14.11 Z \
         M 8.8 12 A 3.2 3.2 0 1 0 15.2 12 A 3.2 3.2 0 1 0 8.8 12 Z",
        IconPaint::Stroke,
    ),
    // AI：四角星（凹边四角星，不是直线菱形）
    Icon::data(
        IconKind::Ai,
        "M 12 3 Q 13.6 10.4 21 12 Q 13.6 13.6 12 21 Q 10.4 13.6 3 12 Q 10.4 10.4 12 3 Z",
        IconPaint::Stroke,
    ),
    // 播放 / 停止：实心（与文字并排时不至于太轻）
    Icon::data(
        IconKind::Play,
        "M 7.5 4.5 L 19 12 L 7.5 19.5 Z",
        IconPaint::Fill,
    ),
    Icon::data(
        IconKind::Stop,
        "M 8 7 H 16 A 1 1 0 0 1 17 8 V 16 A 1 1 0 0 1 16 17 H 8 A 1 1 0 0 1 7 16 V 8 A 1 1 0 0 1 8 7 Z",
        IconPaint::Fill,
    ),
    // 搜索：放大镜
    Icon::data(
        IconKind::Search,
        "M 10.5 4 A 6.5 6.5 0 1 0 10.5 17 A 6.5 6.5 0 1 0 10.5 4 Z M 15.4 15.4 L 20.5 20.5",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::Plus,
        "M 12 5 V 19 M 5 12 H 19",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::Close,
        "M 6.5 6.5 L 17.5 17.5 M 17.5 6.5 L 6.5 17.5",
        IconPaint::Stroke,
    ),
    Icon::data(IconKind::Minimize, "M 6 12 H 18", IconPaint::Stroke),
    Icon::data(
        IconKind::Maximize,
        "M 7.5 5.5 H 16.5 A 2 2 0 0 1 18.5 7.5 V 16.5 A 2 2 0 0 1 16.5 18.5 H 7.5 \
         A 2 2 0 0 1 5.5 16.5 V 7.5 A 2 2 0 0 1 7.5 5.5 Z",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::ChevronLeft,
        "M 14.5 5.5 L 8 12 L 14.5 18.5",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::ChevronRight,
        "M 9.5 5.5 L 16 12 L 9.5 18.5",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::ChevronDown,
        "M 5.5 9.5 L 12 16 L 18.5 9.5",
        IconPaint::Stroke,
    ),
    // 展开为独立页：双对角外箭头
    Icon::data(
        IconKind::Expand,
        "M 14 4 H 20 V 10 M 20 4 L 13.5 10.5 M 10 20 H 4 V 14 M 4 20 L 10.5 13.5",
        IconPaint::Stroke,
    ),
    // 发送：纸飞机
    Icon::data(
        IconKind::Send,
        "M 21 3 L 3 10.5 L 10.2 13.6 L 13.4 20.5 Z M 10.2 13.6 L 21 3",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::Check,
        "M 5 12.5 L 9.8 17.3 L 19 6.5",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::Warn,
        "M 12 4.5 L 20.5 19.5 H 3.5 Z M 12 9.5 V 14 M 12 16.6 V 17",
        IconPaint::Stroke,
    ),
    Icon::data(
        IconKind::Error,
        "M 12 4 A 8 8 0 1 0 12 20 A 8 8 0 1 0 12 4 Z M 12 7.5 V 13 M 12 16.6 V 17",
        IconPaint::Stroke,
    ),
    // 更多：三圆点（实心，原 Slint 版是三个 filled Rectangle）
    Icon::data(
        IconKind::More,
        "M 7.5 12 A 1.5 1.5 0 1 0 4.5 12 A 1.5 1.5 0 1 0 7.5 12 Z \
         M 13.5 12 A 1.5 1.5 0 1 0 10.5 12 A 1.5 1.5 0 1 0 13.5 12 Z \
         M 19.5 12 A 1.5 1.5 0 1 0 16.5 12 A 1.5 1.5 0 1 0 19.5 12 Z",
        IconPaint::Fill,
    ),
    // 侧栏：面板框 + 左侧分栏线
    Icon::data(
        IconKind::Sidebar,
        "M 6 5 H 18 A 2 2 0 0 1 20 7 V 17 A 2 2 0 0 1 18 19 H 6 A 2 2 0 0 1 4 17 V 7 \
         A 2 2 0 0 1 6 5 Z M 9.5 5.5 V 18.5",
        IconPaint::Stroke,
    ),
    // 还原（已最大化时的标题栏按钮）：前后两层方框
    Icon::data(
        IconKind::Restore,
        "M 9 4.5 H 19.5 V 15 M 15 8.5 H 4.5 V 19.5 H 15 Z",
        IconPaint::Stroke,
    ),
    // 账户占位：头像剪影
    Icon::data(
        IconKind::User,
        "M 12 5.3 A 3.7 3.7 0 1 0 12 12.7 A 3.7 3.7 0 1 0 12 5.3 Z \
         M 4.5 20.5 C 4.5 16.9 8 15 12 15 C 16 15 19.5 16.9 19.5 20.5",
        IconPaint::Stroke,
    ),
    // ── M61 新增（追加在末尾，不动既有次序；iced 页面需要而 Slint 版没有的）──
    // 刷新：顺时针单圈 + 右上角箭头
    Icon::data(
        IconKind::Refresh,
        "M 20 12 A 8 8 0 1 1 12 4 C 14.2 4 16.3 4.9 17.8 6.4 L 20 8.5 \
         M 20 4.5 V 8.5 H 16",
        IconPaint::Stroke,
    ),
    // 筛选：漏斗
    Icon::data(
        IconKind::Filter,
        "M 4.5 5.5 H 19.5 L 14 12.5 V 19 L 10 16.5 V 12.5 Z",
        IconPaint::Stroke,
    ),
    // 目录：文件夹（实例目录 / 日志目录入口）
    Icon::data(
        IconKind::Folder,
        "M 4 7.5 A 2 2 0 0 1 6 5.5 H 9.5 L 11.5 7.5 H 18 A 2 2 0 0 1 20 9.5 V 17 \
         A 2 2 0 0 1 18 19 H 6 A 2 2 0 0 1 4 17 Z",
        IconPaint::Stroke,
    ),
    // 删除：垃圾桶
    Icon::data(
        IconKind::Trash,
        "M 4.5 7 H 19.5 M 9.5 7 V 4.5 H 14.5 V 7 M 6.8 7 L 7.8 19.5 H 16.2 L 17.2 7 \
         M 10.5 10.5 V 16 M 13.5 10.5 V 16",
        IconPaint::Stroke,
    ),
];
