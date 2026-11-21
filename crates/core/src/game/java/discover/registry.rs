//! `discover` 的「注册表」这一支：从 `HKLM` 里读出 JDK 的安装根。
//!
//! 两条来源，都按 64 位 + 32 位视图各查一遍（`reg query <键> /s /reg:64|32`）：
//!
//! | 键 | 值 | 谁写的 |
//! | --- | --- | --- |
//! | `HKLM\SOFTWARE\JavaSoft\JDK\*` | `JavaHome`（`REG_SZ`） | 老 Oracle 安装器；新发行版通常不写 |
//! | `HKLM\SOFTWARE\Microsoft\JDK\<ver>\<flavor>\MSI` | `Path`（`REG_SZ`） | Microsoft OpenJDK 的 MSI |
//!
//! 第二条是本模块存在的理由：MS JDK 装到**自定义目录**（不在 Program Files 下）时，
//! 扫目录那支看不见它，只有注册表知道它在哪。
//!
//! 键不存在是常态（本机实测 `JavaSoft\JDK` 就没有）→ 静默跳过，绝不报错。
//! 读注册表走真 `reg.exe`（Win10+ 恒定存在），但**解析与合并都是纯函数 + [`RegSource`]
//! 注入**：单测喂真实 `reg query` 的输出样本，不依赖本机到底装了什么。

use std::path::PathBuf;

use crate::platform::child::quiet;

/// 老 Oracle 安装器登记 `JavaHome` 的键。
const JAVASOFT_KEY: &str = r"HKLM\SOFTWARE\JavaSoft\JDK";

/// Microsoft OpenJDK 的 MSI 登记安装路径的键（`<ver>\<flavor>\MSI` 下面才是 `Path`）。
const MICROSOFT_KEY: &str = r"HKLM\SOFTWARE\Microsoft\JDK";

/// 注册表视图，两个都要试：32 位 JDK 的 MSI 只写 32 位视图。
const VIEWS: [&str; 2] = ["64", "32"];

/// 一次 `reg query /s` 的原始输出来源。
///
/// `None` 与「键不存在」同义——调用方一律当「这一支没有东西」，不区分「查不到」与「查不了」。
pub trait RegSource {
    /// 查 `key` 在 `view`（`"64"` / `"32"`）视图下的递归输出。
    fn key_output(&self, key: &str, view: &str) -> Option<String>;
}

/// 真 `reg.exe`（生产路径）。
pub struct ProcessReg;

impl RegSource for ProcessReg {
    fn key_output(&self, key: &str, view: &str) -> Option<String> {
        let output = quiet("reg")
            .args(["query", key, "/s", "/reg:"])
            .arg(view)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// 注册表里登记的 JDK 根目录（两条来源合并，稳定排序去重；查不到返回空）。
#[must_use]
pub fn registry_homes() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    registry_homes_from(&ProcessReg)
}

/// 同 [`registry_homes`]，但注册表可注入（测试喂假输出，不碰本机）。
#[must_use]
pub fn registry_homes_from(reg: &dyn RegSource) -> Vec<PathBuf> {
    let mut homes = query_views(reg, JAVASOFT_KEY, parse_java_home);
    homes.extend(query_views(reg, MICROSOFT_KEY, parse_msi_paths));
    homes.sort();
    homes.dedup();
    homes
}

/// 一个键在两个视图下各查一次，解析结果拼接（单个视图失败不影响另一个）。
fn query_views(reg: &dyn RegSource, key: &str, parse: fn(&str) -> Vec<PathBuf>) -> Vec<PathBuf> {
    let mut homes = Vec::new();
    for view in VIEWS {
        if let Some(output) = reg.key_output(key, view) {
            homes.extend(parse(&output));
        }
    }
    homes
}

/// 从 `reg query /s` 的输出里抠出 `JavaHome` 的值（纯函数，便于用真实输出做单测）。
#[must_use]
pub fn parse_java_home(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("JavaHome")?;
            let (_, value) = rest.split_once("REG_SZ")?;
            let home = value.trim();
            (!home.is_empty()).then(|| PathBuf::from(home))
        })
        .collect()
}

/// 末尾分隔符**纯文本**归一（`\` 只在 Windows 上是分隔符：借 `Path` 语义会让 Linux 得出另一条路径，M380）。
fn trim_trailing_separators(value: &str) -> &str {
    let trimmed = value.trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        value
    } else {
        trimmed
    }
}

/// 从 `reg query /s` 的输出里抠出 MS JDK 的 `Path` 值。
///
/// 判据是**当前键的最后一段叫 `MSI`**（大小写不限），不是数层数——`<ver>\<flavor>\MSI`
/// 是标准层深，而安装器版本不同时会多一层（`<flavor>\x64\MSI`）或某一层为空，
/// 只要键尾还是 `MSI` 就照收。同层里别的值（`Main`/`EnvironmentPath` 这些 `REG_DWORD`）
/// 以及非 `MSI` 键下的 `Path` 都不认。
///
/// 值里的末尾分隔符在这一层就归一（安装器写不写 `\` 都有）⇒ 同一个 home 的两种写法在**任何平台**上都是同一个 `PathBuf`。
#[must_use]
pub fn parse_msi_paths(output: &str) -> Vec<PathBuf> {
    let mut in_msi = false;
    let mut paths = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("HKEY_") {
            in_msi = trimmed
                .rsplit('\\')
                .next()
                .is_some_and(|last| last.eq_ignore_ascii_case("MSI"));
            continue;
        }
        if !in_msi {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("Path") else {
            continue;
        };
        // 值名得整段叫 `Path`（`PathFoo REG_SZ ...` 不算）：名、类型、数据之间是空白。
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((_, value)) = rest.split_once("REG_SZ") else {
            continue;
        };
        let path = trim_trailing_separators(value.trim());
        if !path.is_empty() {
            paths.push(PathBuf::from(path));
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use super::{parse_java_home, parse_msi_paths, registry_homes_from, RegSource};

    use crate::game::java::discover::java_binaries_in;

    /// 真实 `reg query "HKLM\SOFTWARE\JavaSoft\JDK" /s` 的输出样本（本机没有这个键，
    /// 所以这段是旧机上的实测形态，M49 起就钉在测试里）。
    const JAVASOFT_SAMPLE: &str = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\JavaSoft\\JDK\r\n    CurrentVersion    REG_SZ    1.8\r\n\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\JavaSoft\\JDK\\1.8\r\n    JavaHome    REG_SZ    C:\\Program Files\\Java\\jdk1.8.0_402\r\n";

    /// 真实 `reg query "HKLM\SOFTWARE\Microsoft\JDK" /s /reg:64` 的输出样本
    /// （本机实测原文，21 与 25 两条，`Path` 末尾带反斜杠；同键里 `JavaHome` 是 `REG_DWORD`）。
    const MICROSOFT_SAMPLE: &str = concat!(
        "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\21.0.11.10\r\n\r\n",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\21.0.11.10\\hotspot\r\n\r\n",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\21.0.11.10\\hotspot\\MSI\r\n",
        "    Path    REG_SZ    C:\\Program Files\\Microsoft\\jdk-21.0.11.10-hotspot\\\r\n",
        "    Main    REG_DWORD    0x1\r\n",
        "    EnvironmentPath    REG_DWORD    0x1\r\n",
        "    JavaHome    REG_DWORD    0x1\r\n\r\n",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\25.0.3.9\r\n\r\n",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\25.0.3.9\\hotspot\r\n\r\n",
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\25.0.3.9\\hotspot\\MSI\r\n",
        "    Path    REG_SZ    C:\\Program Files\\Microsoft\\jdk-25.0.3.9-hotspot\\\r\n",
        "    Main    REG_DWORD    0x1\r\n",
    );

    /// `reg query` 的「键不存在」原文（GBK 解出来仍是这句）。
    const MISSING_KEY: &str = "错误: 系统找不到指定的注册表项或值。\r\n";

    /// 假注册表：给 (键, 视图) 配输出，没配的当「键不存在」。
    struct FakeReg {
        replies: Vec<(&'static str, &'static str, String)>,
    }

    impl FakeReg {
        fn new(replies: Vec<(&'static str, &'static str, String)>) -> Self {
            Self { replies }
        }
    }

    impl RegSource for FakeReg {
        fn key_output(&self, key: &str, view: &str) -> Option<String> {
            self.replies
                .iter()
                .find(|(k, v, _)| *k == key && *v == view)
                .map(|(_, _, output)| output.clone())
        }
    }

    /// 记录被查过的 (键, 视图)，一律回「没有」。
    #[derive(Default)]
    struct Recorder {
        seen: RefCell<Vec<(String, String)>>,
    }

    impl RegSource for Recorder {
        fn key_output(&self, key: &str, view: &str) -> Option<String> {
            self.seen
                .borrow_mut()
                .push((key.to_owned(), view.to_owned()));
            None
        }
    }

    fn java_file() -> &'static str {
        crate::game::java::extract::java_binary_name()
    }

    #[test]
    fn parses_real_java_soft_output() {
        assert_eq!(
            parse_java_home(JAVASOFT_SAMPLE),
            vec![PathBuf::from(r"C:\Program Files\Java\jdk1.8.0_402")]
        );
    }

    #[test]
    fn parses_real_microsoft_msi_output() {
        // 同键里的 `JavaHome    REG_DWORD` 不许被当成 JavaSoft 那支的 `JavaHome`。
        assert!(
            parse_java_home(MICROSOFT_SAMPLE).is_empty(),
            "MSI 键里的 JavaHome 是 REG_DWORD，JavaSoft 的解析器不该收"
        );
        assert_eq!(
            parse_msi_paths(MICROSOFT_SAMPLE),
            // 样本里 `Path` 末尾带 `\`（安装器原文），断言按**归一后**形态：写回带 `\` 的字面量 Windows 上相等、Linux 不等。
            vec![
                PathBuf::from(r"C:\Program Files\Microsoft\jdk-21.0.11.10-hotspot"),
                PathBuf::from(r"C:\Program Files\Microsoft\jdk-25.0.3.9-hotspot"),
            ]
        );
    }

    #[test]
    fn missing_registry_key_is_not_an_error() {
        assert!(parse_java_home(MISSING_KEY).is_empty());
        assert!(parse_msi_paths(MISSING_KEY).is_empty());
        // 两个键都查不到 → 空表，不是错误。
        let reg = FakeReg::new(Vec::new());
        assert!(registry_homes_from(&reg).is_empty());
    }

    #[test]
    fn msi_path_with_and_without_trailing_backslash_is_the_same_home() {
        // 安装器写不写末尾反斜杠都有（本机 21/25 两条带），拼 `bin\java.exe` 的结果必须一致。
        let with = parse_msi_paths(
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\21\\hotspot\\MSI\r\n    Path    REG_SZ    C:\\Program Files\\Microsoft\\jdk-21\\\r\n",
        );
        let without = parse_msi_paths(
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\21\\hotspot\\MSI\r\n    Path    REG_SZ    C:\\Program Files\\Microsoft\\jdk-21\r\n",
        );
        assert_eq!(with, without, "末尾反斜杠只是写法差异");
        assert_eq!(with[0].join("bin"), without[0].join("bin"));
        // 两条同时出现（两条支/两个视图各报一次）也要收敛成一条。
        let mut both = with.clone();
        both.extend(without);
        both.sort();
        both.dedup();
        assert_eq!(both.len(), 1);
        // 归一落在**文本**层：`PathBuf` 相等在 Windows 上会掩盖「没归一」，所以直接看文本。
        let text = both[0].to_string_lossy();
        assert_eq!(text, r"C:\Program Files\Microsoft\jdk-21");
    }

    #[test]
    fn trim_keeps_drive_root() {
        // 过度归一的反例：盘符根那唯一一个分隔符必须留着（`C:` 是「C 盘当前目录」，不是根）。
        let root = super::trim_trailing_separators(r"C:\");
        assert_eq!(root, r"C:\");
    }

    #[test]
    fn msi_paths_survive_irregular_version_layers() {
        let output = concat!(
            // 多一层：`<ver>\<flavor>\x64\MSI`
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\21.0.11.10\\hotspot\\x64\\MSI\r\n",
            "    Path    REG_SZ    D:\\jdk\\ms-21\\\r\n\r\n",
            // 空一层：版本目录名空着，直接 `<flavor>\MSI`
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\\\hotspot\\MSI\r\n",
            "    Path    REG_SZ    D:\\jdk\\ms-empty-ver\r\n\r\n",
            // 键尾不是 MSI → 那个 `Path` 不认
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\25.0.3.9\\hotspot\r\n",
            "    Path    REG_SZ    D:\\jdk\\not-an-msi-layer\r\n\r\n",
            // 值名不是整段 `Path` → 不认
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\26\\hotspot\\MSI\r\n",
            "    PathExtra    REG_SZ    D:\\jdk\\path-extra\r\n",
        );
        assert_eq!(
            parse_msi_paths(output),
            vec![
                PathBuf::from(r"D:\jdk\ms-21"),
                PathBuf::from(r"D:\jdk\ms-empty-ver"),
            ]
        );
    }

    #[test]
    fn both_registry_views_are_queried_for_both_keys() {
        // 与 JavaSoft 那支同口径：两个键 × 两个视图，一个都不能省。
        // 键名在这里写**字面量**（不用常量）：常量写错、键名被改，这两条就红。
        let reg = Recorder::default();
        let _ = registry_homes_from(&reg);
        let seen = reg.seen.borrow();
        for key in [
            r"HKLM\SOFTWARE\JavaSoft\JDK",
            r"HKLM\SOFTWARE\Microsoft\JDK",
        ] {
            for view in ["64", "32"] {
                assert!(
                    seen.iter().any(|(k, v)| k == key && v == view),
                    "少查了 {key} /reg:{view}：{seen:?}"
                );
            }
        }
    }

    #[test]
    fn injection_registry_only_lists_the_ms_jdks() {
        // 注入式验收：Program Files / PATH / JAVA_HOME 三个来源全屏蔽（根表给空），
        // 只让注册表这一支出声——本机装没装 MS JDK 不参与判定。
        let base = std::env::temp_dir().join(format!("ull-ms-jdk-{}", std::process::id()));
        let mut output = String::from("\r\n");
        let mut expected: Vec<PathBuf> = Vec::new();
        for version in ["21.0.11.10", "25.0.3.9"] {
            let home = base.join(format!("jdk-{version}-hotspot"));
            std::fs::create_dir_all(home.join("bin")).unwrap();
            std::fs::write(home.join("bin").join(java_file()), b"").unwrap();
            // 照实测形态写：`Path` 末尾带反斜杠，注册表里是 Windows 分隔符。
            output.push_str(&format!(
                "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\JDK\\{version}\\hotspot\\MSI\r\n    Path    REG_SZ    {}{}\r\n\r\n",
                home.display(),
                std::path::MAIN_SEPARATOR
            ));
            expected.push(home.join("bin").join(java_file()));
        }

        // `JavaSoft` 与 32 位视图都「键不存在」：只有 MS 键的 64 位视图有东西。
        // 键名用字面量：常量被改坏（比如写成 `Microsoft\JRE`）时这一条必须红。
        let reg = FakeReg::new(vec![(r"HKLM\SOFTWARE\Microsoft\JDK", "64", output)]);
        let homes = registry_homes_from(&reg);
        assert_eq!(homes.len(), 2, "MSI 的两条登记都要抠出来：{homes:?}");

        let binaries = java_binaries_in(&[], &homes);
        assert_eq!(binaries, expected, "只走注册表也要列出这两条 MS JDK");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn same_jdk_from_two_branches_is_listed_once() {
        // 扫目录那支与注册表那支同时命中同一个 JDK：名字里多了末尾反斜杠也只是写法差异。
        let base = std::env::temp_dir().join(format!("ull-ms-dup-{}", std::process::id()));
        let home = base.join("jdk-21.0.11.10-hotspot");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(home.join("bin").join(java_file()), b"").unwrap();

        let from_registry = format!("{}{}", home.display(), std::path::MAIN_SEPARATOR);
        let roots = [home.clone()];
        let homes = [PathBuf::from(&from_registry), home.clone()];
        let binaries = java_binaries_in(&roots, &homes);
        assert_eq!(binaries.len(), 1, "同一个 JDK 不许列两次：{binaries:?}");
        assert_eq!(binaries[0], home.join("bin").join(java_file()));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn registry_homes_are_absolute_on_this_machine() {
        // 真跑一遍 `reg.exe`（本机有没有 JDK 都算通过）：路径必须绝对、且已去重。
        let homes = super::registry_homes();
        for home in &homes {
            assert!(Path::new(home).is_absolute(), "注册表路径应绝对：{home:?}");
        }
        let mut keys: Vec<String> = homes
            .iter()
            .map(|home| crate::game::java::discover::path_key(home))
            .collect();
        let total = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), total, "注册表结果不该重复：{homes:?}");
    }
}
