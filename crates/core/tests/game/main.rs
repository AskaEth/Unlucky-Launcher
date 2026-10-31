//! `core::game` 的集成测试入口（阶段 A：先写好，mod.rs 接线在阶段 B 完成）。
//!
//! 每个子模块对应 mission 清单里的一项：
//! - `manifest_parse`：版本 JSON 解析与保真
//! - `inherit_chain`：inheritsFrom 解析 / 环检测 / 缺失父 / merge 语义 / patch 优先级
//! - `arguments_rules`：规则求值、参数展开、natives 选择
//! - `version_list`：清单 v2 解析、分类、排序、unlisted 合并
//! - `layout`：`.minecraft` 目录布局
//! - `snapshot`：不可变快照 + 单写者草稿
//! - `profiles`：实例清单与稳定 UUID
//! - `vulkan_capability`：版本库表 → `has_vulkan_backend` 能力位

mod arguments_rules;
mod helpers;
mod inherit_chain;
mod isolation;
mod isolation_switch;
mod java_discover;
mod java_install;
mod java_location;
mod java_mapping;
mod java_mirrors;
mod java_vendor_chain;
mod layout;
mod manifest_parse;
mod profiles;
mod snapshot;
mod unlisted;
mod version_list;
mod vulkan_capability;
