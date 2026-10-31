//! `hs_err_pid*.log` 解析结果的数据结构（标题行 / 线程段 / 内存段 / 失败帧）。
//!
//! 由 `hs_err.rs` 用 `#[path]` 声明并重新导出；拆分只为守单文件 400 行上限。

use std::fmt;
use std::path::PathBuf;

/// hs_err 的崩溃类别（标题行/异常名归并成少数几类，供内置结论判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HsErrKind {
    /// `EXCEPTION_ACCESS_VIOLATION` / `SIGSEGV`：野指针，显卡驱动/原生库最常见。
    AccessViolation,
    /// 内存不足（`insufficient memory` / `Out of Memory Error`）。
    OutOfMemory,
    /// 栈溢出。
    StackOverflow,
    /// JVM 内部致命错误（`Internal Error` / `A fatal error has been detected`）。
    InternalError,
    /// 其它 / 无法归类。
    #[default]
    Other,
}

impl HsErrKind {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccessViolation => "access_violation",
            Self::OutOfMemory => "out_of_memory",
            Self::StackOverflow => "stack_overflow",
            Self::InternalError => "internal_error",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for HsErrKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 失败帧的类型（`# C` 原生码、`# J`/`# j` Java 帧、`# V` VM 生成桩帧）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameKind {
    /// 原生（C/C++）帧：出现在显卡驱动/OpenGL 里基本可判定驱动问题。
    Native,
    /// Java 帧（`# J` / `# j`，二者区别是解释执行还是编译执行，诊断不需要再细分）。
    Java,
    /// VM 生成桩帧（`# V`）：JIT/解释器生成的运行时代码，不是用户代码。
    VmStub,
    /// 未识别。
    #[default]
    Unknown,
}

impl FrameKind {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Java => "java",
            Self::VmStub => "vm_stub",
            Self::Unknown => "unknown",
        }
    }
}

/// `# Problematic frame:` 之后的那一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedFrame {
    /// 原始文本（`C  [ig8d64.dll+0x9b3c4d]`）。
    pub raw: String,
    /// 帧类型。
    pub kind: FrameKind,
    /// 命中的模块/DLL（`ig8d64.dll`）。
    pub module: Option<String>,
    /// 模块内偏移（`0x9b3c4d`）。
    pub offset: Option<String>,
}

/// 内存段摘要（`Memory: 4k page, physical …, swap …`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemorySummary {
    /// 页大小（`4k`）。
    pub page_size: Option<String>,
    /// 物理内存 kB。
    pub physical_kb: Option<u64>,
    /// 物理可用 kB。
    pub physical_free_kb: Option<u64>,
    /// 交换区 kB。
    pub swap_kb: Option<u64>,
    /// 交换区可用 kB。
    pub swap_free_kb: Option<u64>,
    /// 原始行。
    pub raw: String,
}

impl MemorySummary {
    /// 一行人类可读摘要（GB 保留一位小数）。
    #[must_use]
    pub fn summary(&self) -> String {
        let gb = |kb: Option<u64>| {
            kb.map(|kb| format!("{:.1} GB", kb as f64 / 1024.0 / 1024.0))
                .unwrap_or_else(|| "?".to_owned())
        };
        format!(
            "物理内存 {}（可用 {}），交换区 {}（可用 {}）",
            gb(self.physical_kb),
            gb(self.physical_free_kb),
            gb(self.swap_kb),
            gb(self.swap_free_kb)
        )
    }
}

/// 解析结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HsErrReport {
    /// 来源文件（`parse` 时为 `None`）。
    pub path: Option<PathBuf>,
    /// 标题行（`#[derive(Default)]` 之外的语义见 `parse`）。
    pub header: Option<String>,
    /// 崩溃类别。
    pub kind: HsErrKind,
    /// 异常名（`EXCEPTION_ACCESS_VIOLATION (0xc0000005)`）。
    pub exception: Option<String>,
    /// `siginfo:` 行（Unix 侧带 si_signo / si_addr）。
    pub siginfo: Option<String>,
    /// 信号名（`SIGSEGV (0xb)`）。
    pub signal: Option<String>,
    /// 出错时的指令指针。
    pub pc: Option<String>,
    /// 进程 pid。
    pub pid: Option<u32>,
    /// 线程 tid（保留原始十六进制/十进制串）。
    pub tid: Option<String>,
    /// 失败帧。
    pub failed_frame: Option<FailedFrame>,
    /// 当前线程（`JavaThread "Render thread" [_thread_in_native, id=…]`）。
    pub current_thread: Option<String>,
    /// JRE 版本行内容。
    pub jre_version: Option<String>,
    /// JVM 实现行内容。
    pub java_vm: Option<String>,
    /// 操作系统行内容。
    pub os: Option<String>,
    /// `java_command` 行。
    pub command_line: Option<String>,
    /// `VM Arguments:` 段的参数行。
    pub vm_arguments: Vec<String>,
    /// 内存段摘要。
    pub memory: Option<MemorySummary>,
    /// `Heap:` 段的前几行（原样，含 total/used）。
    pub heap: Vec<String>,
    /// 标题块原文（逐行保留 `#` 前缀，规则可以直接在上面匹配）。
    pub header_lines: Vec<String>,
}

impl HsErrReport {
    /// 供规则匹配 / 证据展示的摘要文本。
    ///
    /// 保留标题块原文（`EXCEPTION_ACCESS_VIOLATION` 与 `# C  [ig…dll]` 的相对位置不变，
    /// 因此 `graphics.graphics_driver` 这类跨行规则可以直接命中），再补上线程/内存段要点。
    #[must_use]
    pub fn digest(&self) -> String {
        let mut out = String::new();
        for line in &self.header_lines {
            out.push_str(line);
            out.push('\n');
        }
        for line in [
            self.siginfo.as_deref(),
            self.current_thread.as_deref(),
            self.jre_version.as_deref(),
            self.java_vm.as_deref(),
            self.os.as_deref(),
            self.command_line.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            out.push_str(line);
            out.push('\n');
        }
        for line in &self.heap {
            out.push_str(line);
            out.push('\n');
        }
        if let Some(memory) = &self.memory {
            out.push_str(&memory.raw);
            out.push('\n');
        }
        out
    }

    /// 一句话摘要（诊断输出 / UI 头部）。
    #[must_use]
    pub fn summary(&self) -> String {
        let kind = self.kind.as_str();
        let frame = self
            .failed_frame
            .as_ref()
            .map_or_else(String::new, |frame| format!("，失败帧 {}", frame.raw));
        let header = self.header.as_deref().unwrap_or("JVM 致命错误");
        format!("{header}（{kind}{frame}）")
    }
}
