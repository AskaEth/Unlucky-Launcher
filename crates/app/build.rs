//! Slint 编译入口：把 ui/main.slint 编译成 Rust 代码，供 src/main.rs 的 `slint::include_modules!()` 引入。
//! 保持极简——UI 结构演进（多文件拆分、主题文件）在 M17 阶段 B 里扩展本脚本。

/// 编译 `ui/main.slint`；失败时让构建直接报错（UI 语法错误不该被静默忽略）。
fn main() {
    slint_build::compile("ui/main.slint").expect("Slint UI 编译失败（ui/main.slint）");
}
