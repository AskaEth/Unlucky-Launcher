//! 录制下来的 Modrinth v2 响应（**形状按站点原文，字段抄真**；数值是示意，不是某天的真值）。
//!
//! 三条刻意的细节：
//! - 检索命中的日期字段是 `date_modified`（不是 `updated`）——旧实现读错了字段；
//! - `categories` 里混着 `optimization` 这种分类标签，只有 `fabric` 是真加载器；
//! - 版本里的文件有 `primary: false` 的 `sources.jar`，取第一个会装错。

/// `/v2/search?query=sodium`（两页里的第一页）。
pub(super) const SEARCH: &str = r#"{
  "hits": [
    {
      "project_id": "AANobbMI",
      "project_type": "mod",
      "slug": "sodium",
      "author": "jellysquid3",
      "title": "Sodium",
      "description": "Modern rendering engine and client-side optimization mod",
      "categories": ["fabric", "optimization", "quilt"],
      "display_categories": ["optimization"],
      "versions": ["1.20.1", "1.21.1"],
      "downloads": 42123456,
      "follows": 9912,
      "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png",
      "date_created": "2020-12-01T00:00:00Z",
      "date_modified": "2026-02-03T10:11:12Z",
      "latest_version": "mc1.21.1-0.6.0",
      "license": "LGPL-3.0-only",
      "client_side": "required",
      "server_side": "unsupported",
      "gallery": []
    },
    {
      "project_id": "P7dR8mSH",
      "project_type": "mod",
      "slug": "fabric-api",
      "author": "modmuss50",
      "title": "Fabric API",
      "description": "Lightweight and modular API providing common hooks and intercompatibility measures",
      "categories": ["fabric", "library"],
      "versions": ["1.20.1", "1.21.1"],
      "downloads": 99000000,
      "follows": 12000,
      "icon_url": "",
      "date_modified": "2026-01-30T08:00:00Z"
    }
  ],
  "offset": 0,
  "limit": 2,
  "total_hits": 1234
}"#;

/// `/v2/project/sodium`。
pub(super) const PROJECT: &str = r###"{
  "id": "AANobbMI",
  "slug": "sodium",
  "project_type": "mod",
  "team": "team-id",
  "title": "Sodium",
  "description": "Modern rendering engine and client-side optimization mod",
  "body": "## Sodium\n\nA modern rendering engine for Minecraft which greatly improves performance.",
  "published": "2020-12-01T00:00:00Z",
  "updated": "2026-02-03T10:11:12Z",
  "downloads": 42123456,
  "followers": 9912,
  "categories": ["fabric", "optimization"],
  "game_versions": ["1.20.1", "1.21.1"],
  "loaders": ["fabric", "quilt"],
  "versions": ["ver-1", "ver-2"],
  "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png",
  "source_url": "https://github.com/CaffeineMC/sodium",
  "issues_url": "https://github.com/CaffeineMC/sodium/issues"
}"###;

/// `/v2/project/sodium/version`（站点顺序 = 最新在前）。
pub(super) const VERSIONS: &str = r#"[
  {
    "id": "ver-2",
    "project_id": "AANobbMI",
    "name": "Sodium 0.6.0 for 1.21.1",
    "version_number": "mc1.21.1-0.6.0",
    "version_type": "release",
    "loaders": ["fabric"],
    "game_versions": ["1.21.1"],
    "date_published": "2026-02-01T12:00:00Z",
    "downloads": 4321,
    "files": [
      {
        "hashes": {"sha1": "1111111111111111111111111111111111111111", "sha512": "aa"},
        "url": "https://cdn.modrinth.com/data/AANobbMI/versions/ver-2/sodium-sources.jar",
        "filename": "sodium-sources.jar",
        "size": 1000,
        "primary": false
      },
      {
        "hashes": {"sha1": "2222222222222222222222222222222222222222", "sha512": "bb"},
        "url": "https://cdn.modrinth.com/data/AANobbMI/versions/ver-2/sodium-fabric-0.6.0.jar",
        "filename": "sodium-fabric-0.6.0.jar",
        "size": 2048,
        "primary": true
      }
    ]
  }
]"#;

/// `GET /v2/version_file/{sha512}?algorithm=sha512` 的响应（一个版本对象）。
///
/// 里面有两个文件：只有 `hashes.sha512` 与请求值相同的那条能取 sha1（另一个是 `sources.jar`）。
pub(super) const VERSION_FILE_A: &str = r#"{
  "id": "ver-a",
  "project_id": "AANobbMI",
  "name": "Sodium 0.5.11 for 1.20.1",
  "version_number": "mc1.20.1-0.5.11",
  "version_type": "release",
  "loaders": ["fabric"],
  "game_versions": ["1.20.1"],
  "date_published": "2025-06-01T00:00:00Z",
  "downloads": 9999,
  "files": [
    {
      "hashes": {"sha1": "3333333333333333333333333333333333333333", "sha512": "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1"},
      "url": "https://cdn.modrinth.com/data/AANobbMI/versions/ver-a/sodium-fabric-0.5.11.jar",
      "filename": "sodium-fabric-0.5.11.jar",
      "size": 4096,
      "primary": true
    },
    {
      "hashes": {"sha1": "4444444444444444444444444444444444444444", "sha512": "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"},
      "url": "https://cdn.modrinth.com/data/AANobbMI/versions/ver-a/sodium-sources.jar",
      "filename": "sodium-sources.jar",
      "size": 512,
      "primary": false
    }
  ]
}"#;

/// 补全时请求的那个 sha512（128 位十六进制；形状不对客户端根本不发请求）。
pub(super) const LOOKUP_SHA512: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";

/// 同一版本里另一个文件的 sha512（`sources.jar`，不该被拿来当身份）。
pub(super) const OTHER_SHA512: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

/// 补全应该拿到的 sha1（`VERSION_FILE_A` 主文件那条）。
pub(super) const LOOKUP_SHA1: &str = "3333333333333333333333333333333333333333";
