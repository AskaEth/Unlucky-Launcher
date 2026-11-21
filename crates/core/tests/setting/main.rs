//! M12（core::setting + platform）集成测试入口。
//!
//! cargo 把 `tests/<name>/main.rs` 识别为独立的 test target（target 名 = `setting`），
//! 因此本目录下每个子模块就是一个测试主题，避免单文件超过 400 行。

mod appearance_keys;
mod atomic_write;
mod common;
mod debounce;
mod deprecated_keys;
mod dir_migration;
mod dirs;
mod document;
mod isolation_keys;
mod java_install_keys;
mod layout;
mod migration_chain;
mod redirect;
mod schema_compat;
mod spec_ai;
mod store;
mod update_keys;
