//! 壁纸探测要用的外部 IO：文件读取、文件头、目录列表、子进程，以及宿主根目录。
//!
//! 生产实现是 [`HostIo`]，测试实现是 `fake::FakeIo`（固定夹具，不碰真机壁纸）。
//! 这一层的存在只为把「外部世界」变成注入参数，不含任何平台判断。

use std::path::{Path, PathBuf};

use crate::platform::paths::EnvSource;

/// 探测需要的外部世界：四个方法覆盖三平台的全部读法。
pub trait ProbeIo {
    /// 读整个文件并按 UTF-8 lossy 转文本。
    ///
    /// 二进制 plist 也走这里：路径串本身是可读 ASCII，坏字节会被替换成 U+FFFD 而不影响
    /// 路径串的完整性（macOS 的 `Index.plist` 就是 bplist00）。
    fn read_text(&self, path: &Path) -> Option<String>;

    /// 读文件头最多 `limit` 字节（图片魔数嗅探用；不存在或读不了返回 `None`）。
    fn read_head(&self, path: &Path, limit: usize) -> Option<Vec<u8>>;

    /// 列出目录里的**文件**（非递归、不排序；目录不存在返回空列表）。排序由调用方做。
    fn list_dir(&self, dir: &Path) -> Vec<PathBuf>;

    /// 跑一个外部命令取 stdout（命令缺失 / 非零退出 / 空输出都返回 `None`）。
    fn run(&self, program: &str, args: &[&str]) -> Option<String>;
}

/// 真实宿主 IO。
#[derive(Debug, Clone, Copy, Default)]
pub struct HostIo;

impl ProbeIo for HostIo {
    fn read_text(&self, path: &Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn read_head(&self, path: &Path, limit: usize) -> Option<Vec<u8>> {
        use std::io::Read;
        let file = std::fs::File::open(path).ok()?;
        let mut buffer = vec![0_u8; limit];
        let mut handle = file.take(limit as u64);
        let read = handle.read(&mut buffer).ok()?;
        buffer.truncate(read);
        Some(buffer)
    }

    fn list_dir(&self, dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
            })
            .map(|entry| entry.path())
            .collect()
    }

    fn run(&self, program: &str, args: &[&str]) -> Option<String> {
        let output = std::process::Command::new(program)
            .args(args)
            .output()
            .ok()?;
        // `Command::output` 对非零退出同样返回 `Ok`，所以退出码要单独判：三条命令
        // （`reg.exe` / `gsettings` / `defaults`）出错时都走 stderr，只看 stdout 会把
        // 「查不到」和「命令失败」混成一种，调用方的原因文案就名不副实了。
        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// 探测要用的宿主根目录（全部由环境变量表推出，因此测试可以整体替换）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRoots {
    /// 用户主目录（`HOME`，退 `USERPROFILE`）。
    pub home: PathBuf,
    /// Windows `%APPDATA%`（退 `~/AppData/Roaming`）；其它平台用不到，但仍填上以免分支取空。
    pub app_data: PathBuf,
    /// Windows `%LOCALAPPDATA%`（退 `~/AppData/Local`）：Win10+ 的「当前主题」
    /// `Custom.theme` 落在这一侧。
    pub local_app_data: PathBuf,
    /// `$XDG_CONFIG_HOME`（仅接受绝对路径），退 `~/.config`。
    pub config_home: PathBuf,
    /// Windows `%SystemRoot%`（`.theme` 里 `Wallpaper=%SystemRoot%\...` 展开用），无则不展开。
    pub system_root: Option<PathBuf>,
}

impl HostRoots {
    /// 从环境变量表推出根目录；连主目录都没有时返回 `None`（调用方记一条原因）。
    ///
    /// 顺序固定（`HOME` 优先于 `USERPROFILE`），**不按编译目标分支**：否则在 Windows 上
    /// 跑 Linux 夹具时会被 `USERPROFILE` 抢走主目录，夹具就不再确定。
    #[must_use]
    pub fn from_env(env: &dyn EnvSource) -> Option<Self> {
        let home = env
            .var("HOME")
            .or_else(|| env.var("USERPROFILE"))
            .map(PathBuf::from)?;
        let app_data = env
            .var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"));
        let local_app_data = env
            .var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"));
        let config_home = env
            .var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let system_root = env
            .var("SYSTEMROOT")
            .or_else(|| env.var("WINDIR"))
            .map(PathBuf::from);
        Some(Self {
            home,
            app_data,
            local_app_data,
            config_home,
            system_root,
        })
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use super::ProbeIo;

    /// PNG 魔数（夹具用它冒充「像图片」的文件）。
    pub(crate) const PNG_HEAD: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

    /// 固定夹具 IO：文件内容、图片、目录列表、命令输出全部预先给定。
    ///
    /// 未登记的路径一律「不存在 / 读不出 / 目录为空」，命令也未登记则报失败。
    #[derive(Debug, Default, Clone)]
    pub struct FakeIo {
        texts: BTreeMap<PathBuf, String>,
        images: Vec<PathBuf>,
        dirs: BTreeMap<PathBuf, Vec<PathBuf>>,
        commands: BTreeMap<String, String>,
    }

    impl FakeIo {
        /// 空世界（一切探测都会失败）。
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// 登记一个文本文件（配置文件、`.theme`、plist、xml……）。
        pub(crate) fn with_file(mut self, path: impl AsRef<Path>, text: &str) -> Self {
            self.texts
                .insert(path.as_ref().to_path_buf(), text.to_owned());
            self
        }

        /// 登记一个「图片」文件（`read_head` 返回 PNG 魔数）。
        pub(crate) fn with_image(mut self, path: impl AsRef<Path>) -> Self {
            self.images.push(path.as_ref().to_path_buf());
            self
        }

        /// 登记目录内容（按文件名给，路径拼在 `dir` 上）。
        pub(crate) fn with_dir(mut self, dir: impl AsRef<Path>, names: &[&str]) -> Self {
            let dir = dir.as_ref();
            let entries = names.iter().map(|name| dir.join(name)).collect();
            self.dirs.insert(dir.to_path_buf(), entries);
            self
        }

        /// 登记一条命令的 stdout；`key` 是「程序名 + 空格分隔的参数」整串。
        pub(crate) fn with_command(mut self, key: &str, stdout: &str) -> Self {
            self.commands.insert(key.to_owned(), stdout.to_owned());
            self
        }
    }

    fn command_key(program: &str, args: &[&str]) -> String {
        let mut key = program.to_owned();
        for arg in args {
            key.push(' ');
            key.push_str(arg);
        }
        key
    }

    impl ProbeIo for FakeIo {
        fn read_text(&self, path: &Path) -> Option<String> {
            self.texts.get(path).cloned()
        }

        fn read_head(&self, path: &Path, limit: usize) -> Option<Vec<u8>> {
            if self.images.iter().any(|image| image.as_path() == path) {
                return Some(PNG_HEAD[..limit.min(PNG_HEAD.len())].to_vec());
            }
            let text = self.texts.get(path)?;
            let bytes = text.as_bytes();
            Some(bytes[..limit.min(bytes.len())].to_vec())
        }

        fn list_dir(&self, dir: &Path) -> Vec<PathBuf> {
            self.dirs.get(dir).cloned().unwrap_or_default()
        }

        fn run(&self, program: &str, args: &[&str]) -> Option<String> {
            let key = command_key(program, args);
            let stdout = self.commands.get(&key)?;
            if stdout.is_empty() {
                return None;
            }
            Some(stdout.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::paths::MapEnv;

    #[test]
    fn roots_come_from_env_in_fixed_order() {
        let env = MapEnv::new()
            .with("HOME", "/home/u")
            .with("USERPROFILE", "C:/Users/ignored")
            .with("SYSTEMROOT", "C:/Windows");
        let roots = HostRoots::from_env(&env).expect("HOME 已给");
        assert_eq!(roots.home, PathBuf::from("/home/u"));
        assert_eq!(roots.app_data, PathBuf::from("/home/u/AppData/Roaming"));
        assert_eq!(roots.local_app_data, PathBuf::from("/home/u/AppData/Local"));
        assert_eq!(roots.config_home, PathBuf::from("/home/u/.config"));
        assert_eq!(roots.system_root, Some(PathBuf::from("C:/Windows")));
    }

    #[test]
    fn roots_prefer_explicit_appdata_and_absolute_xdg() {
        let env = MapEnv::new()
            .with("HOME", "/home/u")
            .with("APPDATA", "C:/Users/u/AppData/Roaming")
            .with("LOCALAPPDATA", "C:/Users/u/AppData/Local")
            // 相对路径按 XDG 规范忽略。
            .with("XDG_CONFIG_HOME", "relative/dir");
        let roots = HostRoots::from_env(&env).expect("HOME 已给");
        assert_eq!(roots.app_data, PathBuf::from("C:/Users/u/AppData/Roaming"));
        assert_eq!(
            roots.local_app_data,
            PathBuf::from("C:/Users/u/AppData/Local")
        );
        assert_eq!(roots.config_home, PathBuf::from("/home/u/.config"));
        assert_eq!(roots.system_root, None);
    }

    #[test]
    fn host_io_treats_non_zero_exit_as_failure() {
        // 有 stdout 但退出码非零：只看 stdout 的旧实现会把这种输出当成功。
        #[cfg(windows)]
        let (program, args) = ("cmd", ["/c", "echo hi & exit 3"]);
        #[cfg(not(windows))]
        let (program, args) = ("sh", ["-c", "echo hi; exit 3"]);
        assert!(HostIo.run(program, &args).is_none());
    }

    #[test]
    fn fake_io_serves_only_registered_world() {
        let io = fake::FakeIo::new()
            .with_file("/home/u/note.txt", "hello")
            .with_image("/home/u/a.png")
            .with_dir("/home/u/dir", &["a.png", "b.theme"])
            .with_command("gsettings get x y", "'file:///home/u/a.png'");
        assert_eq!(
            io.read_text(Path::new("/home/u/note.txt")).as_deref(),
            Some("hello")
        );
        assert_eq!(
            io.read_head(Path::new("/home/u/note.txt"), 4).as_deref(),
            Some(&b"hell"[..])
        );
        assert_eq!(
            io.read_head(Path::new("/home/u/a.png"), 8).as_deref(),
            Some(&fake::PNG_HEAD[..8])
        );
        assert_eq!(io.list_dir(Path::new("/home/u/dir")).len(), 2);
        assert!(io.run("gsettings", &["get", "x", "y"]).is_some());
        assert!(io.run("reg.exe", &["query"]).is_none());
        assert!(io.read_text(Path::new("/nope")).is_none());
    }
}
