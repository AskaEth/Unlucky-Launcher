//! 依赖解析用的录制响应（形状按站点原文；数值是示意，不是某天的真值）。
//!
//! 从 `fixtures.rs` 分出来的原因很实在：那里已经在钉「字段名与 `primary`」这类解析细节，
//! 再往里塞四五个带 `dependencies` 的版本对象就会贴到 400 行硬线。
//!
//! 三条刻意的布置：
//! - 根项目（`sodium`）的版本里**同时有四类依赖**：只有 `required` 该被跟；
//! - 每个版本都有 `primary: false` 的 `sources.jar`，取第一个就会装错；
//! - 环（`cycle-a` ⇄ `cycle-b`）与扇入（两个分支都要 `shared-lib`）各有一组独立夹具。

/// 根项目 `sodium` 的版本：一条 required + 一条 optional + 一条 incompatible + 一条 embedded。
pub(super) const SODIUM_WITH_DEPENDENCIES: &str = r#"[
  {
    "id": "ver-sodium",
    "project_id": "AANobbMI",
    "name": "Sodium 0.6.0 for 1.20.1",
    "version_number": "mc1.20.1-0.6.0",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "date_published": "2026-02-01T12:00:00Z",
    "downloads": 4321,
    "files": [
      {
        "hashes": {"sha1": "1111111111111111111111111111111111111111", "sha512": "aa"},
        "url": "https://cdn.modrinth.com/data/AANobbMI/versions/ver-sodium/sodium-sources.jar",
        "filename": "sodium-sources.jar",
        "size": 1000,
        "primary": false
      },
      {
        "hashes": {"sha1": "2222222222222222222222222222222222222222", "sha512": "bb"},
        "url": "https://cdn.modrinth.com/data/AANobbMI/versions/ver-sodium/sodium-fabric-0.6.0.jar",
        "filename": "sodium-fabric-0.6.0.jar",
        "size": 2048,
        "primary": true
      }
    ],
    "dependencies": [
      {"version_id": null, "project_id": "P7dR8mSH", "file_name": null, "dependency_type": "required"},
      {"version_id": null, "project_id": "OCS3z7f6", "file_name": null, "dependency_type": "optional"},
      {"version_id": null, "project_id": "AAAABBBB", "file_name": null, "dependency_type": "incompatible"},
      {"version_id": "ver-embedded", "project_id": null, "file_name": "embedded-lib.jar", "dependency_type": "embedded"}
    ]
  }
]"#;

/// `fabric-api`（`P7dR8mSH`）：它自己又要求 `yacl`（`1eAoo2KR`）⇒ 第二层嵌套。
pub(super) const FABRIC_API_WITH_DEPENDENCIES: &str = r#"[
  {
    "id": "ver-fabric-api",
    "project_id": "P7dR8mSH",
    "name": "Fabric API 0.92.2 for 1.20.1",
    "version_number": "0.92.2+1.20.1",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "date_published": "2025-12-01T00:00:00Z",
    "downloads": 9999,
    "files": [
      {
        "hashes": {"sha1": "3333333333333333333333333333333333333333", "sha512": "cc"},
        "url": "https://cdn.modrinth.com/data/P7dR8mSH/versions/ver-fabric-api/fabric-api-0.92.2-sources.jar",
        "filename": "fabric-api-0.92.2-sources.jar",
        "size": 512,
        "primary": false
      },
      {
        "hashes": {"sha1": "4444444444444444444444444444444444444444", "sha512": "dd"},
        "url": "https://cdn.modrinth.com/data/P7dR8mSH/versions/ver-fabric-api/fabric-api-0.92.2+1.20.1.jar",
        "filename": "fabric-api-0.92.2+1.20.1.jar",
        "size": 4096,
        "primary": true
      }
    ],
    "dependencies": [
      {"version_id": null, "project_id": "1eAoo2KR", "file_name": null, "dependency_type": "required"}
    ]
  }
]"#;

/// `yacl`（`1eAoo2KR`）：链的末端，没有依赖。
pub(super) const YACL: &str = r#"[
  {
    "id": "ver-yacl",
    "project_id": "1eAoo2KR",
    "name": "YetAnotherConfigLib 3.3.2",
    "version_number": "3.3.2+1.20.1",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "date_published": "2025-11-01T00:00:00Z",
    "downloads": 1234,
    "files": [
      {
        "hashes": {"sha1": "5555555555555555555555555555555555555555", "sha512": "ee"},
        "url": "https://cdn.modrinth.com/data/1eAoo2KR/versions/ver-yacl/yacl-3.3.2.jar",
        "filename": "yacl-3.3.2+1.20.1.jar",
        "size": 8192,
        "primary": true
      }
    ],
    "dependencies": []
  }
]"#;

/// 扇入的根：两个 required 分支。
pub(super) const FANIN_ROOT: &str = r#"[
  {
    "id": "ver-root",
    "project_id": "ROOT0001",
    "name": "Fan-in Root 1.0",
    "version_number": "1.0",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "files": [
      {
        "hashes": {"sha1": "6666666666666666666666666666666666666666", "sha512": "ff"},
        "url": "https://cdn.modrinth.com/data/ROOT0001/versions/ver-root/fanin-root-1.0.jar",
        "filename": "fanin-root-1.0.jar",
        "size": 1024,
        "primary": true
      }
    ],
    "dependencies": [
      {"project_id": "BRANCHAA", "dependency_type": "required"},
      {"project_id": "BRANCHBB", "dependency_type": "required"}
    ]
  }
]"#;

/// 分支 A：也要 `shared-lib`。
pub(super) const FANIN_A: &str = r#"[
  {
    "id": "ver-branch-a",
    "project_id": "BRANCHAA",
    "name": "Branch A 2.0",
    "version_number": "2.0",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "files": [
      {
        "hashes": {"sha1": "7777777777777777777777777777777777777777", "sha512": "ab"},
        "url": "https://cdn.modrinth.com/data/BRANCHAA/versions/ver-branch-a/branch-a-2.0.jar",
        "filename": "branch-a-2.0.jar",
        "size": 1024,
        "primary": true
      }
    ],
    "dependencies": [
      {"project_id": "SHARED01", "dependency_type": "required"}
    ]
  }
]"#;

/// 分支 B：也要 `shared-lib`（同一项目两条路径 ⇒ 只装一次）。
pub(super) const FANIN_B: &str = r#"[
  {
    "id": "ver-branch-b",
    "project_id": "BRANCHBB",
    "name": "Branch B 3.0",
    "version_number": "3.0",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "files": [
      {
        "hashes": {"sha1": "8888888888888888888888888888888888888888", "sha512": "ac"},
        "url": "https://cdn.modrinth.com/data/BRANCHBB/versions/ver-branch-b/branch-b-3.0.jar",
        "filename": "branch-b-3.0.jar",
        "size": 1024,
        "primary": true
      }
    ],
    "dependencies": [
      {"project_id": "SHARED01", "dependency_type": "required"}
    ]
  }
]"#;

/// 扇入的公共前置。
pub(super) const SHARED_LIB: &str = r#"[
  {
    "id": "ver-shared",
    "project_id": "SHARED01",
    "name": "Shared Lib 1.5",
    "version_number": "1.5",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "files": [
      {
        "hashes": {"sha1": "9999999999999999999999999999999999999999", "sha512": "ad"},
        "url": "https://cdn.modrinth.com/data/SHARED01/versions/ver-shared/shared-lib-1.5.jar",
        "filename": "shared-lib-1.5.jar",
        "size": 2048,
        "primary": true
      }
    ],
    "dependencies": []
  }
]"#;

/// 环的一端：`cycle-a` 要求 `cycle-b`。
pub(super) const CYCLE_A: &str = r#"[
  {
    "id": "ver-cycle-a",
    "project_id": "cycAid01",
    "name": "Cycle A 1.0",
    "version_number": "1.0",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "files": [
      {
        "hashes": {"sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "sha512": "a1"},
        "url": "https://cdn.modrinth.com/data/cycAid01/versions/ver-cycle-a/cycle-a-1.0.jar",
        "filename": "cycle-a-1.0.jar",
        "size": 1024,
        "primary": true
      }
    ],
    "dependencies": [
      {"project_id": "cycBid01", "dependency_type": "required"}
    ]
  }
]"#;

/// 环的另一端：`cycle-b` 又要求 `cycle-a`（不设防就是无限递归）。
pub(super) const CYCLE_B: &str = r#"[
  {
    "id": "ver-cycle-b",
    "project_id": "cycBid01",
    "name": "Cycle B 1.0",
    "version_number": "1.0",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.20.1"],
    "files": [
      {
        "hashes": {"sha1": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "sha512": "b1"},
        "url": "https://cdn.modrinth.com/data/cycBid01/versions/ver-cycle-b/cycle-b-1.0.jar",
        "filename": "cycle-b-1.0.jar",
        "size": 1024,
        "primary": true
      }
    ],
    "dependencies": [
      {"project_id": "cycAid01", "dependency_type": "required"}
    ]
  }
]"#;

/// 光影检索的一页：`categories` 里是 `iris` / `optifine`（光影的加载器）。
pub(super) const SEARCH_SHADER: &str = r#"{
  "hits": [
    {
      "project_id": "shader01",
      "project_type": "shader",
      "slug": "complementary-reimagined",
      "author": "EminGT",
      "title": "Complementary Reimagined",
      "description": "A shader pack",
      "categories": ["iris", "optifine", "shaders"],
      "versions": ["1.20.1"],
      "downloads": 1234567,
      "follows": 4242,
      "icon_url": "",
      "date_modified": "2026-01-02T03:04:05Z"
    }
  ],
  "offset": 0,
  "limit": 1,
  "total_hits": 1
}"#;
