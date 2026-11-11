//! 「Java」分组的一行运行时（M175）：路径 / 版本 / 厂商 / 来源 + 上屏用的截断。
//!
//! 从 `state.rs` 拆出来的（那个文件加了上下文与校验之后到 424 行，越过 400 行硬线）：
//! 这里只放「一行长什么样」，状态与校验仍在 `state.rs`。

use unlucky_i18n::Key;

/// 一个扫到的 / 手动加的运行时（页面要的那几格）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRow {
    /// `bin/java[.exe]` 的绝对路径（也是 `java.selected_path` 的取值）。
    pub binary: String,
    /// `java -version` 的首行（探测失败时为空串）。
    pub version: String,
    /// 主版本号（探测不出为 `None`）。
    pub major: Option<u32>,
    /// 发行方（认不出为 `None`）。
    pub vendor: Option<String>,
    /// 来源 id（`managed` / `env` / `path` / `system` / `manual`）。
    pub source: &'static str,
}

impl RuntimeRow {
    /// 版本号：**能区分 `17.0.13` 与 `17.0.9`** 的那一位，而不是只给大版本（r1 复审 P2-4）。
    ///
    /// 三层取法，逐层降级（不许因为解析不出来就只显示 `17`）：
    /// 1. `-version` 首行引号里的版本号（`openjdk version "21.0.11" …` → `21.0.11`）；
    /// 2. 无引号形态里的第一个版本样 token（`openjdk 25.0.3 2026-04-21` → `25.0.3`）；
    /// 3. 整行原样（原始版本串比一个大版本号有用）；
    /// 4. 连首行都没有（探测失败）→ 大版本；再没有 → `?`。
    #[must_use]
    pub fn version_label(&self) -> String {
        if let Some(quoted) = quoted_version(&self.version) {
            return quoted;
        }
        if let Some(bare) = bare_version(&self.version) {
            return bare;
        }
        let line = self.version.trim();
        if !line.is_empty() {
            return line.to_string();
        }
        self.major
            .map_or_else(|| "?".to_string(), |major| major.to_string())
    }

    /// 版本一句话：`Java 21.0.11 · Microsoft`。
    #[must_use]
    pub fn summary(&self) -> String {
        let version = self.version_label();
        match &self.vendor {
            Some(vendor) => format!("Java {version} · {vendor}"),
            None => format!("Java {version}"),
        }
    }

    /// JDK 安装目录（`…/bin/java.exe` 的上一级再上一级）。
    ///
    /// 上屏用它而不是整条可执行文件路径：424 宽的正文列放不下一条 `C:\Program Files\…\bin\java.exe`，
    /// 而用户认的是 JDK 目录（也是他手动添加时会粘的那一段）。
    #[must_use]
    pub fn home(&self) -> String {
        let path = std::path::Path::new(self.binary.trim());
        let parent = path.parent();
        match parent.filter(|dir| {
            dir.file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
        }) {
            Some(bin) => bin
                .parent()
                .map_or_else(|| self.binary.clone(), |home| home.display().to_string()),
            None => parent.map_or_else(|| self.binary.clone(), |dir| dir.display().to_string()),
        }
    }

    /// 上屏用的目录：过长时保留**尾部**两段，前面用 `…` 收掉。
    ///
    /// 为什么必须截：无空格的 Windows 路径在 iced 的文本排版里不可断行，一条 60 字符的路径会直接
    /// 盖住右边的「设为默认」chip（M175 出图时实测到了这个重叠）。保尾部而不是开头，因为磁盘名
    /// （`C:`）谁都知道，而 `…\Java\temurin\17.0.13` 才是能区分两条 JDK 的那一段。
    #[must_use]
    pub fn display_home(&self) -> String {
        const LIMIT: usize = 39;
        let home = self.home();
        if home.chars().count() <= LIMIT {
            return home;
        }
        let tail: String = home
            .chars()
            .rev()
            .take(LIMIT - 1)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        // 从第一个分隔符之后开始（别让 `…` 后面接着半截目录名）。
        let tail = tail
            .find(['\\', '/'])
            .map_or(tail.clone(), |at| tail[at + 1..].to_string());
        format!("…{}{}", std::path::MAIN_SEPARATOR, tail)
    }

    /// 来源文案 key。
    #[must_use]
    pub fn source_key(&self) -> Key {
        match self.source {
            "managed" => Key::SettingsJavaSourceManaged,
            "env" => Key::SettingsJavaSourceEnv,
            "path" => Key::SettingsJavaSourcePath,
            "manual" => Key::SettingsJavaSourceManual,
            _ => Key::SettingsJavaSourceSystem,
        }
    }
}

/// 引号里的版本号（`openjdk version "21.0.11" 2025-10-21 LTS` → `21.0.11`）。
fn quoted_version(line: &str) -> Option<String> {
    let start = line.find('"')? + 1;
    let rest = &line[start..];
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// 无引号形态里的版本样 token（`openjdk 25.0.3 2026-04-21` → `25.0.3`）。
fn bare_version(line: &str) -> Option<String> {
    line.split_whitespace().find_map(|token| {
        let candidate = token.trim_matches([',', '(', ')', '"']);
        let mut parts = candidate.split('.');
        let first = parts.next()?;
        let looks_like_version = first.chars().all(|c| c.is_ascii_digit())
            && !first.is_empty()
            && parts.next().is_some();
        looks_like_version.then(|| candidate.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::RuntimeRow;

    fn row(version: &str, major: Option<u32>) -> RuntimeRow {
        RuntimeRow {
            binary: "C:/jdk/bin/java.exe".to_string(),
            version: version.to_string(),
            major,
            vendor: Some("Microsoft".to_string()),
            source: "system",
        }
    }

    /// 版本号必须能区分同一大版本的两份 JDK（`17.0.13` vs `17.0.9`），不许只给 `17`。
    ///
    /// 变异证明：把 `version_label` 换回「只用 `major`」，这条立刻红。
    #[test]
    fn the_version_label_keeps_the_minor_digits() {
        assert_eq!(
            row("openjdk version \"17.0.13\" 2024-10-15", Some(17)).version_label(),
            "17.0.13"
        );
        assert_eq!(
            row("openjdk version \"17.0.9\" 2024-01-16", Some(17)).version_label(),
            "17.0.9"
        );
        assert_ne!(
            row("openjdk version \"17.0.13\" 2024-10-15", Some(17)).summary(),
            row("openjdk version \"17.0.9\" 2024-01-16", Some(17)).summary(),
            "两份同大版本 JDK 的摘要必须不同"
        );
        assert_eq!(
            row("openjdk 25.0.3 2026-04-21", Some(25)).version_label(),
            "25.0.3",
            "无引号形态也要认"
        );
        assert_eq!(
            row("something unexpected 21", Some(21)).version_label(),
            "something unexpected 21",
            "解析不出来时给原始版本串，不给光秃秃的大版本"
        );
        assert_eq!(row("", Some(21)).version_label(), "21");
        assert_eq!(row("", None).version_label(), "?");
    }

    #[test]
    fn the_summary_carries_version_and_vendor() {
        assert_eq!(
            row("openjdk version \"21.0.11\" 2025-10-21 LTS", Some(21)).summary(),
            "Java 21.0.11 · Microsoft"
        );
    }
}
