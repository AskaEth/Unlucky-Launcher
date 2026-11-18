//! 诊断包（M187）：把一次启动失败的现场打成一个 zip 交给用户。
//!
//! ## 为什么不只是「打开日志文件夹」
//!
//! 用户实测（2026-11-08）：「MC 启动失败『游戏崩溃 退出码 1』，找半天没找到崩溃日志」。
//! 现场散在四处——启动器日志（`.ULL/logs/`）、会话事件流 JSONL（`data/agent/sessions/game/`）、
//! 游戏自己写的 `crash-reports/` 与 `logs/latest.log`、还有环境（系统 / Java / 命令行）。
//! 让用户自己去四个目录里拼，等于没修。打成一个包 + 一行清单，才是「能发给别人看」的东西。
//!
//! ## 三条纪律
//!
//! 1. **落盘即脱敏**（`launch::redact` 的唯一一份规则）：主目录 → `<home>`，token → 占位符，
//!    长不透明串（base64/hex）与带点 token（JWT 形状）同样替换；调用方另外把**会话 id**
//!    之类也要抹掉的串塞进 [`BundleSpec::secrets`]。
//! 2. **缺什么要写出来**：`crash-reports/` 不存在不是错误——那是「游戏没来得及写」的**证据**，
//!    清单里必须有一行说清楚，不许静默漏掉。见 [`BundleSpec::missing`]。
//! 3. **有上限**：单个文件最多收 [`MAX_ENTRY_BYTES`]，包内总字节最多 [`MAX_TOTAL_BYTES`]。
//!    超了就在清单里记一行「已截断」。数百 MB 的 `latest.log` 不该把导出卡成十秒。
//!
//! 不做的事：不判断崩溃原因（那是诊断引擎 `diagnose` 的活），不改任何图片之外的二进制，
//! 不联网。打包只读盘。

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::redact::Redactor;

/// 单个文件最多收多少字节（超出按**头 + 尾**截断，清单里记「已截断」）。
///
/// 为什么不是「只收头部」：崩溃证据在**尾部**——会话 JSONL 的最后几行是 `exit` 事件与
/// stderr，`latest.log` 的栈也在尾。只留头部的包等于「证据是残的，而用户不知道」。
pub const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024;
/// 截断时头部保留的字节（另一半点给尾部）。
pub const HEAD_BYTES: u64 = MAX_ENTRY_BYTES / 2;
/// 整个包最多收多少字节（超出后剩余文件只记清单、不收内容）。
pub const MAX_TOTAL_BYTES: u64 = 48 * 1024 * 1024;

/// 一个要进包的文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEntry {
    /// 包内分组（`launcher` / `session` / `game` / `instance`），决定包内目录。
    pub group: &'static str,
    /// 源文件绝对路径。
    pub path: PathBuf,
    /// 包内固定文件名（`None` = 用源文件名）。
    ///
    /// 为什么要能钉死：会话 JSONL 的源文件名就是**会话 id**（`<uuid>.jsonl`），
    /// 拿它当条目名要么泄漏 id、要么让「清单里的名字」与「包里的名字」对不上。钉成
    /// `session.jsonl` 两个问题一起消失（见 [`BundleSpec::entry_names`]）。
    pub name: Option<String>,
}

impl BundleEntry {
    /// 建一条（包内名 = 源文件名）。
    pub fn new(group: &'static str, path: impl Into<PathBuf>) -> Self {
        Self {
            group,
            path: path.into(),
            name: None,
        }
    }

    /// 建一条并**钉死包内文件名**（会话 JSONL、`latest.log` 这类有唯一身份的用它）。
    pub fn named(group: &'static str, path: impl Into<PathBuf>, name: &str) -> Self {
        Self {
            group,
            path: path.into(),
            name: Some(name.to_string()),
        }
    }
}

/// 一次导出的输入（全部是事实，不含任何结论）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BundleSpec {
    /// 实例名。
    pub instance: String,
    /// 版本 id。
    pub version: String,
    /// 会话 id（**同时会被脱敏**）。
    pub session: String,
    /// 退出码（`None` = 进程没起来）。
    pub exit_code: Option<i32>,
    /// 退出判定 slug（`application_error` / `jvm_error` / `normal` …）。
    pub exit_type: String,
    /// 存活毫秒数。
    pub uptime_ms: u64,
    /// 脱敏后的命令行（**调用方**已经过 [`Redactor::scrub_all`]）。
    pub command: String,
    /// Java 一行说明（路径 + 主版本）。
    pub java: String,
    /// 系统一行说明（`OsInfo::summary_line`）。
    pub system: String,
    /// 额外说明（例如「本次没有生成 crash-report」）。
    pub notes: Vec<String>,
    /// 明确不存在的现场（清单里写成「没有：…」）。
    pub missing: Vec<String>,
    /// 要收的文件。
    pub entries: Vec<BundleEntry>,
    /// 除了主目录之外还要抹掉的串（会话 id 等）。
    pub secrets: Vec<String>,
    /// 用户主目录（`None` = 不抹）。
    pub home: Option<PathBuf>,
}

impl BundleSpec {
    /// 清单文本（`manifest.txt`；导出前会再过一遍脱敏）。
    ///
    /// 这是「自算条目名」的包装（测试与单独看清单用）；真正落盘时用的是
    /// [`Self::header`] + 收集循环里长出来的行，保证与包内条目一字不差。
    #[must_use]
    pub fn manifest(&self) -> String {
        let rows: Vec<String> = self
            .entry_names()
            .iter()
            .zip(&self.entries)
            .map(|(name, entry)| format!("- {name}  <- {}", entry.path.display()))
            .collect();
        // 对外出口一律脱敏（r2 复审 P3-6）：不给「谁调谁泄漏」的口子。
        self.redactor().scrub(&self.header(&rows)).into_owned()
    }

    /// 清单的**事实段 + files 段**（files 段由调用方给，行内已是最终条目名）。
    fn header(&self, rows: &[String]) -> String {
        let mut text = String::new();
        text.push_str("Unlucky Launcher — launch failure bundle\n");
        text.push_str("=======================================\n");
        text.push_str(&format!("instance: {}\n", self.instance));
        text.push_str(&format!("version: {}\n", self.version));
        text.push_str(&format!("session: {}\n", self.session));
        text.push_str(&format!(
            "exit_code: {}\n",
            self.exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "(none)".to_string())
        ));
        text.push_str(&format!("exit_type: {}\n", self.exit_type));
        text.push_str(&format!("uptime: {:.1}s\n", self.uptime_ms as f64 / 1000.0));
        text.push_str(&format!("os: {}\n", self.system));
        text.push_str(&format!("java: {}\n", self.java));
        text.push_str(&format!("command: {}\n", self.command));

        if !self.notes.is_empty() {
            text.push_str("\nnotes\n-----\n");
            for note in &self.notes {
                text.push_str(&format!("- {note}\n"));
            }
        }
        if !self.missing.is_empty() {
            text.push_str("\nmissing (不存在)\n----------------\n");
            for item in &self.missing {
                text.push_str(&format!("- {item}\n"));
            }
        }

        text.push_str("\nfiles\n-----\n");
        // 清单里的名字**必须**与包里的条目一一对应（r1 复审 P2-2 就是这里对不上）：
        // 两边都走同一份解析（`rows` 由调用方给出，行内已是最终条目名）：
        // 清单与 zip 不许各算一遍 —— 各算一遍就是 r1 复审 P2-2。
        for row in rows {
            text.push_str(row);
            text.push('\n');
        }
        text
    }

    /// 包内条目名（与 `entries` 一一对应；**名字也过脱敏**）。
    ///
    /// 名字与内容分开治：内容走 `redactor` 的整套规则，名字走同一份 redactor 的 token 规则。
    /// **两边都以返回值为准**——清单与 zip 都读这一份，不许各算一遍（各算一遍就是 P2-2）。
    #[must_use]
    pub fn entry_names(&self) -> Vec<String> {
        let redactor = self.redactor();
        self.entries
            .iter()
            .map(|entry| {
                let raw = entry.name.clone().unwrap_or_else(|| file_name(&entry.path));
                let safe = redactor.scrub(&raw).into_owned();
                format!("{}/{}", entry.group, safe)
            })
            .collect()
    }

    fn redactor(&self) -> Redactor {
        let mut redactor = Redactor::new();
        if let Some(home) = &self.home {
            redactor = redactor.with_home(home);
        }
        for secret in &self.secrets {
            redactor = redactor.with_secret(secret.clone());
        }
        redactor
    }
}

/// 一次导出的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// 落盘路径。
    pub path: PathBuf,
    /// 字节数（**落盘后**的真实大小）。
    pub bytes: u64,
    /// 收进包的文件数（不含清单）。
    pub files: usize,
    /// 被截断 / 因总量上限没收内容的文件数。
    pub truncated: usize,
}

impl Bundle {
    /// 一行说明（Toast 与日志共用）。
    pub fn describe(&self) -> String {
        format!(
            "{} ({} bytes, {} files, {} truncated)",
            self.path.display(),
            self.bytes,
            self.files,
            self.truncated
        )
    }
}

/// 把 `spec` 打成一个 zip 写到 `target`（父目录不存在会建）。
///
/// # Errors
/// 目录建不出来、zip 写失败、清单写失败时返回人话（导出失败要能一眼看出哪一步断）。
pub fn write_bundle(spec: &BundleSpec, target: &Path) -> Result<Bundle, String> {
    let redactor = spec.redactor();

    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("建目录 {} 失败：{error}", parent.display()))?;
        }
    }
    let file = File::create(target)
        .map_err(|error| format!("建诊断包 {} 失败：{error}", target.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    // 名字与清单**先在同一个循环里长出来**：`files` 段只列真进了包的条目，
    // 谁被跳过、谁被截断都在 `collected` 段写明（r1 复审 P2-2 的修法）。
    let names = spec.entry_names();
    let mut listed: Vec<String> = Vec::new();
    let mut collected: Vec<String> = Vec::new();
    let mut total: u64 = 0;
    let mut files = 0usize;
    let mut truncated = 0usize;
    let mut used: Vec<String> = Vec::new();

    for (index, entry) in spec.entries.iter().enumerate() {
        let name = unique_name(&mut used, &names[index]);
        let Ok(bytes) = std::fs::read(&entry.path) else {
            collected.push(format!("- {name}: 读不出来（已跳过）"));
            continue;
        };
        let original_len = bytes.len();
        // **唯一的通路**：解码 → 脱敏 → 按字符边界截断（见 `clip.rs` 的模块文档）。
        // 没有「二进制原样」分支，所以也不存在「截坏一个字就整份跳过脱敏」的旁路
        // （r2 复审 P2-1）。`lossy` 保证任何字节序列都能进脱敏器。
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let scrubbed = redactor.scrub(&text).into_owned();
        let budget = MAX_TOTAL_BYTES.saturating_sub(total).min(MAX_ENTRY_BYTES);
        let mut kept = scrubbed;
        if kept.len() as u64 > budget {
            let Some(clipped) = clip_text(&kept, budget) else {
                collected.push(format!("- {name}: 超出总量上限，未收内容"));
                truncated += 1;
                continue;
            };
            kept = clipped;
            collected.push(format!(
                "- {name}: 已截断（收 {} B，原 {original_len} B）",
                kept.len()
            ));
            truncated += 1;
        }
        let body = kept.into_bytes();
        total += body.len() as u64;
        files += 1;
        listed.push(format!("- {name}  <- {}", entry.path.display()));
        write_entry(&mut zip, &name, &body, options)?;
    }

    let mut manifest = spec.header(&listed);
    if !collected.is_empty() {
        manifest.push_str("\ncollected\n---------\n");
        for line in collected {
            manifest.push_str(&line);
            manifest.push('\n');
        }
    }
    write_entry(
        &mut zip,
        "manifest.txt",
        redactor.scrub(&manifest).as_bytes(),
        options,
    )?;
    zip.finish()
        .map_err(|error| format!("收尾诊断包失败：{error}"))?;

    let bytes = std::fs::metadata(target)
        .map(|meta| meta.len())
        .unwrap_or(0);
    Ok(Bundle {
        path: target.to_path_buf(),
        bytes,
        files,
        truncated,
    })
}

fn write_entry(
    zip: &mut zip::ZipWriter<File>,
    name: &str,
    body: &[u8],
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    zip.start_file(name, options)
        .map_err(|error| format!("写入 {name} 失败：{error}"))?;
    zip.write_all(body)
        .map_err(|error| format!("写入 {name} 失败：{error}"))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string())
}

/// 包内重名时在**目录名后**加序号（`game/latest.log` 与 `game/2-latest.log` 不会互相覆盖）。
fn unique_name(used: &mut Vec<String>, name: &str) -> String {
    let base = name.to_string();
    if !used.contains(&base) {
        used.push(base.clone());
        return base;
    }
    let (group, file) = base.split_once('/').unwrap_or(("", base.as_str()));
    for index in 2.. {
        let candidate = format!("{group}/{index}-{file}");
        if !used.contains(&candidate) {
            used.push(candidate.clone());
            return candidate;
        }
    }
    unreachable!("序号一定找得到空位")
}

/// 默认落点：启动器日志分区下的 `exports/`。
///
/// 为什么不弹文件选择框：本仓没有 `rfd` 这类原生对话框依赖（Cargo.toml 不在本 mission scope），
/// 也不该为一次导出引一棵新依赖树。导出后界面把**完整路径**与字节数都给出来，另有
/// 「打开文件夹」按钮直达——用户不用猜。
pub fn default_target_dir(logs_dir: &Path) -> PathBuf {
    logs_dir.join("exports")
}

mod clip;

use clip::clip_text;

#[cfg(test)]
mod tests;
