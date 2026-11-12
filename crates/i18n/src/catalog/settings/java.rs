//! 设置页「Java」分组文案（M175）：扫描 / 运行时列表 / 默认 JDK / 内存 / JVM 参数。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。本域在装配链里排在
//! `settings_appearance_entries` 之后、`settings_download_entries` 之前，所以金表那批行要插在
//! `SettingsAppearanceCornerSuspended` 之后、`SettingsGroupAssistant`（M174 的助手域）之前
//! ——**顺序即 `Key::ALL`**。
//!
//! 三条口径：
//!
//! 1. 技术名词不翻：`JDK` / `JVM` / `-version` / `bin/java` / `java.selected_path` 原样；
//! 2. **不许出现写死的版本号或数字**：本组以前那三行（`Temurin 21.0.4` / `4 GiB`）正是被用户
//!    当成假数据的那三条，现在所有数值都由页面从真读盘的结果里格式化进来；
//! 3. 拒绝原因一句一条（空 / 不存在 / 不是 java / 跑不出 -version / 重复），合成一句
//!    「路径无效」等于让用户自己猜。

macro_rules! settings_java_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 扫描（状态 / 进度 / 取消）
            SettingsJavaScan => ("Scan now", "立即扫描"),
            SettingsJavaScanningStart => ("Scanning…", "正在扫描…"),
            SettingsJavaScanning => ("Scanning… {done}/{total}", "正在扫描… {done}/{total}"),
            SettingsJavaCancelling => ("Cancelling…", "正在取消…"),
            SettingsJavaCancel => ("Cancel", "取消"),
            SettingsJavaCancelled => (
                "The scan was cancelled; the list below is from the previous run.",
                "扫描已取消；下面的列表还是上一次的结果。"
            ),
            SettingsJavaScanDone => ("{count} JDK found", "找到 {count} 个 JDK"),
            SettingsJavaNeverScanned => ("Not scanned yet", "还没扫描过"),
            SettingsJavaNeverScannedBody => (
                "The list is still empty. The launcher scans once by itself when the settings page opens; press Scan now to look again right away.",
                "列表还是空的。打开设置页时启动器会自己扫一次；也可以点「立即扫描」立刻再扫一遍。"
            ),
            SettingsJavaEmptyTitle => ("No Java runtime found", "未检测到 Java 运行时"),
            SettingsJavaEmptyBody => (
                "Nothing matched. Add the path of a JDK you already unpacked below, or install one with ull java install 21 on the command line.",
                "没找到任何 JDK。可以在下面手动加上你自己解压的那份，或者用命令行 ull java install 21 装一个。"
            ),
            // ── 运行时列表
            SettingsJavaDefault => ("Default", "默认"),
            SettingsJavaSetDefault => ("Use as default", "设为默认"),
            SettingsJavaFits => ("Meets the current instance requirement", "满足当前实例的要求"),
            SettingsJavaTooOld => (
                "Below the requirement: this instance needs Java {minimum}+",
                "低于要求：当前实例需要 Java {minimum} 及以上"
            ),
            SettingsJavaDefaultSet => ("Default JDK is now {path}", "默认 JDK 已设为 {path}"),
            SettingsJavaSourceManaged => ("installed by this launcher", "本启动器安装"),
            SettingsJavaSourceEnv => ("JAVA_HOME (environment variable)", "JAVA_HOME（环境变量）"),
            SettingsJavaSourcePath => ("on PATH", "PATH 上"),
            SettingsJavaSourceSystem => ("system install", "系统安装"),
            SettingsJavaSourceManual => ("added by you", "手动添加"),
            // ── 手动添加
            SettingsJavaAdd => ("Add a JDK path", "手动添加 JDK 路径"),
            SettingsJavaAddPlaceholder => (
                "C:\\Program Files\\Java\\jdk-21",
                "例：C:\\Program Files\\Java\\jdk-21"
            ),
            SettingsJavaAddHint => (
                "A JDK folder or its java executable. We run java -version on it; anything that cannot answer is refused.",
                "JDK 目录或它的 java 可执行文件。会真跑一次 java -version；答不出话的一律拒绝。"
            ),
            SettingsJavaAddAction => ("Add", "添加"),
            SettingsJavaAdded => ("Added {path}", "已添加 {path}"),
            // ── 内存
            SettingsJavaMemory => ("Default memory (MB)", "默认内存（MB）"),
            SettingsJavaMemoryHint => (
                "Whole megabytes, {min}–{max}. Instances without their own override follow this value.",
                "整数 MB，范围 {min}~{max}。没有实例级覆盖的实例都用它。"
            ),
            SettingsJavaLayerGlobal => (
                "In effect: {mb} MB, from the global default.",
                "生效：{mb} MB，来自全局默认值。"
            ),
            SettingsJavaLayerInstance => (
                "In effect: {mb} MB, set by instance {instance}. Changing the global value below will not affect that instance.",
                "生效：{mb} MB，由实例 {instance} 覆盖。改下面的全局值不会影响它。"
            ),
            // ── JVM 参数
            SettingsJavaJvm => ("Extra JVM arguments", "额外 JVM 参数"),
            SettingsJavaJvmPlaceholder => ("-XX:+UseG1GC -Dfile.encoding=UTF-8", "例：-XX:+UseG1GC -Dfile.encoding=UTF-8"),
            SettingsJavaJvmHint => (
                "Separated by spaces, quotes allowed. Appended to the command line of every launch; an instance's own JVM arguments come after it.",
                "按空白分隔，支持引号。会拼到每次启动的命令行上；实例自己的 JVM 参数排在它后面。"
            ),
            SettingsJavaJvmSaved => ("Saved value: {value}", "已保存的值：{value}"),
            SettingsJavaJvmSavedNone => (
                "No extra JVM arguments are saved yet.",
                "还没有保存任何额外参数。"
            ),
            // ── 保存与回执
            SettingsJavaSave => ("Save", "保存"),
            SettingsJavaSaved => (
                "Saved and read back from disk. The next launch uses the new value.",
                "已保存，并从磁盘读回确认。下次启动就用新值。"
            ),
            SettingsJavaUnsaved => ("There are unsaved changes.", "有未保存的改动。"),
            SettingsJavaRefused => ("Not saved: {reason}", "未保存：{reason}"),
            SettingsJavaSkipNotDirty => (
                "there was nothing to change, so no file was written",
                "没有任何改动，因此没有写文件"
            ),
            SettingsJavaSkipReadOnly => (
                "the settings file is read-only: a newer launcher wrote it, or it is marked untrusted",
                "配置文件只读：它被更新版本的启动器写过，或被标记为不可信"
            ),
            SettingsJavaSkipUnreadable => (
                "the settings file is untrusted, so it is treated as read-only",
                "配置文件不可信，因此按只读处理"
            ),
            SettingsJavaNoStorage => (
                "No settings file is available in this run, so nothing can be scanned or saved.",
                "这一次运行没有可用的设置文件，扫描与保存都做不了。"
            ),
            // ── 拒绝原因（一条一句）
            SettingsJavaReasonMemoryEmpty => ("the memory field is empty", "内存没填"),
            SettingsJavaReasonMemoryNotNumber => ("the memory must be a whole number of MB", "内存必须是整数 MB"),
            SettingsJavaReasonMemoryRange => (
                "the memory is outside the allowed range",
                "内存超出允许范围"
            ),
            SettingsJavaReasonJvmQuotes => (
                "the JVM arguments have an unclosed quote",
                "JVM 参数里有没配对的引号"
            ),
            SettingsJavaReasonJvmTooLong => (
                "the JVM arguments are longer than the settings file allows (1024 characters)",
                "JVM 参数超过配置文件允许的长度（1024 字符）"
            ),
            SettingsJavaReasonAddEmpty => ("the path is empty", "路径是空的"),
            SettingsJavaReasonAddMissing => ("nothing exists at that path", "那个路径上什么都没有"),
            SettingsJavaReasonAddNotJava => (
                "there is no java executable at that path",
                "那个路径里没有 java 可执行文件"
            ),
            SettingsJavaReasonAddNoVersion => (
                "java -version did not answer there, so it is not a usable JDK",
                "在那里跑不出 java -version，不是一份能用的 JDK"
            ),
            SettingsJavaReasonAddTooLong => (
                "the path is too long to be a JDK location",
                "路径太长，不像是一个 JDK 位置"
            ),
            SettingsJavaReasonAddWriteFailed => (
                "the JDK is fine, but the list file could not be written (disk or permission)",
                "这份 JDK 没问题，但名单文件写不进去（磁盘或权限）"
            ),
            SettingsJavaReasonAddDuplicate => ("that JDK is already in the list", "这份 JDK 已经在列表里了"),
            SettingsJavaReasonWriteFailed => (
                "the settings file could not be written (disk full, no permission, or the value was rejected)",
                "设置写不进去（磁盘满、没权限，或这个值被拒绝）"
            ),
            // ── M197：版本与实例要求的匹配（两向）
            SettingsJavaTooNew => (
                "Above the requirement: this instance can only use Java {maximum} or older (the old loader cannot read newer class files)",
                "高于要求：当前实例只能用 Java {maximum} 及以下（老加载器吃不了新 class 文件）"
            ),
            // ── M197：下载一个 JDK（版本目录 / 下载中 / 失败三态）
            SettingsJavaDownloadTitle => ("Download a JDK", "下载一个 JDK"),
            SettingsJavaDownloadHint => (
                "Downloads go through the built-in mirror table: in China the campus mirrors are tried first (Temurin over Tsinghua TUNA), and GitHub-hosted archives are fetched through gh-proxy. Every archive is verified against its SHA-256 before it is unpacked.",
                "下载走内置镜像表：国内先试院校镜像（Temurin 走清华 TUNA），GitHub 上的归档套 gh-proxy。落盘前一律按 SHA-256 校验。"
            ),
            SettingsJavaDownloadLts => ("LTS", "长期支持版（LTS）"),
            SettingsJavaDownloadBaseline => ("Recommended baseline", "推荐基准版本"),
            SettingsJavaDownloadVendors => ("Vendors: {vendors}", "发行版：{vendors}"),
            SettingsJavaDownloadNoVendor => (
                "No vendor publishes this major version on this platform.",
                "这个平台上没有发行版发这个大版本。"
            ),
            SettingsJavaDownloadAction => ("Install", "安装"),
            SettingsJavaDownloadReinstall => ("Install again", "重装"),
            SettingsJavaDownloadInstalled => ("Already installed: {path}", "已安装：{path}"),
            SettingsJavaDownloadNeeded => (
                "The current instance needs {requirement}.",
                "当前实例需要 {requirement}。"
            ),
            SettingsJavaDownloadFits => (
                "Usable by the current instance",
                "当前实例可用"
            ),
            SettingsJavaDownloadTooLow => (
                "The current instance needs Java {minimum} or newer, so installing this one will not start it.",
                "当前实例需要 Java {minimum} 及以上 —— 装这一档也起不来。"
            ),
            SettingsJavaDownloadTooHigh => (
                "The current instance can only use Java {maximum} or older, so installing this one will not start it.",
                "当前实例只能用 Java {maximum} 及以下 —— 装这一档也起不来。"
            ),
            SettingsJavaDownloadRecommended => (
                "Recommended for the current instance",
                "为当前实例推荐这一档"
            ),
            SettingsJavaDownloadBusy => ("Installing Java {major}…", "正在安装 Java {major}…"),
            SettingsJavaDownloadPhasePreparing => ("Looking up the release…", "正在查版本…"),
            SettingsJavaDownloadPhaseDownloading => (
                "Downloading {percent}% ({done} of {total})",
                "下载中 {percent}%（{done} / {total}）"
            ),
            SettingsJavaDownloadPhaseVerifying => ("Verifying the archive…", "正在校验归档…"),
            SettingsJavaDownloadPhaseExtracting => ("Unpacking…", "正在解压…"),
            SettingsJavaDownloadSource => ("Source: {source}", "来源：{source}"),
            SettingsJavaDownloadRetry => (
                "{source} failed ({reason}); retry {attempt} in {seconds}s",
                "{source} 失败（{reason}）；{seconds} 秒后重试第 {attempt} 次"
            ),
            SettingsJavaDownloadSwitch => ("Switching source: {from} → {to}", "换源：{from} → {to}"),
            SettingsJavaDownloadDone => (
                "Java {major} ({vendor}) is ready at {root} · source {source}",
                "Java {major}（{vendor}）已就绪：{root} · 来源 {source}"
            ),
            SettingsJavaDownloadDoneReused => (
                "Java {major} ({vendor}) was already installed at {root}; nothing was downloaded.",
                "Java {major}（{vendor}）之前已经装好：{root}，这一次没有下载。"
            ),
            SettingsJavaDownloadFailed => (
                "Java {major} could not be installed; the last source tried was {source}. The full reason is in the log.",
                "Java {major} 没装成；最后试到的源是 {source}。完整原因在日志里。"
            ),
            SettingsJavaDownloadFailedNoSource => (
                "Java {major} could not be installed: the version lookup itself failed. The full reason is in the log.",
                "Java {major} 没装成：查版本这一步就失败了。完整原因在日志里。"
            ),
            SettingsJavaDownloadFailedHint => (
                "Check the network, or change the vendor order below.",
                "先查网络，或者改下面的发行版顺序。"
            ),
            SettingsJavaDownloadTriedHeader => ("Sources tried:", "试过的源："),
            SettingsJavaDownloadTried => ("{source}: {reason}", "{source}：{reason}"),
            SettingsJavaDownloadRefused => ("Not started: {reason}", "没开始：{reason}"),
            SettingsJavaReasonDownloadBusy => (
                "another JDK install is already running",
                "已经有一个 JDK 在装了"
            ),
            SettingsJavaReasonDownloadUnknownMajor => (
                "that major version is not in the catalog",
                "目录里没有这个大版本"
            ),
            SettingsJavaReasonDownloadNoVendor => (
                "no vendor publishes that major version here",
                "没有发行版在这里发这个大版本"
            ),
            SettingsJavaReasonRootUnresolved => (
                "the JDK install folder cannot be resolved on this system",
                "这台机器上解析不出 JDK 安装目录"
            ),
            // ── 源失败原因（结构化，`DownloadErrorKind` 的文案面；core 的原句只进日志）
            SettingsJavaSourceReasonNotFound => (
                "this mirror does not have that file (404)",
                "这个镜像没有这个文件（404）"
            ),
            SettingsJavaSourceReasonServer => (
                "the server answered with a 5xx / 429 error",
                "服务器报错 5xx / 429"
            ),
            SettingsJavaSourceReasonTimeout => ("the request timed out", "请求超时"),
            SettingsJavaSourceReasonNetwork => ("the connection failed", "连不上"),
            SettingsJavaSourceReasonIntegrity => ("the checksum did not match", "校验不符"),
            SettingsJavaSourceReasonIo => ("a local disk error", "本地磁盘错误"),
            SettingsJavaSourceReasonOther => ("the request was rejected", "请求被拒"),
            // ── M197：三个设置键（`java.installRoot` / `java.vendorPriority` / `java.auto_install`）
            SettingsJavaInstallRoot => ("JDK install folder", "JDK 安装目录"),
            SettingsJavaInstallRootPlaceholder => ("D:\\Java", "例：D:\\Java"),
            SettingsJavaInstallRootHint => (
                "Leave it empty to follow the data mode: portable keeps JDKs inside the launcher folder, standard uses the system Java folder. A path here overrides both, and JDKs land in <root>/<vendor>/<version>/.",
                "留空 = 跟数据目录模式走：portable 放在启动器目录里，standard 放在系统 Java 目录。填了它覆盖前两者，JDK 落在 <root>/<发行方>/<版本>/。"
            ),
            SettingsJavaInstallRootEffective => ("In effect: {mode} · {path}", "生效：{mode} · {path}"),
            SettingsJavaInstallRootLayerSetting => (
                "this comes from java.installRoot",
                "这一项来自 java.installRoot"
            ),
            SettingsJavaInstallRootLayerDataMode => (
                "this comes from core.data_mode ({mode})",
                "这一项来自 core.data_mode（{mode}）"
            ),
            SettingsJavaInstallRootUnresolved => (
                "The system Java folder cannot be resolved here. Type a folder above, or set core.data_mode back to portable.",
                "这台机器上解析不出系统标准 Java 目录。在上面填一个目录，或把 core.data_mode 改回 portable。"
            ),
            SettingsJavaVendorPriority => ("Vendor order", "发行版顺序"),
            SettingsJavaVendorOrderEffective => ("In effect: {order}", "生效顺序：{order}"),
            SettingsJavaVendorOrderLayerSetting => (
                "this comes from java.vendorPriority",
                "这一项来自 java.vendorPriority"
            ),
            SettingsJavaVendorOrderLayerRegion => (
                "this comes from the built-in order for your region ({region})",
                "这一项来自内置的地区默认序（{region}）"
            ),
            SettingsJavaRegionDomestic => ("China", "国内"),
            SettingsJavaRegionGlobal => ("rest of the world", "国外"),
            SettingsJavaVendorUp => ("Up", "上移"),
            SettingsJavaVendorDown => ("Down", "下移"),
            SettingsJavaVendorReset => ("Reset to the region default", "恢复地区默认序"),
            SettingsJavaVendorSupports => ("ships {majors}", "发 {majors}"),
            SettingsJavaAutoInstall => ("Recommend a JDK when none is found", "缺 JDK 时推荐一档"),
            SettingsJavaAutoInstallHint => (
                "On: the catalog below marks the version the current instance needs, and a freshly installed JDK becomes the default when no default is set yet. Nothing is downloaded on its own — you press Install. Off: nothing is marked and you pick everything yourself.",
                "开：下面的目录会按当前实例的要求标出该装哪一档，装完之后若还没有默认 JDK 就自动设为默认。不会自己开始下载 —— 点「安装」才会下。关：什么也不标，全部自己挑。"
            ),
            SettingsJavaAutoInstallOn => ("on", "开"),
            SettingsJavaAutoInstallOff => ("off", "关"),
            SettingsJavaPrefsSave => ("Save", "保存"),
            SettingsJavaPrefsSaved => (
                "Saved and read back from disk: all three keys are in the settings file.",
                "已保存并从磁盘读回：三个键都进配置文件了。"
            ),
            SettingsJavaPrefsUnsaved => (
                "There are unsaved changes in this section.",
                "这一段有未保存的改动。"
            ),
            SettingsJavaReasonInstallRootTooLong => (
                "the folder path is too long",
                "目录路径太长"
            ),
        } }
    };
}

pub(crate) use settings_java_entries;
