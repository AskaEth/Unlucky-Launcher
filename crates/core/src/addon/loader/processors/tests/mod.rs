//! `addon::loader::processors` 的单元测试：占位替换 / 计划形状 / 执行与失败分流 / 幂等跳过。
//!
//! 刻意**全部离线**：installer 用夹具 JSON 描述，依赖库的"在不在盘上"由测试自己摆，
//! 真正外调 java 的用例只走「起不来」这一条确定性分支（假 java 路径不存在），
//! 真机执行由验收（Forge 1.20.1 / NeoForge 21.1.80）覆盖。

mod exec;
mod plan;
mod vars;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::addon::loader::forge::InstallProfile;

/// 实测 Forge 1.20.1-47.2.0 的形状（裁剪到 3 个 processor：2 个 client 侧 + 1 个仅 server）。
pub(crate) const FORGE_PROFILE: &str = r#"{
  "spec": 1,
  "profile": "forge",
  "version": "1.20.1-forge-47.2.0",
  "minecraft": "1.20.1",
  "json": "/version.json",
  "processors": [
    {
      "sides": ["client"],
      "jar": "net.minecraftforge:installertools:1.3.0",
      "classpath": ["net.minecraftforge:jarsplitter:1.1.4"],
      "args": ["--task", "MCP_DATA", "--input", "{MAPPINGS}", "--output", "{MC_SLIM}", "--side", "{SIDE}"],
      "outputs": {"{MC_SLIM}": "aa11bb22cc33dd44ee55ff660011223344556677"}
    },
    {
      "sides": ["server"],
      "jar": "net.minecraftforge:installertools:1.3.0",
      "classpath": [],
      "args": ["--task", "SERVER"],
      "outputs": {}
    },
    {
      "jar": "net.minecraftforge:jarsplitter:1.1.4",
      "classpath": ["net.minecraftforge:installertools:1.3.0"],
      "args": ["--input", "{MINECRAFT_JAR}", "--output", "{MC_EXTRA}", "--singleThreaded"],
      "outputs": {"{MC_EXTRA}": "'not-a-sha1'"}
    }
  ],
  "libraries": [
    {"name": "net.minecraftforge:installertools:1.3.0", "url": "https://maven.minecraftforge.net/"},
    {"name": "net.minecraftforge:jarsplitter:1.1.4", "url": "https://maven.minecraftforge.net/"}
  ],
  "data": {
    "MAPPINGS": {"client": "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip]", "server": "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip]"},
    "MC_SLIM": {"client": "[net.minecraft:client:1.20.1-20230612.114412:slim]", "server": "[net.minecraft:server:1.20.1-20230612.114412:slim]"},
    "MC_EXTRA": {"client": "[net.minecraft:client:1.20.1-20230612.114412:extra]", "server": "[net.minecraft:server:1.20.1-20230612.114412:extra]"},
    "BINPATCH": {"client": "/data/client.lzma", "server": "/data/server.lzma"},
    "SERVER_ONLY": {"server": "'server'"}
  }
}"#;

/// 夹具 profile。
pub(crate) fn profile() -> InstallProfile {
    serde_json::from_str(FORGE_PROFILE).expect("夹具 install profile 应可解析")
}

/// 一次性临时目录（用例结束不清理，落在系统 temp 下）。
pub(crate) fn temp_root(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ull-processors-{}-{tag}-{unique}",
        std::process::id()
    ))
}
