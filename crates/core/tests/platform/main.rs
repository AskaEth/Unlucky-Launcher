//! platform 模块集成测试入口（`cargo test -p unlucky-core --test platform`）。
//!
//! 覆盖 D19（`PlanDoc/11-os-detection.md`）的运行期探测：
//! - 等级映射 / 文案：注入式假数据，不改机器真实版本；
//! - 各平台探测的**解析**（Windows 注册表与 `ver`、Linux `/etc/os-release`+内核+glibc、
//!   macOS `sw_vers`+`sysctl`）：解析函数跨平台编译，所以能在任意宿主上单测；
//! - `rules.os.version` 用真实探测值求值的接线；
//! - 系统主题探测（`theme_probe`：注册表 / `defaults` / `gsettings` 解析 + 三态契约）。

mod linux_caps_cases;
mod macos_cases;
mod os_info_cases;
mod prelude;
mod rules_wiring;
mod theme_cases;
mod vk_cases;
mod windows_version_cases;
