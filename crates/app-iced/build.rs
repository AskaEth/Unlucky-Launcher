//! `unlucky-launcher-iced` 的 PE 版本资源（M190）。
//!
//! 与 `crates/cli/build.rs` 同一份共享口径（`packaging/pe_version.rs`）：两个产物的
//! CompanyName / ProductName / LegalCopyright / 版本号必须逐字相同。
//!
//! ⚠ 本文件**只**写版本资源，不碰子系统：PE 的 `Subsystem` 由 rustc 的
//! `windows_subsystem` 属性决定（本分支上还没有那个属性，是 M196 的事），版本资源与子系统是两件事。
//! 判据：`packaging/check_pe_version.py` 会把它读到的 `subsystem` 打出来，加资源前后必须一致。

include!("../../packaging/pe_version.rs");

fn main() {
    emit_pe_version_resource("unlucky-launcher-iced");
}
