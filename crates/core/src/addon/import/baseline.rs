//! 三方合并的 **base 侧**：上次导入写下的"包给的哈希"。
//!
//! # 为什么必须落盘
//!
//! 三方合并的三个输入是：`theirs`（这次包里给的）、`ours`（磁盘现状）、`base`（**上次**包里
//! 给的）。`base` 不能从这次的包现算——那是 `theirs`；也不能从磁盘现算——那是 `ours`。少了
//! `base` 就分不清两种截然不同的事实：
//!
//! - 磁盘内容 ≠ 新包内容，且磁盘内容 == 上次包内容 ⇒ **整合包升级**，该覆盖（用户没动过）；
//! - 磁盘内容 ≠ 新包内容，且磁盘内容 ≠ 上次包内容 ⇒ **用户改过**，不许静默覆盖。
//!
//! 所以第一次导入就在实例根写一份基线；没有基线时（首次导入 / 用户手动塞进来的文件 / 基线
//! 属于别的整合包）一律按"磁盘上已有同名文件"处理 —— 不覆盖、进冲突清单（最安全的一侧）。
//!
//! # 落点：**唯一来源是 [`crate::game::layout::GameLayout::ull_root`]**
//!
//! `<实例根>/.ull/pack-baseline.json`（游戏根内的启动器私有目录）。
//!
//! 这里有个踩过的坑（复审 r5 P1-1）：基线最初写成 `<实例根>/.ULL/…`（用了
//! [`crate::platform::layout::LAUNCHER_HOME_DIR`]，那是**启动器数据根**的名字，在 exe/portable
//! 那一层），而读侧用 `GameLayout::ull_root()`（`<实例根>/.ull`，小写，实例**游戏根**内的
//! 私有目录）。Windows / macOS 的大小写不敏感文件系统上两者是同一个文件，本机全绿；ext4 上
//! 是两个文件 ⇒ 基线永远读不到（`UserModified`/`UserDeleted` 永不出现）。
//!
//! 现在**读写都从 `ull_root()` 派生**：绝对路径 [`PackBaseline::path`]，相对路径
//! [`PackBaseline::relative_path`]，两者由 `addon/tests/import_merge.rs` 的一条断言钉死
//! （`path(root) == root.join(relative_path())`）——这条在大小写敏感的系统上也抓得住。
//!
//! # 包身份
//!
//! 基线记包名与格式；下次导入发现"基线属于另一个包"时**不拿它当 base**（否则跨包导入同一
//! 目录会把别人的哈希当成本次包的历史），而是降级成"无基线"并如实告警。
//!
//! 基线里存的是**包声明的哈希**（不是磁盘实际哈希）：用户改过的文件在基线里仍是包的值，
//! 下次导入才会继续判成"用户改过"，而不会被"上次也是改动值"洗白。`.disabled` / `.old`
//! 后缀按原样进键名（后缀是路径的一部分，不附加语义）。
//!
//! 读失败**不是**致命错误：基线坏了最多退化成"全部按 pre-existing 冲突处理"（保留用户文件），
//! 比"因为读不出基线就整包回滚"合理。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::game::layout::GameLayout;

/// 基线文件名。
pub const BASELINE_FILE: &str = "pack-baseline.json";
/// 基线 schema 版本。
pub const BASELINE_SCHEMA: u32 = 1;

/// 上次导入写下的包声明哈希表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackBaseline {
    /// schema 版本（迁移链入口）。
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    /// 上次导入的包名（诊断 + 包身份判据）。
    #[serde(default)]
    pub pack_name: String,
    /// 上次导入命中的格式 id（包身份判据的另一半）。
    #[serde(default)]
    pub format: String,
    /// 实例内相对路径 → 包声明的 SHA-1。
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

const fn default_schema() -> u32 {
    BASELINE_SCHEMA
}

impl Default for PackBaseline {
    fn default() -> Self {
        Self {
            schema_version: BASELINE_SCHEMA,
            pack_name: String::new(),
            format: String::new(),
            files: BTreeMap::new(),
        }
    }
}

impl PackBaseline {
    /// 实例根下的基线**绝对**路径。
    ///
    /// 唯一来源是 `GameLayout::ull_root()`：实例的游戏根里那个启动器私有目录。
    #[must_use]
    pub fn path(game_root: &Path) -> PathBuf {
        GameLayout::new(game_root).ull_root().join(BASELINE_FILE)
    }

    /// 基线**相对实例根**的路径（暂存期与写盘用；与 [`Self::path`] 同源）。
    #[must_use]
    pub fn relative_path() -> PathBuf {
        // 用空根求 `ull_root()` 就得到那一段相对目录名（`.ull`），不另抄一份字符串。
        GameLayout::new(Path::new(""))
            .ull_root()
            .join(BASELINE_FILE)
    }

    /// 基线是否属于这份包（空基线 / 没有身份信息的老基线一律放行）。
    #[must_use]
    pub fn matches_pack(&self, format: &str, pack_name: &str) -> bool {
        if self.pack_name.is_empty() && self.format.is_empty() {
            return true;
        }
        self.format == format && self.pack_name == pack_name
    }

    /// 读基线；文件不存在返回空表（**不是错误**），形状不符返回 `Err`。
    ///
    /// # Errors
    ///
    /// 非 UTF-8 / JSON 非法 / 版本号比本进程新（未来版本写的）时返回人读原因。
    pub fn load(game_root: &Path) -> Result<Self, String> {
        let path = Self::path(game_root);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => {
                return Err(format!("读 {} 失败：{error}", path.display()));
            }
        };
        let text = String::from_utf8(bytes)
            .map_err(|error| format!("{} 不是 UTF-8：{error}", path.display()))?;
        let baseline: Self = serde_json::from_str(&text)
            .map_err(|error| format!("{} 形状不符：{error}", path.display()))?;
        if baseline.schema_version > BASELINE_SCHEMA {
            return Err(format!(
                "{} 的 schemaVersion={} 比本进程（{BASELINE_SCHEMA}）新",
                path.display(),
                baseline.schema_version
            ));
        }
        Ok(baseline)
    }

    /// 查上次给这个路径的哈希。
    #[must_use]
    pub fn sha1(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    /// 记一条。
    pub fn insert(&mut self, path: impl Into<String>, sha1: impl Into<String>) {
        self.files.insert(path.into(), sha1.into());
    }

    /// 序列化成落盘字节（pretty：这文件是给人排障看的）。
    ///
    /// # Errors
    ///
    /// 序列化失败（纯理论，`BTreeMap<String, String>` 不会失败）。
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec_pretty(self).map_err(|error| format!("基线序列化失败：{error}"))
    }
}
