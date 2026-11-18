//! `ull` 的 PE 版本资源（M190）。
//!
//! 只做一件事：调共享口径 [`emit_pe_version_resource`]（定义在 `packaging/pe_version.rs`，
//! 两个 bin 共用同一份署名表）。非 windows 目标里那个函数会立刻返回，Linux/macOS 构建零影响。

include!("../../packaging/pe_version.rs");

fn main() {
    emit_pe_version_resource("ull");
}
