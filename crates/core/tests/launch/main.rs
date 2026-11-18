//! launch 模块集成测试入口（`cargo test --test launch`）。

mod adapter_cases;
mod cmdline_snapshot;
mod command_builder_cases;
mod exit_cases;
mod graphics_backend_cases;
mod natives_args_cases;
mod natives_cases;
mod natives_dry_run_cases;
mod pipeline_cases;
mod prelude;
mod process_smoke;
mod pump_buffer;
mod session_events;
mod sink_cases;
mod tree_kill_cases;
mod watchdog_cases;
