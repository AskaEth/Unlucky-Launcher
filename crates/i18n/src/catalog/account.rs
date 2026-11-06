//! 账户页文案（M114 三轨：微软正版 / 第三方皮肤站 / 本地档案）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。本域**追加在装配链末尾**，
//! 所以金表（`catalog::golden`）里这批 key 也排在最后。
//!
//! ## 口径（不许改写成别的说法）
//!
//! 本地档案的说明必须带「仅本地/隔离网络测试用，无法连接启用正版验证的服务器」——
//! 这是 README 已定的口径（`AccountTrackOfflineBody` / `AccountOfflineWarning` 两处都写全），
//! 不许写成「兼容旧启动器迁移」这种暗示能进正版服的说法。
//!
//! 文案里**没有任何 token 字样**：账户页从不展示凭据（M114 审核红线），
//! 「密码只用于这一次登录」这条提示是唯一提到密码的地方。

macro_rules! account_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 页头 / 列表
            AccountTitle => ("Accounts", "账户"),
            AccountSubtitle => ("Microsoft, third-party skin servers and local profiles", "微软正版、第三方皮肤站与本地档案"),
            AccountListTitle => ("All accounts", "全部账户"),
            AccountAddedOn => ("Added {date}", "添加于 {date}"),
            AccountActiveChip => ("Active", "当前"),
            AccountSwitch => ("Use for launch", "切换为启动账户"),
            AccountRelogin => ("Sign in again", "重新登录"),
            AccountRemove => ("Remove", "删除"),
            AccountKindMicrosoft => ("Microsoft", "微软正版"),
            AccountKindAuthlib => ("Third-party skin server", "第三方皮肤站"),
            AccountKindOffline => ("Local profile", "本地档案"),
            AccountServer => ("Server {server}", "服务器 {server}"),
            // ── 空态
            AccountEmptyTitle => ("No accounts yet", "还没有账户"),
            AccountEmptyBody => ("Add a Microsoft account, a third-party skin server account, or a local profile.", "添加微软正版账户、第三方皮肤站账户，或建一个本地档案。"),
            AccountEmptyAction => ("Add an account", "添加账户"),
            // ── 三条轨道的入口卡
            AccountTrackMicrosoftTitle => ("Microsoft account", "微软正版"),
            AccountTrackMicrosoftBody => ("Sign in with a device code or in your browser. Online servers require this kind of account.", "用设备码或浏览器登录。联机服务器必须用这种账户。"),
            AccountTrackAuthlibTitle => ("Third-party skin server", "第三方皮肤站"),
            AccountTrackAuthlibBody => ("An authlib-injector compatible server (LittleSkin and friends) brings its own skins and capes.", "兼容 authlib-injector 的皮肤站（如 LittleSkin）自带皮肤与披风。"),
            AccountTrackOfflineTitle => ("Local profile", "本地档案"),
            AccountTrackOfflineBody => ("Local / isolated-network testing only — it cannot join servers that require a genuine Minecraft account.", "仅本地/隔离网络测试用，无法连接启用正版验证的服务器。"),
            AccountTrackOpen => ("Sign in", "登录"),
            AccountTrackCreate => ("Create profile", "创建档案"),
            AccountDialogClose => ("Close", "关闭"),
            // ── 本地档案表单
            AccountOfflineNameLabel => ("Profile name", "档案名"),
            AccountOfflineNameHint => ("3–16 letters, digits or underscore", "3–16 位字母、数字或下划线"),
            AccountOfflineSubmit => ("Create profile", "创建档案"),
            AccountOfflineWarning => ("Local / isolated-network testing only — it cannot join servers that require a genuine Minecraft account.", "仅本地/隔离网络测试用，无法连接启用正版验证的服务器。"),
            // ── 微软登录面板（设备码 / 授权码）
            AccountDeviceCodeLabel => ("Enter this code", "输入这个用户码"),
            AccountDeviceCodeCopy => ("Copy code", "复制用户码"),
            AccountDeviceLinkCopy => ("Copy link", "复制链接"),
            AccountDeviceCountdown => ("{time} left", "剩 {time}"),
            AccountDeviceExpired => ("This code has expired. Start again for a fresh one.", "这个用户码已过期，重新开始换一个新的。"),
            AccountDeviceRetry => ("Try again", "重试"),
            AccountDeviceCancel => ("Cancel", "取消"),
            AccountDeviceBrowser => ("Sign in with browser", "用浏览器登录"),
            AccountDeviceUseCode => ("Use a device code instead", "改用设备码"),
            // ── 第三方站表单
            AccountServerLabel => ("Server address", "服务器地址"),
            AccountServerHint => ("API root, e.g. https://littleskin.cn/api/yggdrasil/", "API 根，例：https://littleskin.cn/api/yggdrasil/"),
            AccountServerProbe => ("Check server", "校验服务器"),
            AccountLoginNameLabel => ("Email or username", "邮箱或用户名"),
            AccountPasswordLabel => ("Password", "密码"),
            AccountPasswordHint => ("Used once for this sign-in. It is never stored.", "只在这次登录里用一次，不会保存。"),
            AccountAuthlibSubmit => ("Sign in", "登录"),
            AccountAuthlibIdleHint => ("The server is checked automatically when you sign in; Check server only does it up front.", "登录时会自动校验服务器；「校验服务器」只是提前跑一遍。"),
            // ── 皮肤区
            AccountSkinTitle => ("Skin", "皮肤"),
            AccountSkinNone => ("No skin available", "没有可用皮肤"),
            AccountSkinHint => ("Skins come from the account's own server; nothing is generated locally.", "皮肤来自账户自己的服务器，本地不生成任何假皮肤。"),
            // ── 导航轨底部头像
            AccountRailSignedOut => ("Not signed in", "未登录"),
            AccountDeviceHint => ("Open the link in your browser and enter the code above.", "在浏览器里打开链接，输入上面的用户码。"),
            AccountDeviceBrowserHint => ("Finish the sign-in in the browser window we just opened; the link below also works.", "在我们刚打开的浏览器窗口里完成登录；下面的链接也能手动打开。"),
            AccountServerChecking => ("Checking the server…", "正在校验服务器…"),
            AccountServerOk => ("Server {server} is ready", "服务器 {server} 可用"),
            AccountLoginFailed => ("Sign-in failed", "登录失败"),
            AccountCurrentTitle => ("Current account", "当前账户"),
            // ── 失败原因码（M114 r2：后端只给码，文案在这里；原文只进日志）
            AccountFailureCode => ("Code: {code}", "代码：{code}"),
            AccountFailureDenied => ("The server refused this sign-in (access denied).", "服务器拒绝了这次登录（拒绝访问）。"),
            AccountFailureWhitelist => ("Microsoft has not approved this launcher's app registration yet. An administrator has to accept the app, then try again.", "微软还没批准本启动器的应用注册：需要管理员同意该应用后再试。"),
            AccountFailureUnreachable => ("Could not reach the server. Check the network connection or the address, then try again.", "连不上服务器：检查网络或地址后重试。"),
            AccountFailureNotAuthlib => ("This address does not look like an authlib-injector server (the API root usually ends with /api/yggdrasil/).", "这个地址不像 authlib-injector 服务器（API 根通常以 /api/yggdrasil/ 结尾）。"),
            AccountFailureBadCredentials => ("The server rejected the username or password.", "服务器不认这个用户名或密码。"),
            AccountFailureNoProfile => ("The account has no Minecraft profile on this server yet. Create the character on the server first.", "这个账号在该服务器上还没有角色：先在皮肤站里创建角色。"),
            AccountFailureStore => ("Could not save the account to disk.", "账户没能写到磁盘上。"),
            AccountFailureOther => ("The sign-in failed. The full reason is in the launcher log.", "登录失败，完整原因在启动器日志里。"),
            // M132：缺 client id 是**配置项缺失**，原因就是用户能执行的下一步，所以这一条
            // 必须自己说清「缺什么、怎么补」，不许退回上面那句「完整原因在日志里」。
            // 纪律：只写环境变量名与 `<id>` 占位，**不许出现任何真实 client id**；
            // 也不许写成「未支持 / unsupported」——登录链路是完整的。
            AccountLoginNotConfigured => ("Microsoft sign-in is not configured", "微软登录未配置"),
            AccountFailureMsaNotConfigured => (
                "This build has no Azure public client id, so Microsoft sign-in cannot start (the sign-in flow itself is implemented). Set the environment variable ULL_MSA_CLIENT_ID=<id> before starting the launcher, or build with ULL_MSA_CLIENT_ID=<id>. Then restart the launcher: a running process does not pick up a new environment variable.",
                "本启动器没有配置 Azure 公共客户端 id，微软登录无法开始（登录功能已实现，只差这一项配置）。启动前设置环境变量 ULL_MSA_CLIENT_ID=<id>，或构建时注入：ULL_MSA_CLIENT_ID=<id> cargo build。改完请重启启动器——已在运行的进程读不到新设的环境变量。"
            ),
            AccountDeviceRequesting => ("Contacting Microsoft…", "正在联系微软…"),
            AccountLoginSucceeded => ("Signed in as {name}", "已登录 {name}"),
            // ── 账户页横幅（添加/删除/切换的结果；页面上真实可见，不是 toast）
            AccountNoticeAdded => ("Added {name}", "已添加 {name}"),
            AccountNoticeRemoved => ("Removed {name}", "已删除 {name}"),
            AccountNoticeSwitched => ("{name} will be used for the next launch", "{name} 将作为下次启动的账户"),
            AccountNoticeOfflineNameInvalid => ("Profile names must be 3–16 letters, digits or underscores.", "档案名必须是 3–16 位字母、数字或下划线。"),
            AccountNoticeAccountExists => ("An account with that name already exists.", "同名账户已经存在。"),
            AccountNoticeStoreFailed => ("Could not save the account store. Check the disk and try again.", "账户库写不进去：检查磁盘后重试。"),
            AccountNoticeNotFound => ("That account no longer exists.", "这个账户已经不在了。"),
            AccountNoticeOther => ("The account action failed. The full reason is in the launcher log.", "账户操作失败，完整原因在启动器日志里。"),
            AccountNoticeDismiss => ("Dismiss", "关闭"),
            AccountNoticeTitleFailed => ("Account action failed", "账户操作失败"),
            AccountNoticeTitleDone => ("Account updated", "账户已更新"),
            // ── 重新登录的引导
            AccountHintAuthlibReauth => ("Enter the password again for {server}.", "请输入 {server} 的密码。"),
            AccountHintOfflineNoRelogin => ("Local profiles have no server account to sign in again — create a new profile instead.", "本地档案没有可重新登录的服务器账户，请新建一个档案。"),
        } }
    };
}

pub(crate) use account_entries;

#[cfg(test)]
mod tests {
    use crate::{Catalog, Key, Lang};

    /// **M132**：缺 client id 这条是本页唯一「说清原因」的失败文案，它的纪律比别的正文更硬。
    ///
    /// 抓得住：文案退回「登录失败 / 完整原因在日志里」、退化成「未支持」、或者只说缺 id 却
    ///         不给要设的变量名；也能抓住有人把某个真实 client id 写进词条。
    /// 抓不住：读起来顺不顺（那靠 review 与截图）。
    #[test]
    fn the_missing_client_id_copy_says_what_to_configure_in_both_languages() {
        for &lang in Lang::ALL {
            let catalog = Catalog::of(lang);
            let title = catalog.text(Key::AccountLoginNotConfigured);
            let body = catalog.text(Key::AccountFailureMsaNotConfigured);
            let all = format!("{title}\n{body}");

            assert!(body.contains("ULL_MSA_CLIENT_ID"), "{}: {all}", lang.tag());
            assert!(
                body.contains("<id>"),
                "示例只许用占位：{}: {all}",
                lang.tag()
            );
            // 面板上是「标题 + 正文」一起：结论（未配置）给标题，怎么做给正文。
            assert!(
                all.to_lowercase().contains("not configured") || all.contains("未配置"),
                "要说清「未配置」：{}: {all}",
                lang.tag()
            );
            assert!(!body.to_lowercase().contains("unsupported"), "{all}");
            assert!(
                !body.contains("未支持") && !body.contains("未实现"),
                "{all}"
            );
            // 不许退回「完整原因在日志里」那句（那正是用户报的 bug）
            assert!(!body.contains("launcher log"), "{all}");
            assert!(!body.contains("启动器日志"), "{all}");
            // 硬约束：`ULL_MSA_CLIENT_ID=` 后面只许跟 `<id>` 占位——写进来一个真 id 就红。
            assert_eq!(
                body.matches("ULL_MSA_CLIENT_ID=").count(),
                body.matches("ULL_MSA_CLIENT_ID=<id>").count(),
                "示例里出现了真 id：{all}"
            );
        }
        for &lang in Lang::ALL {
            let catalog = Catalog::of(lang);
            let title = catalog.text(Key::AccountLoginNotConfigured);
            assert!(
                title.to_lowercase().contains("not configured") || title.contains("未配置"),
                "{}: {title}",
                lang.tag()
            );
            assert!(
                !title.contains('=') && !title.contains("ULL_MSA_CLIENT_ID"),
                "标题只报结论，变量名在正文里：{title}"
            );
        }
    }
}
