//! 临时目录准备：建目录、解包 installer 内的文件、复制原版 client jar。
//!
//! 三条都是「起 java 之前的脏活」，失败一律归 `processor_io`/`processor_plan`
//! （区别于 processor 自己跑挂的 `processor_failed`）。

use super::error::ProcessorError;
use super::plan::ProcessorPlan;

/// 单条目的**预分配**上限（64 MiB）：installer jar 下载后只校验「是合法 zip」，
/// 没有校验和（`meta.rs` 的 `fetch_installer`），因此 zip header 里声明的解压尺寸
/// 是不可信输入——损坏或镜像投毒的 installer 可以声明天文数字，让一次小文件解包直接
/// 申请多 GB 内存。`Vec` 仍按实际读入增长，合法条目行为不变。
const PREALLOC_MAX: u64 = 64 << 20;

/// 单条目的**实际字节**上限（256 MiB）：预分配封顶挡不住「声明小、解压极大」的
/// zip bomb，所以读入过程本身也要有硬顶。超过就明确报错（不静默截断）。
const MAX_EMBEDDED_BYTES: u64 = 256 << 20;

/// 执行前的准备：建临时目录、解包 installer 内的文件、复制原版 client jar。
pub(super) fn prepare(plan: &ProcessorPlan) -> Result<(), ProcessorError> {
    std::fs::create_dir_all(&plan.scratch).map_err(|error| {
        ProcessorError::io(format!(
            "建临时目录失败（{}）：{error}",
            plan.scratch.display()
        ))
    })?;
    extract_embedded(plan)?;
    copy_minecraft_jar(plan)
}

/// 把 install profile 声明要解包的文件从 installer zip 里抽到临时目录。
fn extract_embedded(plan: &ProcessorPlan) -> Result<(), ProcessorError> {
    let embedded = plan.variables.embedded();
    if embedded.is_empty() {
        return Ok(());
    }
    use std::io::Read;

    let installer = &plan.installer;
    let file = std::fs::File::open(installer).map_err(|error| {
        ProcessorError::io(format!(
            "打开 installer 失败（{}）：{error}",
            installer.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        ProcessorError::plan(format!("{} 不是合法 zip：{error}", installer.display()))
    })?;
    for item in embedded {
        let mut entry = archive.by_name(&item.entry).map_err(|error| {
            ProcessorError::plan(format!(
                "installer 里没有 {}（data 表声明要解包）：{error}",
                item.entry
            ))
        })?;
        let mut bytes = Vec::with_capacity(prealloc_hint(entry.size()));
        let read = (&mut entry)
            .take(MAX_EMBEDDED_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                ProcessorError::io(format!("读 installer 条目 {} 失败：{error}", item.entry))
            })?;
        if read as u64 > MAX_EMBEDDED_BYTES {
            return Err(ProcessorError::plan(format!(
                "installer 条目 {} 解压后超过 {} MiB，拒绝继续（installer 可能损坏或被投毒）",
                item.entry,
                MAX_EMBEDDED_BYTES >> 20
            )));
        }
        crate::platform::atomic::write_atomic(&item.dest, &bytes).map_err(|error| {
            ProcessorError::io(format!(
                "写临时文件失败（{}）：{error}",
                item.dest.display()
            ))
        })?;
    }
    Ok(())
}

/// 预分配提示：**不信任** header 声明的尺寸，封顶 [`PREALLOC_MAX`]。
fn prealloc_hint(declared: u64) -> usize {
    declared.min(PREALLOC_MAX) as usize
}

/// 复制原版 client jar 到临时目录（processor 会改写输入，不能让它碰共享层）。
fn copy_minecraft_jar(plan: &ProcessorPlan) -> Result<(), ProcessorError> {
    let source = &plan.minecraft_jar;
    let target = &plan.minecraft_jar_copy;
    let fresh = match (std::fs::metadata(source), std::fs::metadata(target)) {
        (Ok(left), Ok(right)) => left.len() != right.len(),
        (Ok(_), Err(_)) => true,
        (Err(error), _) => {
            return Err(ProcessorError::io(format!(
                "原版 client jar 不在盘上（{}）：{error}",
                source.display()
            ))
            .with_hint("先装原版：`ull install <version>`"))
        }
    };
    if !fresh {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            ProcessorError::io(format!("建目录失败（{}）：{error}", parent.display()))
        })?;
    }
    std::fs::copy(source, target).map_err(|error| {
        ProcessorError::io(format!(
            "复制 {} → {} 失败：{error}",
            source.display(),
            target.display()
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{prealloc_hint, prepare, PREALLOC_MAX};
    use crate::addon::loader::forge::InstallProfile;
    use crate::addon::loader::processors::{ProcessorPlan, ProcessorPlanInput};
    use crate::addon::loader::LoaderKind;
    use crate::game::arguments::Environment;
    use crate::game::layout::GameLayout;

    /// 一步 processor + 一个要从 installer 里解包的 data 值。
    const PROFILE: &str = r#"{
      "spec": 1, "minecraft": "1.20.1", "json": "/version.json",
      "processors": [{"jar": "net.minecraftforge:binarypatcher:1.1.1", "args": ["--apply", "{BINPATCH}"]}],
      "libraries": [],
      "data": {"BINPATCH": {"client": "/data/client.lzma"}}
    }"#;

    #[test]
    fn preallocation_is_capped_so_a_forged_header_cannot_oom() {
        // installer jar 没有校验和：header 可以声称 16 EiB 的解压尺寸。
        assert_eq!(prealloc_hint(u64::MAX), PREALLOC_MAX as usize);
        assert_eq!(prealloc_hint(1 << 40), PREALLOC_MAX as usize);
        // 合法条目照旧按实际尺寸预分配（封顶不影响常见大小）。
        assert_eq!(prealloc_hint(0), 0);
        assert_eq!(prealloc_hint(4096), 4096);
        assert_eq!(prealloc_hint(PREALLOC_MAX), PREALLOC_MAX as usize);
        assert_eq!(prealloc_hint(PREALLOC_MAX - 1), PREALLOC_MAX as usize - 1);
    }

    #[test]
    fn prepare_extracts_the_declared_entry_and_copies_the_client_jar() {
        let root = super::super::tests::temp_root("prepare-extract");
        let layout = GameLayout::new(&root);
        let profile: InstallProfile = serde_json::from_str(PROFILE).expect("夹具应可解析");
        let installer = root.join("cache/installer.jar");
        write_zip(&installer, "data/client.lzma", b"patch-bytes");
        let vanilla = root.join("versions/1.20.1/1.20.1.jar");
        std::fs::create_dir_all(vanilla.parent().expect("有父目录")).expect("建目录");
        std::fs::write(&vanilla, b"client-jar").expect("写原版 jar");

        let plan = ProcessorPlan::build(ProcessorPlanInput {
            kind: LoaderKind::Forge,
            mc_version: "1.20.1",
            version_id: "1.20.1-forge-47.2.0",
            profile: &profile,
            layout: &layout,
            installer: &installer,
            scratch: &root.join("scratch"),
            environment: &Environment::host(),
        })
        .expect("计划应能组出来");
        assert_eq!(plan.variables.embedded().len(), 1);

        prepare(&plan).expect("准备应成功");
        let dest = &plan.variables.embedded()[0].dest;
        assert_eq!(std::fs::read(dest).expect("读解包产物"), b"patch-bytes");
        // processor 拿到的是副本，共享层的原版 jar 内容不受影响。
        assert_eq!(
            std::fs::read(&plan.minecraft_jar_copy).expect("读 client jar 副本"),
            b"client-jar"
        );
        assert_ne!(plan.minecraft_jar_copy, plan.minecraft_jar);
        // 幂等：重跑按大小判定，不重复复制、不报错。
        prepare(&plan).expect("重跑应成功");
    }

    /// 写一个只含一条 Stored 条目的 zip（installer 夹具）。
    fn write_zip(path: &Path, entry: &str, bytes: &[u8]) {
        use std::io::Write;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建目录");
        }
        let file = std::fs::File::create(path).expect("建 zip");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file(entry, options).expect("写条目");
        writer.write_all(bytes).expect("写内容");
        writer.finish().expect("收尾 zip");
    }
}
