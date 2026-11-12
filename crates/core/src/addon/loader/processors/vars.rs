//! 占位替换：install profile 的 `data` 表 + 标准变量 → 一张 `{键 → 值}` 表。
//!
//! 这是 Forge processor 协议里最容易出错的一层，三种字面量语义（照 installer spec，
//! 与 HMCL `ForgeNewInstallTask.parseLiteral` 同构）：
//!
//! | 写法 | 含义 | 例 |
//! |------|------|----|
//! | `[group:artifact:version(:classifier)(@ext)]` | maven 构件 → `libraries/` 下的绝对路径 | `[de.oceanlabs.mcp:mcp_config:1.20.1@zip]` |
//! | `'literal'` | 去掉引号的常量 | `'client'` |
//! | 其它 | **installer 里的文件路径** → 解包到临时目录后的绝对路径 | `/data/client.lzma` |
//!
//! 展开（`{KEY}`）的规则与 HMCL 的 `replaceTokens` 一致：`\` 转义下一个字符、
//! `'…'` 原样输出、`{KEY}` 查表替换（**查不到就报错**，不静默留空——留空会让
//! processor 拿着半截参数跑出更难查的错）。
//!
//! # 为什么 `MINECRAFT_JAR` 指向临时目录的副本
//!
//! HMCL 在起 processor 前把原版 client jar **复制到临时目录**再用，注释写得很直白：
//! `External processors must not receive the shared cache path`。processor 会改写输入的
//! jar（拆包/打补丁），直接把 `versions/<mc>/<mc>.jar` 递过去等于让外挂程序改共享层。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::error::ProcessorError;
use super::spec::Datum;
use crate::game::layout::GameLayout;
use crate::game::library::MavenCoordinate;

/// 临时目录里那份原版 client jar 的文件名。
pub const MINECRAFT_JAR_NAME: &str = "minecraft.jar";

/// 一个需要从 installer zip 里解出来的文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFile {
    /// installer 内的条目路径（`data/client.lzma`）。
    pub entry: String,
    /// 解包落点（临时目录下的扁平文件名）。
    pub dest: PathBuf,
}

/// 变量表的构造输入。
pub struct VariablesInput<'a> {
    /// install profile 的 `data` 表。
    pub data: &'a BTreeMap<String, Datum>,
    /// 游戏目录布局（`libraries/` 与 `versions/` 的路径规则）。
    pub layout: &'a GameLayout,
    /// 目标原版版本（`net.minecraft:client:<ver>` 这类坐标回落用）。
    pub mc_version: &'a str,
    /// installer jar 的磁盘路径（`{INSTALLER}`，也是解包来源）。
    pub installer: &'a Path,
    /// 临时目录（`{MINECRAFT_JAR}` 与解包文件都落这里）。
    pub scratch: &'a Path,
}

/// 展开后的变量表。
#[derive(Debug, Clone, Default)]
pub struct Variables {
    values: BTreeMap<String, String>,
    embedded: Vec<EmbeddedFile>,
    /// 参数里 `[坐标]` 的解析上下文（官方 profile 会在 `args` 里直接写坐标，不只写在 `data` 里）。
    artifacts: Option<ArtifactRoots>,
}

/// `[坐标]` 的解析上下文。
#[derive(Debug, Clone)]
struct ArtifactRoots {
    libraries: PathBuf,
    vanilla_jar: PathBuf,
}

impl Variables {
    /// 组装变量表：先展开 `data` 表，再注入标准变量。
    ///
    /// # Errors
    ///
    /// 构件坐标不合法、`data` 表里的值是空路径、变量引用缺定义时返回
    /// [`ProcessorErrorKind::Plan`](super::error::ProcessorErrorKind::Plan) 错误。
    pub fn build(input: VariablesInput<'_>) -> Result<Self, ProcessorError> {
        let mut variables = Self {
            artifacts: Some(ArtifactRoots {
                libraries: input.layout.libraries_dir(),
                vanilla_jar: input.layout.version_jar(input.mc_version),
            }),
            ..Self::default()
        };
        for (key, datum) in input.data {
            let Some(raw) = datum.client_value() else {
                continue;
            };
            let value = variables.resolve_literal(raw, &input)?;
            variables.values.insert(key.clone(), value);
        }
        variables.inject_standard(&input);
        Ok(variables)
    }

    /// 标准变量（installer spec 没有写死，但各家启动器都注入同一套）。
    fn inject_standard(&mut self, input: &VariablesInput<'_>) {
        let scratch = absolute(input.scratch);
        let standard = [
            ("SIDE", "client".to_string()),
            ("MINECRAFT_VERSION", input.mc_version.to_string()),
            (
                "MINECRAFT_JAR",
                absolute(&scratch.join(MINECRAFT_JAR_NAME))
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "ROOT",
                absolute(input.layout.root()).to_string_lossy().into_owned(),
            ),
            (
                "INSTALLER",
                absolute(input.installer).to_string_lossy().into_owned(),
            ),
            (
                "LIBRARY_DIR",
                absolute(&input.layout.libraries_dir())
                    .to_string_lossy()
                    .into_owned(),
            ),
        ];
        for (key, value) in standard {
            self.values.insert(key.to_string(), value);
        }
    }

    /// 把一个 `data` 表的值展开成实际字符串。
    fn resolve_literal(
        &mut self,
        raw: &str,
        input: &VariablesInput<'_>,
    ) -> Result<String, ProcessorError> {
        let trimmed = raw.trim();
        if let Some(inner) = trim_brackets(trimmed) {
            return self.resolve_artifact(inner);
        }
        if let Some(inner) = trimmed
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
        {
            return Ok(inner.to_string());
        }
        if trimmed.starts_with('{') {
            // data 表自己引用别的键：官方 profile 不这么写，但展开一次成本为零。
            return self.expand(trimmed);
        }
        let entry = trimmed.trim_start_matches('/').to_string();
        if entry.is_empty() {
            return Err(ProcessorError::plan(format!(
                "install profile 的 data 表里有空路径：`{raw}`"
            )));
        }
        // 其余一律当 installer 内的文件：解到临时目录（执行期才真正解包）。
        let dest = absolute(&absolute(input.scratch).join(embedded_name(&entry)));
        self.embedded.push(EmbeddedFile {
            entry,
            dest: dest.clone(),
        });
        Ok(dest.to_string_lossy().into_owned())
    }

    /// `[坐标]` → `libraries/` 下的绝对路径。
    fn resolve_artifact(&self, descriptor: &str) -> Result<String, ProcessorError> {
        let Some(roots) = &self.artifacts else {
            return Err(ProcessorError::plan(format!(
                "构件坐标 `{descriptor}` 需要游戏目录上下文才能解析"
            )));
        };
        artifact_path(&roots.libraries, &roots.vanilla_jar, descriptor)
    }

    /// 展开一段含占位的文本（`{KEY}` / `'字面量'` / `[坐标]` / `\` 转义）。
    ///
    /// `[坐标]` 只在**整段就是坐标**时生效（与 HMCL `parseLiteral` 一致）：
    /// official profile 的 `args` 里常直接写 `--input [de.oceanlabs.mcp:…@zip]`。
    ///
    /// # Errors
    ///
    /// `{KEY}` 在表里查不到、`\` 结尾无字符可转义、`{`/`'` 未闭合、坐标不合法时返回规划错误。
    pub fn expand(&self, pattern: &str) -> Result<String, ProcessorError> {
        let trimmed = pattern.trim();
        if let Some(inner) = trim_brackets(trimmed) {
            return self.resolve_artifact(inner);
        }
        let mut out = String::with_capacity(pattern.len());
        let mut chars = pattern.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some(next) => out.push(next),
                    None => {
                        return Err(ProcessorError::plan(format!(
                            "占位文本以 `\\` 结尾（无法转义）：`{pattern}`"
                        )))
                    }
                },
                '{' => {
                    let key = collect_until(&mut chars, '}', pattern)?;
                    let value = self.values.get(&key).ok_or_else(|| {
                        ProcessorError::plan(format!(
                            "占位 {{{key}}} 在 install profile 的 data 表里没有定义（原文 `{pattern}`）"
                        ))
                    })?;
                    out.push_str(value);
                }
                '\'' => out.push_str(&collect_until(&mut chars, '\'', pattern)?),
                other => out.push(other),
            }
        }
        Ok(out)
    }

    /// 某个变量的值。
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// 需要从 installer 里解包的文件（执行期用）。
    #[must_use]
    pub fn embedded(&self) -> &[EmbeddedFile] {
        &self.embedded
    }

    /// 变量条数（`--dry-run` 只报数量，不吐全部路径）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// 是否一条变量都没有（实际不可能：标准变量恒在）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// 读到 `closing` 为止（跳过 `\` 转义），返回中间文本。
fn collect_until(
    chars: &mut impl Iterator<Item = char>,
    closing: char,
    pattern: &str,
) -> Result<String, ProcessorError> {
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(next) => out.push(next),
                None => {
                    return Err(ProcessorError::plan(format!(
                        "占位文本以 `\\` 结尾（无法转义）：`{pattern}`"
                    )))
                }
            }
        } else if ch == closing {
            return Ok(out);
        } else {
            out.push(ch);
        }
    }
    Err(ProcessorError::plan(format!(
        "占位文本缺少收尾的 `{closing}`：`{pattern}`"
    )))
}

/// 整段是 `[坐标]` 时取出坐标（不到首尾就返回 `None`）。
fn trim_brackets(raw: &str) -> Option<&str> {
    raw.strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
}

/// `[坐标]` → `libraries/` 下的绝对路径。
///
/// 少数坐标进不了 `libraries/`：`net.minecraft:client:<ver>`（无 classifier）对应的是
/// `versions/<ver>/<ver>.jar`。官方 installer 会把 client jar 塞进本地 maven 布局，
/// 我们不复制（磁盘上原版 jar 只有一份），因此按目标版本回落。
fn artifact_path(
    libraries: &Path,
    vanilla_jar: &Path,
    descriptor: &str,
) -> Result<String, ProcessorError> {
    let coordinate = MavenCoordinate::parse(descriptor).map_err(|error| {
        ProcessorError::plan(format!(
            "install profile 里的构件坐标不合法：`{descriptor}`（{error}）"
        ))
    })?;
    let path = libraries.join(coordinate.relative_path());
    if coordinate.group == "net.minecraft"
        && coordinate.artifact == "client"
        && coordinate.classifier.is_none()
        && vanilla_jar.is_file()
    {
        return Ok(absolute(vanilla_jar).to_string_lossy().into_owned());
    }
    Ok(absolute(&path).to_string_lossy().into_owned())
}

/// installer 内的条目 → 临时目录里的扁平文件名（`data/client.lzma` → `data-client.lzma`）。
///
/// 压平是为了防路径穿越：`../` 与绝对路径在拼进临时目录时都会失去意义。
fn embedded_name(entry: &str) -> String {
    entry
        .chars()
        .map(|ch| {
            if matches!(ch, '/' | '\\' | ':') {
                '-'
            } else {
                ch
            }
        })
        .collect()
}

/// 尽量转绝对路径（相对路径按当前工作目录；失败就原样返回）。
#[must_use]
pub fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
