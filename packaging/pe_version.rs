// PE 版本资源（VERSIONINFO）的唯一口径：发布者/产品/版权/版本号。
//
// 这个文件被 `crates/cli/build.rs` 与 `crates/app-iced/build.rs` 各 `include!` 一次——两个产物
// （`ull` 与 `unlucky-launcher-iced`）的署名必须逐字相同，复制两份表迟早会漂。
//
// # 字段口径（M190，塔已拍板）
//
// | 字段 | 值 | 为什么 |
// |---|---|---|
// | `CompanyName` | `Unlucky Launcher Project` | 发布者。**不得出现个人真实姓名**：这个名字随每个下载的 exe 传播，将来买 Authenticode 证书时 SmartScreen 显示的是证书主体，二者要对得上 |
// | `ProductName` / `FileDescription` | `Unlucky Launcher` | Windows「属性 → 详细信息」里最显眼的两行 |
// | `LegalCopyright` | `© 2026 Unlucky Launcher Project · Apache-2.0` | 版权 + 许可（仓库是 Apache-2.0） |
// | `FileVersion` / `ProductVersion` | `CARGO_PKG_VERSION` 的四段式 | 与 `Cargo.toml` **同源**：不另写一份版本号 |
// | `InternalName` / `OriginalFilename` | bin 名 / `<bin>.exe` | 供「这个 exe 到底叫什么」的取证 |
// | `Comments` | 仓库地址 | 项目署名 `AskaEth` 的可读落点（GitHub 命名空间，不是个人姓名） |
//
// # 只在 windows 目标上真跑
//
// 版本资源是 PE 概念：`build.rs` 先看 `CARGO_CFG_TARGET_OS`，不是 `windows` 就整段返回，
// Linux/macOS 构建一个字节都不多编译（它们的发布产物本来也没有 PE 头）。
//
// # 为什么不用 `windows_subsystem` 之外的资源
//
// 只要 VERSIONINFO 一块：图标、清单（DPI/权限）另有归属，本 mission 不碰（加了会改变运行时行为）。
//
// # 守门
//
// `packaging/check_pe_version.py` 是「写进源码不算，读回产物才算」的那道判据：它直接读 exe 字节，
// 断言 `VS_VERSION_INFO` 结构在、上述字符串（UTF-16LE）在。CI 的构建槽位跑它。

/// 发布者（CompanyName）。**改它等于改署名**：同时要改 Authenticode 证书主体与 README 的署名口径。
pub const COMPANY_NAME: &str = "Unlucky Launcher Project";

/// 产品名（`ProductName` 与 `FileDescription`）。
pub const PRODUCT_NAME: &str = "Unlucky Launcher";

/// 版权行（`LegalCopyright`）。
pub const COPYRIGHT: &str = "© 2026 Unlucky Launcher Project · Apache-2.0";

/// 项目署名（GitHub 命名空间；`Comments` 里随仓库地址一起出现）。
pub const PROJECT_HANDLE: &str = "AskaEth";

/// 仓库地址（`Comments`）。
pub const REPOSITORY: &str = "https://github.com/AskaEth/Unlucky-Launcher";

/// 把版本号补成四段式的数值（`0.2.3` → `0.2.3.0`；预发布段丢掉，因为 PE 的数值字段只认数字）。
///
/// 返回 `(四段元组, 打包成 FILEVERSION 用的 u64)`；任一数字大于 65535 时按 65535 截断
/// （PE 的每个分量只有 16 位，真实版本号远小于它）。
#[must_use]
pub fn four_part(version: &str) -> ([u64; 4], u64) {
    // `0.2.3-beta.1` 的数值部分只有前三段；`+build` 元数据同理不计入版本资源。
    let numeric = version.split(['-', '+']).next().unwrap_or(version).trim();
    let mut parts = [0_u64; 4];
    for (index, piece) in numeric.split('.').take(4).enumerate() {
        parts[index] = piece.parse::<u64>().unwrap_or(0).min(0xFFFF);
    }
    let packed = (parts[0] << 48) | (parts[1] << 32) | (parts[2] << 16) | parts[3];
    (parts, packed)
}

/// 给一个 bin 名（`ull` / `unlucky-launcher-iced`）写出版本资源。
///
/// 由两个 crate 的 `build.rs` 调用；非 windows 目标直接返回。
pub fn emit_pe_version_resource(binary_name: &str) {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    // 共享口径文件改了也要重编（`include!` 不被 cargo 追踪）。
    println!("cargo:rerun-if-changed=../../packaging/pe_version.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        // Linux/macOS：没有 PE 头，整段不跑（发布产物的形状不变）。
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let (parts, packed) = four_part(&version);
    let dotted = format!("{}.{}.{}.{}", parts[0], parts[1], parts[2], parts[3]);

    let mut resource = winresource::WindowsResource::new();
    resource.set("CompanyName", COMPANY_NAME);
    resource.set("ProductName", PRODUCT_NAME);
    resource.set("FileDescription", PRODUCT_NAME);
    resource.set("LegalCopyright", COPYRIGHT);
    resource.set("InternalName", binary_name);
    resource.set("OriginalFilename", &format!("{binary_name}.exe"));
    resource.set("Comments", REPOSITORY);
    // 字符串形态（「属性」页显示的）与数值形态（资源管理器/安装器排序用的）都要给。
    resource.set("FileVersion", &dotted);
    resource.set("ProductVersion", &dotted);
    resource.set_version_info(winresource::VersionInfo::FILEVERSION, packed);
    resource.set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed);

    if let Err(error) = resource.compile() {
        // 资源编不出来就**中断构建**：宁可不出产物，也不出一个「属性里什么都没有」的 exe
        // （那正是 M190 要修的病）。
        panic!("写 PE 版本资源失败（{binary_name} {dotted}）：{error}");
    }
    println!("cargo:warning=PE VERSIONINFO: {binary_name} {dotted} / {COMPANY_NAME}");
    let _ = PROJECT_HANDLE;
}
