//! 自更新文案（M159）：设置页「更新」分组 + 顶部提示条。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节；本域**追加在链尾**（`account_entries`
//! 之后），于是 `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，最后 update（金表只追加不重排）。
//!
//! 三条口径：
//!
//! 1. **「检查失败」与「已是最新」是两句不同的文案**（mission 的硬要求）：前者带原因，
//!    后者带版本号。别为了少一个 key 把它们合并成一条；
//! 2. 体积走 `{size}`（由页面按 `human_bytes` 格式化后传入），版本号走 `{version}`——
//!    目录里不写死任何版本号；
//! 3. 品牌/技术名词不翻：`OpenGL` / `SHA256` / `GitHub` 这类保持原样。

macro_rules! update_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 设置页「更新」分组
            SettingsGroupUpdate => ("Updates", "更新"),
            SettingsUpdateCurrent => ("Current version", "当前版本"),
            SettingsUpdateChannel => ("Channel", "更新渠道"),
            SettingsUpdateChannelStable => ("Stable", "稳定版"),
            SettingsUpdateAutoCheck => ("Check for updates on startup", "启动时检查更新"),
            SettingsUpdateCheckNow => ("Check now", "立即检查"),
            SettingsUpdateChecking => ("Checking…", "正在检查…"),
            SettingsUpdateNever => ("Not checked yet", "还没检查过"),
            SettingsUpdateUpToDate => ("You are on the latest version ({version})", "已是最新（{version}）"),
            SettingsUpdateAvailable => ("Version {version} is available ({size})", "有新版本 {version}（{size}）"),
            SettingsUpdateMissingArtifact => (
                "Version {version} is out, but there is no build for this platform yet.",
                "{version} 已发布，但还没有本平台的安装包。"
            ),
            SettingsUpdateFailed => ("Check failed: {reason}", "检查失败：{reason}"),
            SettingsUpdateApply => ("Download and install", "下载并安装"),
            SettingsUpdateApplying => ("Downloading and verifying…", "正在下载并校验…"),
            SettingsUpdateReady => ("Version {version} is ready — restart to use it.", "版本 {version} 已就位，重启后生效。"),
            SettingsUpdateNotes => ("Release notes: {notes}", "更新说明：{notes}"),
            SettingsUpdateForced => ("This update is required", "这是一次强制更新"),
            SettingsUpdateLastChecked => ("Last check", "上次检查"),
            SettingsUpdateHint => (
                "Downloads are verified with SHA256 before they touch the launcher. The running copy stays as it is until you restart.",
                "更新包在落到启动器之前会做 SHA256 校验；正在运行的这一份在你重启之前不会变。"
            ),
            // ── 顶部提示条
            UpdateBannerTitleAvailable => ("Version {version} is available", "发现新版本 {version}"),
            UpdateBannerBodyAvailable => ("Download size {size}", "下载体积 {size}"),
            UpdateBannerTitleReady => ("Update ready", "更新已就位"),
            UpdateBannerBodyReady => ("Version {version} will run after a restart.", "重启后运行版本 {version}。"),
            UpdateBannerTitleFailed => ("Update failed", "更新失败"),
            UpdateBannerApplying => ("Downloading the update…", "正在下载更新…"),
            UpdateBannerInstall => ("Download and install", "下载并安装"),
            UpdateBannerDismiss => ("Skip this version", "忽略这个版本"),
            UpdateBannerRestartNow => ("Restart now", "立即重启"),
            UpdateBannerRetry => ("Try again", "重试"),
        } }
    };
}

pub(crate) use update_entries;
