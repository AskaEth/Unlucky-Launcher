//! 磁贴网格的纯函数与单元类型（从 `state.rs` 原样搬出，守 400 行硬线）。
//!
//! **纯搬移**：类型、函数、文档一字未改，只是换了文件；`state.rs` 用 `pub use` 再导出，
//! 于是 `crate::state::{TileCell, grid_positions}` 这两个老路径继续可用。
//!
//! M224 又把 [`AppState::tiles`] 从 `state.rs` 搬到这里（同一个 400 行原因）：筛选 + 排版是
//! 磁贴域自己的事，方法留在 `impl AppState` 的另一个块里，调用点 `state.tiles()` 不变。

use crate::dto::InstanceSummary;

use super::AppState;

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

impl AppState {
    /// 按搜索词过滤后的磁贴网格（顺序稳定 = 实例列表顺序）。
    pub fn tiles(&self) -> Vec<TileCell<'_>> {
        let query = self.instance_query.trim().to_lowercase();
        let filtered: Vec<&InstanceSummary> = self
            .instances
            .iter()
            .filter(|instance| query.is_empty() || instance.name.to_lowercase().contains(&query))
            .collect();
        let positions = grid_positions(filtered.len(), self.prefs.tile_size.columns());
        filtered
            .into_iter()
            .zip(positions)
            .map(|(instance, (row, col))| TileCell { instance, row, col })
            .collect()
    }
}
