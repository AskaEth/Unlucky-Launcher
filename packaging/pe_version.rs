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
// # 为什么只有 VERSIONINFO + （GUI 产物的）图标
//
// 本文件负责**两块** PE 资源：版本资源（两个 bin 都有）与应用图标（只有 GUI 产物有，M376）。
// 其余一律不碰：清单（DPI/权限）与 `windows_subsystem` 另有归属 —— 图标是资源，子系统是可选头里的
// 一个字段，加图标**不会**动子系统（`packaging/check_pe_version.py` 会把读到的 subsystem 打出来）。
//
// # 守门
//
// `packaging/check_pe_version.py` 是「写进源码不算，读回产物才算」的那道判据：它直接读 exe 字节，
// 断言 `VS_VERSION_INFO` 结构在、上述字符串（UTF-16LE）在。CI 的构建槽位跑它。
//
// # 构建日志里那一行「写进去了」的回报（M422）
//
// `emit_pe_version_resource_with_icon` 成功时会打一行 `PE VERSIONINFO: <bin> <四段式> / <发布者>`。
// 这行是**纯信息**：它不表示有问题，只是把刚写进资源的值回显一遍。问题在于 cargo 的 build script
// 指令只有 `cargo:warning=` 与 `cargo:error=` 两个级别，没有 info 级，所以想让它出现在日志里就
// 只能用 warning —— 于是每次重编 build script（改 `build.rs` / 改本文件 / 版本号变）都会在正常
// 构建日志里刷一条 `warning:` 开头的行，夹在真 warning 之间，让人更难分辨哪个才是要修的问题。
// （cargo 只把它当终端输出，**不**计入任何警告计数：日志实测，这行出现过的那次构建里
// `unlucky-cli (build script) generated …` 一行都没有。）
//
// 处置：默认**不打**，只有显式设了 `ULL_BUILD_VERBOSE`（**任意非空值**算开，空值算关）才打。
// 它的口径与 `ULL_ACCEPTANCE` 的规范判据（`acceptance::enabled()`，`0` / `off` / `false` / `no`
// 算关）**不同** —— `ULL_BUILD_VERBOSE=0` 在这里算开。它不是判据：有没有真的写进去由
// `check_pe_version.py` 读产物字节回答，本行只是给人看的。

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

/// 打开「PE VERSIONINFO 回显行」的环境变量名。
///
/// 只在排查「版本资源到底写了什么」时设它；平常不设，构建日志里就只剩真正的 warning。
pub const VERBOSE_ENV: &str = "ULL_BUILD_VERBOSE";

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

/// 给一个 bin 名（`ull` / `unlucky-launcher-iced`）写出版本资源（不带图标）。
///
/// 由 `crates/cli/build.rs` 调用；非 windows 目标直接返回。GUI 产物要图标，走
/// [`emit_pe_version_resource_with_icon`]。
pub fn emit_pe_version_resource(binary_name: &str) {
    emit_pe_version_resource_with_icon(binary_name, None);
}

/// 同上，另把 `icon`（`.ico` 文件）作为**应用图标**写进 PE 资源（M376）。
///
/// `icon` 的路径相对/绝对都可以（`winresource` 会把 `\` 转义后写进 `.rc`，rc 自己解析）；
/// 调用方给绝对路径最稳（构建目录与仓库根不在同一层时也不会读错文件）。
pub fn emit_pe_version_resource_with_icon(binary_name: &str, icon: Option<&std::path::Path>) {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    // 开关也要登记，否则「export ULL_BUILD_VERBOSE=1 再 cargo build」会一路命中缓存、build script
    // 一次都不重跑 ⇒ 那行永远等不到（在读环境变量的 build script 里漏 rerun-if-env-changed 就是这个坑）。
    println!("cargo:rerun-if-env-changed={VERBOSE_ENV}");
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
    if let Some(icon) = icon {
        // 图标是「文件名」而不是字符串资源：rc 的 `ICON` 语句读磁盘上的 .ico（多尺寸都塞进去），
        // 运行时由 Windows 按需要的档位取。它**不影响** subsystem / 清单。
        println!("cargo:rerun-if-changed={}", icon.display());
        let path = icon.to_string_lossy().replace('\\', "/");
        resource.set_icon(&path);
    }
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
    // 回显这一行是纯信息，cargo 又只有 warning 级没有 info 级（详见文件头「构建日志里那一行」），
    // 所以默认静音，只有 `ULL_BUILD_VERBOSE` 非空时打 —— 免得正常构建日志里多一条 `warning:`
    // 行，跟真 warning 混在一起。
    if std::env::var_os(VERBOSE_ENV).is_some_and(|value| !value.is_empty()) {
        println!("cargo:warning=PE VERSIONINFO: {binary_name} {dotted} / {COMPANY_NAME}");
    }
    let _ = PROJECT_HANDLE;
}
