//! 金表的第四段（`tail_ai.rs` 之后、`tail2.rs` 之外新开的一段，M334）。
//!
//! 为什么新开一段而不是往 `tail2.rs` 后面追：`tail2.rs` 到 M331 已有 352 行，再追加十几条
//! 就贴 400 行硬线。口径与前三段完全一致——**只追加、不改既有行**，顺序即 `Key::ALL` 的续段；
//! 拼装见 `golden.rs` 的 `rows()`（少 chain 一段 `catalog::tests` 会当场红，见那里的文档）。
//!
//! 本段内容：M334 的「下载 Java {major}」出口（`catalog/java_install.rs` 那个域），
//! 以及 M336 的「目标实例不可用」那一族（`catalog/pack_host.rs`；宿主侧失败不再拼英文散文）。

pub(super) const ROWS: &[(&str, &str, &str)] = &[
    (
        "JavaGuardDownload",
        "Download Java {major}",
        "下载 Java {major}",
    ),
    (
        "JavaGuardDownloading",
        "Downloading Java {major}…",
        "正在下载 Java {major}…",
    ),
    (
        "JavaGuardDownloadingHint",
        "This one needs a download; this dialog updates when it finishes",
        "这一步要联网下载，装完这个框会自己更新",
    ),
    (
        "JavaGuardDownloadDone",
        "Java {major} is ready",
        "Java {major} 已装好",
    ),
    (
        "JavaGuardDownloadFailed",
        "Java {major} could not be installed ({reason})",
        "Java {major} 没能装上（{reason}）",
    ),
    ("JavaGuardRetryLaunch", "Launch now", "现在启动"),
    (
        "JavaGuardDownloadNoVendor",
        "No vendor ships Java {major}",
        "没有一家发行版提供 Java {major}",
    ),
    (
        "JavaGuardDownloadFailedLocal",
        "The download could not start ({reason})",
        "这次下载没能开始（{reason}）",
    ),
    (
        "JavaVendorTemurin",
        "Eclipse Temurin",
        "Eclipse Temurin 发行版",
    ),
    ("JavaVendorOracle", "Oracle JDK", "Oracle JDK（甲骨文）"),
    (
        "JavaVendorMicrosoft",
        "Microsoft OpenJDK",
        "Microsoft OpenJDK（微软）",
    ),
    ("JavaVendorZulu", "Azul Zulu", "Azul Zulu 发行版"),
    ("JavaVendorTermux", "Termux system package", "Termux 系统包"),
    ("JavaVendorUnknown", "unknown vendor", "未知发行版"),
    // M336：宿主侧「目标实例不可用」那一族（`catalog/pack_host.rs`）。
    // 英文那一列就是以前被硬编码上屏的那句大类名，现在它只在这一张表里出现一次。
    (
        "PackHostTargetUnusable",
        "Target instance is not usable",
        "目标实例不可用",
    ),
    (
        "PackHostOccupied",
        "That folder already exists and is not empty: {detail}",
        "那个目录已经存在且不是空的：{detail}",
    ),
    (
        "PackHostUnreadable",
        "The launcher could not read that folder: {detail}",
        "启动器读不出那个目录：{detail}",
    ),
    (
        "PackHostCreateFailed",
        "The launcher could not create that folder: {detail}",
        "启动器建不出那个目录：{detail}",
    ),
    (
        "PackHostInstanceGone",
        "That instance can no longer be resolved (it may have been removed).",
        "那个实例现在解析不出来（它可能已经被删掉了）。",
    ),
];
