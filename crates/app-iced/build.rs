//! `unlucky-launcher-iced` 的 PE 资源：版本信息（M190）+ 应用图标（M376）。
//!
//! 与 `crates/cli/build.rs` 同一份共享口径（`packaging/pe_version.rs`）：两个产物的
//! CompanyName / ProductName / LegalCopyright / 版本号必须逐字相同。
//!
//! ⚠ 本文件写的是**资源**，不碰子系统：PE 的 `Subsystem` 由 rustc 的 `windows_subsystem`
//! 属性决定（本分支上还没有那个属性，是 M196 的事），加图标与改子系统是两件事。
//! 判据：`packaging/check_pe_version.py` 会把它读到的 `subsystem` 打出来，加资源前后必须一致。
//!
//! 图标只有这个 GUI 产物有（`ull` 是命令行工具，理由写在 M376 的报告里）：`.ico` 里的
//! 多尺寸档位由 Windows 自己挑，构建只负责把它挂到资源 id 1（「应用图标」）。

include!("../../packaging/pe_version.rs");

fn main() {
    // 绝对路径：`rc` 的 `ICON` 语句按 .rc 所在目录 + include 路径找文件，而 OUT_DIR 在 target 下，
    // 用相对路径就要赌那一层相对关系；`CARGO_MANIFEST_DIR` 是构建时唯一稳定的锚点。
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR 一定存在");
    let icon = std::path::Path::new(&manifest).join("../../assets/branding/ull.ico");
    emit_pe_version_resource_with_icon("unlucky-launcher-iced", Some(&icon));
}
