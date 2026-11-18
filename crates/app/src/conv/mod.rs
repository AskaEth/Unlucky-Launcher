//! 状态层 → Slint 视图模型的单向转换（DTO 值 → `models.slint` 结构体）。
//!
//! 只做「领域值 → 展示值」的搬运与格式化，不含业务判断；
//! 这样 `ui_bind.rs` 里的同步逻辑就是纯粹的「状态灌进属性」。
//! 按领域拆子模块：instances / versions / logs / agent / ui，全量 `pub use`，
//! 调用方统一写 `crate::conv::tiles(...)` 这类路径。

mod agent;
mod instances;
mod logs;
mod pages;
mod ui;
mod versions;

pub use agent::{chat, dialog, toasts};
pub use instances::{running_tiles, tiles};
pub use logs::{diagnosis, log_lines, log_sessions};
pub use pages::{
    accounts, active_account_name, device, install, java_chain, java_runtimes, java_vendors,
    notices, setting_groups, wizard, wizard_versions,
};
pub use ui::{
    accent_color, bg_fill, bg_fill_from, category_from, instance_view, instance_view_from, route,
    route_from, theme_mode, theme_mode_from, tile_size, tile_size_from, version_category,
};
pub use versions::versions;
