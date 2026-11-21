//! # Ink Cat 规格卡 → 令牌（形状 / 尺寸档 / 字号校核）
//!
//! 来源是用户自制的规格卡 `DevDocs/artifacts/components-ink.svg`（Ink Cat / M3 Expressive，
//! 1480×2260）。色板在 [`crate::color::boards`]，**本模块只装几何与排版**：
//!
//! | 子模块 | 内容 |
//! |---|---|
//! | [`size`] | 按钮 XS~XL 五档（[`size::ControlSize`]）+ FAB 四态（[`size::FabSize`]） |
//! | [`metrics`] | 控件方框几何：卡片 / 对话框 / snackbar / 列表行 / chip / 徽标 / 字段 / 复选框 / 单选框 / 开关 / 进度 / 加载 |
//! | [`type_scale`] | 规格卡出现的每个字号 → 最近官方档的**校核结论**（命中 / 落在两档之间 / 低于 UI 下限） |
//!
//! ## 与 [`crate::component_map`] 的关系：并列来源，不是替代
//!
//! `component_map` 的口径是**官方 M3 组件令牌**（`androidx.compose.material3.tokens`，A 级）；
//! 规格卡是**用户自己的视觉决议**（也是 A 级，但是产品口径）。两者在若干处不一致，
//! 本模块把差异**逐条登记**而不是偷偷改 `component_map`：
//!
//! | 项 | 规格卡 | `component_map`（官方） | 处置 |
//! |---|---|---|---|
//! | 卡片圆角 | **16**（`rx=16`） | `FilledCard/OutlinedCard = Medium(12)` | 登记：换肤时取 [`metrics::CARD`]（Large 16）。表值不动——app-iced 的 `pages/frame/tests.rs` 按 12 pin 住，改表等于连带改观感，归 M229 |
//! | Snackbar | 高 **52** / 圆角 **16** | `ExtraSmall(4)` | 登记同上（[`metrics::SNACKBAR`]） |
//! | 列表行 | 高 **60** / 药丸 **30** | `Small(8)` | 登记同上（[`metrics::LIST_ROW`]） |
//! | 输入框 | 圆角 **4**（默认）/ **28**（药丸变体） | `ExtraSmall(4)` | 一致 + 新增药丸变体（[`metrics::FIELD_SHAPE_ROUNDED`]） |
//! | Chip | 高 **32** / 圆角 **8** | `Small(8)`、无高度 | 一致 + 补高度（[`metrics::CHIP`]） |
//! | Checkbox | **18×18** / 圆角 **4** / 描边 2 | 20×20 / 圆角 2（微半径） | 差异登记（[`metrics::CHECKBOX`]）。4 是 `ShapeScale` 刻度值，42 号位不冲突 |
//! | Radio | **18 Ø** / 环 2 / 内点 10 | 20 Ø / 环 2 / 内点 10 | 差异登记（[`metrics::RADIO`]） |
//! | 按钮档 | **XS~XL = 32/40/56/96/136** | 无档位（`LayoutMetrics::control_height = 40`） | 新增 [`size::ControlSize`] |
//! | FAB | 四态 40/56/80/160×56，圆角 12/16/20/16 | 无 | 新增 [`size::FabSize`] |
//! | 对话框 | 380×190 / 圆角 **28** | `ExtraLarge(28)` | 一致（[`metrics::DIALOG`]） |
//!
//! **为什么不动 `component_map` 的表值**：那张表已经被 iced 壳层消费并 pin 住（卡片 12、
//! 搜索条 Full 等），改值等于在令牌层做了一次没人截图验收的换肤。本模块先给出规格卡口径的
//! 数据，M229 换肤时一次切换 + 出图验收，避免「一半旧一半新」。
//!
//! ## 数值口径
//!
//! - 所有尺寸单位都是**逻辑 px**（100% DPI 下 1px = 1dp），与 `ShapeScale::dp()` 同一刻度；
//! - `0.0` 一律表示「由内容决定」（与 [`crate::component_map::ComponentMetrics`] 同约定），
//!   不是「尺寸为零」；
//! - 规格卡没写、但实现需要的值（按钮水平内距）标 `[估]` 并注明推导规则。

pub mod metrics;
pub mod size;
pub mod type_scale;

pub use metrics::SpecBox;
pub use size::{ControlSize, FabSize};
pub use type_scale::{SpecFit, SpecText, UI_TEXT_FLOOR_PX};
