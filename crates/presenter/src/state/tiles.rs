//! 磁贴网格的纯函数与单元类型（从 `state.rs` 原样搬出，守 400 行硬线）。
//!
//! **纯搬移**：类型、函数、文档一字未改，只是换了文件；`state.rs` 用 `pub use` 再导出，
//! 于是 `crate::state::{TileCell, grid_positions}` 这两个老路径继续可用。

use crate::dto::InstanceSummary;

/// 磁贴网格单元：位置由 [`grid_positions`] 算好，UI 侧只消费 row/col。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileCell<'a> {
    /// 实例数据
    pub instance: &'a InstanceSummary,
    /// 网格行号（0 起）
    pub row: usize,
    /// 网格列号（0 起）
    pub col: usize,
}

/// 纯函数：把 n 个磁贴摊到 `columns` 列的网格上，返回 `(row, col)`。
///
/// 特意放在 Rust 侧：声明式 UI 的表达式里做整数除法容易引出隐式浮点，且这段逻辑可以单测。
pub fn grid_positions(count: usize, columns: usize) -> Vec<(usize, usize)> {
    let columns = columns.max(1);
    (0..count)
        .map(|index| (index / columns, index % columns))
        .collect()
}
