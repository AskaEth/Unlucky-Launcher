//! 游戏根的发现与去重（M274）：**当前配置的根** + **官方启动器的标准位置**。
//!
//! 用户原话：「其他第三方启动器甚至能自动读到官方启动器的版本和存档」。官方启动器用的是
//! `%APPDATA%\.minecraft` / `~/Library/Application Support/minecraft` / `~/.minecraft`，
//! 而便携模式下的 ULL 用的是 `<exe>/.minecraft`——**两个根，不是一回事**。所以列表要能同时读两边。
//!
//! # 只有一件事不是显然的：什么算「同一个根」
//!
//! 同一个目录可以有四种写法：大小写不同、`\` 与 `/` 混用、8.3 短名（`PROGRA~1`）、
//! 符号链接 / 目录联接（Windows 的 junction）。按字面量比较会让**同一批实例列两遍**，
//! 所以判据落在 [`key_of`] 的**规范化真实路径**上：
//!
//! 1. `canonicalize`（Windows 上是 `GetFinalPathNameByHandle`：短名展开成长名，联接解析到目标）；
//! 2. `\` → `/`；
//! 3. **去掉 `\\?\` 前缀**——Windows 的 `canonicalize` 一律返回 verbatim 形式（`\\?\C:\…`），
//!    不去掉的话「同一路径的两种写法」永远不相等（这是个已知坑，用例
//!    `junction_to_the_same_folder_is_one_root` 钉住）；
//! 4. Windows / macOS 折成小写。
//!
//! # 已知取舍（这是**我们**的选择，不是文件系统性质）
//!
//! 第 4 步折小写是为了让 `C:\Mc` 与 `c:\mc`（Windows 上同一个目录）算同一个根。代价在 macOS：
//! **APFS 可以按卷选择区分大小写**，那种卷上 `/a` 与 `/A` 是两个真实不同的目录，折小写会把它们
//! 并成一个键 ⇒ 第二个根（官方位置）被**静默跳过**（不报错、不列它的实例）。触发条件只有
//! 「用户在区分大小写的 APFS 卷上，且两个根只差大小写」——那种布局几乎不可能是真实安装，
//! 所以接受这个代价：宁可漏一个根，也不要把同一批实例列两遍（后者是更常见的错）。
//!
//! 另外三条边界（都不是缺陷，记在这里免得下次重新推一遍）：
//! - `\\?\UNC\server\share` 剥前缀后成 `unc/server/share`：仍然**唯一**、不会误合并，
//!   只是丢了 UNC 语义（键只用于相等比较，不再当路径用）；
//! - `\\?\C:\` 去掉尾斜杠之后是 `c:`：与 `c:/foo` 这类前缀不相同，不会撞；
//! - Linux 的 `/`：去尾斜杠会得到空串，有兜底写回 `"/"`。
//!
//! `canonicalize` 失败（根还不存在）时退化成「字面量归一」：`\`→`/`、去尾斜杠、折小写。
//! 尚不存在的目录没有短名/链接可解析，这一步够用；代价是那个目录**创建之后**键可能变一次
//! （短名 → 长名、大小写归到真实名字），于是该根下已发现的实例 id 会变一次。
//!
//! # 只读
//!
//! 本模块只 `is_dir()` / `canonicalize()`：不建目录、不写一个字节。官方启动器那个根上尤其要紧
//! ——那是**别人的**目录。（唯一的例外是 `#[cfg(test)]` 的 [`make_dir_link`]：它只在用例自己的
//! 临时目录里造链接，不属于任何产品路径。）

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use unlucky_core::game::GameLayout;

use super::super::env::HostEnv;

/// 一个被发现的游戏根。
///
/// `configured` 那一格区分「用户在设置里指定的根」与「官方启动器的标准位置」：档案绑定、
/// 新建/安装版本都只对前者有意义（我们**从不往官方根写东西**，见 M274 的只读纪律）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::host) struct GameRoot {
    key: String,
    game: GameLayout,
    configured: bool,
}

impl GameRoot {
    /// 这个根的布局（`versions/` / `libraries/` / `assets/` 都在它下面）。
    pub(in crate::host) fn game(&self) -> &GameLayout {
        &self.game
    }

    /// 规范化后的比较键（实例 id 的哈希输入之一，见 [`super::discovered::external_instance_id`]）。
    pub(in crate::host) fn key(&self) -> &str {
        &self.key
    }

    /// 是不是**当前配置**的那个根（`env.game`）。
    pub(in crate::host) fn configured(&self) -> bool {
        self.configured
    }

    /// 根目录本身（显示用）。
    pub(in crate::host) fn path(&self) -> &Path {
        self.game.root()
    }
}

/// 全部游戏根：**配置根永远第一位**，官方启动器的标准位置（存在、且不是同一个物理目录）第二位。
///
/// 官方位置不存在 ⇒ 不列。「没装官方启动器」不该在列表上留一个空根——那是「这里本来就没有」，
/// 与「有个空目录」是两件事。
pub(in crate::host) fn all(env: &HostEnv) -> Vec<GameRoot> {
    let mut roots = vec![GameRoot {
        key: key_of(env.game.root()),
        game: env.game.clone(),
        configured: true,
    }];
    let mut seen: BTreeSet<String> = roots.iter().map(|root| root.key.clone()).collect();
    for extra in env.standard_game.iter() {
        if !extra.is_dir() {
            continue;
        }
        let key = key_of(extra);
        // 同一个物理目录（符号链接 / 目录联接 / 大小写 / 短名）⇒ 只留配置根那一条。
        if !seen.insert(key.clone()) {
            continue;
        }
        roots.push(GameRoot {
            key,
            game: GameLayout::new(extra.clone()),
            configured: false,
        });
    }
    roots
}

/// 路径 → **比较键**（判据的完整理由见模块文档）。
///
/// 同一个物理目录的所有写法必须给同一个键；不同的目录必须给不同的键（大小写不敏感的平台除外，
/// 那里它们本来就是同一个目录）。
pub(in crate::host) fn key_of(path: &Path) -> String {
    let resolved: PathBuf = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy().replace('\\', "/");
    // verbatim 前缀（`\\?\C:\…` 在上一步已经变成 `//?/C:/…`）：不去掉就与普通写法永不相等。
    // UNC 会因此变成 `unc/server/share`（仍唯一，见模块文档的「已知取舍」）。
    let text = text
        .strip_prefix("//?/")
        .or_else(|| text.strip_prefix("//./"))
        .unwrap_or(text.as_str());
    let text = text.trim_end_matches('/');
    let text = if text.is_empty() { "/" } else { text };
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text.to_string()
    }
}

/// 造一个目录链接：Windows 用 junction（无管理员权限也能建），Unix 用符号链接。造不出来返回 `false`。
///
/// 模块级而不是 `mod tests` 里，因为 [`super::tests_multiroot`] 的判据②也要造链接（M291）：两处
/// 各写一份一字不差的 `cmd /c mklink /J` 只是把同一个坑埋两遍，`child_quiet_guard` 也是一次抓两处。
/// 可见性只开到 `super`（`host::game`）——两个调用方都是它的直接子模块。
///
/// 走 [`unlucky_core::platform::child::quiet`]：本仓库的 exe 是 GUI 程序（PE Subsystem = 2），
/// spawn `cmd.exe` 这种控制台程序时 Windows 会新开一块控制台（桌面上「黑框闪一下」）。
/// 语义不变：只看退出码（`.status()`），不读输出，失败一律 `false`。
#[cfg(all(test, windows))]
pub(super) fn make_dir_link(link: &Path, target: &Path) -> bool {
    unlucky_core::platform::child::quiet("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(all(test, not(windows)))]
pub(super) fn make_dir_link(link: &Path, target: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{tag}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("建临时目录");
        path
    }

    /// 斜杠方向 / 尾斜杠 / 大小写不影响比较键（`canonicalize` 之后三者本来就该相等）。
    #[test]
    fn key_ignores_separators_case_and_trailing_slash() {
        let root = temp_root("ull-roots-key");
        let nested = root.join("Minecraft/.minecraft");
        std::fs::create_dir_all(&nested).expect("建目录");
        let plain = key_of(&nested);
        assert_eq!(plain, key_of(&PathBuf::from(format!("{}/", plain))));
        let mixed = PathBuf::from(plain.replace('/', "\\"));
        assert_eq!(plain, key_of(&mixed), "反斜杠写法必须是同一个根");
        assert!(
            !plain.starts_with("//?/"),
            "verbatim 前缀没去掉，两种写法永远不相等：{plain}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// **去重判据**：`root_b` 是指向 `root_a` 的目录联接（junction）⇒ 两个写法是同一个根。
    ///
    /// 这一条是整个多根发现最容易假绿的地方：只比字面量时它必然「通过」两条，而用户看到同一批
    /// 实例列了两遍。Windows 上用 `mklink /J`（不需要管理员），其它平台用符号链接。
    #[test]
    fn junction_to_the_same_folder_is_one_root() {
        let root = temp_root("ull-roots-junction");
        // 逐段 `join`：`join("official/.minecraft")` 会留下正斜杠，而 `cmd` 把 `/` 当开关前缀，
        // `mklink` 于是报「无效语法」⇒ 这条用例一直走的是下面的「跳过」分支（M291 实测）。
        let target = root.join("official").join(".minecraft");
        std::fs::create_dir_all(target.join("versions")).expect("建目录");
        let link = root.join("linked");
        if !make_dir_link(&link, &target) {
            // 造不出链接就**不假绿**：这一条判据在没链接的盘上等于没验，如实标出来。
            eprintln!("跳过：这个环境造不出目录链接（{}）", link.display());
            std::fs::remove_dir_all(&root).ok();
            return;
        }
        assert_eq!(
            key_of(&target),
            key_of(&link),
            "联接与目标是同一个根：{} vs {}",
            key_of(&target),
            key_of(&link)
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
