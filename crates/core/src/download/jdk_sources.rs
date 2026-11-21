//! JDK 分发源表（`mirrors.json` v3）：Oracle OpenJDK / Microsoft OpenJDK / Azul Zulu。
//!
//! v2 只有 `adoptium`（Temurin）一张表，因为当时只做一家发行版。D23 定下三家 + 第四备选后，
//! 三家的分发形态各不相同，表结构也就各有一套（research/06 §5.2 条目样例照抄）：
//!
//! | 发行版 | 枚举方式 | 下载形态 |
//! | --- | --- | --- |
//! | Oracle OpenJDK | Apache 目录页（华为云 / 南大，与 `download.java.net` 同构） | `{base}/{ver}/{filename}`，hash 段不可推导 |
//! | Microsoft OpenJDK | 内置版本快照（官方只有人读 HTML 表） | `aka.ms` 模板 301 → VS CDN，校验走成对的 `.sha256sum.txt` |
//! | Zulu | 免登录 JSON API（两跳：列表 → 详情） | `download_url` 直出，只信 `sha256_hash` |
//!
//! 表本身只是「数据」：怎么用由 `game::java` 的 `oracle` / `microsoft` / `zulu` 决定。
//! v3 兼容读 v2：新表全部 `#[serde(default)]`，老文件照常解析（新表为空，回退链自然退化成只有 Temurin）。

use serde::{Deserialize, Serialize};

use super::mirrors::MirrorTable;

const fn default_true() -> bool {
    true
}

/// 一个 Oracle OpenJDK 镜像（`download.java.net/openjdk` 的忠实镜像）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleMirror {
    /// 源标识（`huawei` / `nju`）。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 目录根（形如 `https://mirrors.huaweicloud.com/openjdk`）。
    pub base: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 实测吞吐（展示用，不参与决策）。
    #[serde(default)]
    pub speed: String,
    /// 备注（展示用）。
    #[serde(default)]
    pub note: String,
}

impl OracleMirror {
    /// 目录根（去掉尾斜杠，拼路径时不会再出双斜杠）。
    #[must_use]
    pub fn base(&self) -> &str {
        self.base.trim_end_matches('/')
    }
}

/// Microsoft OpenJDK 的分发表：官方模板 + 内置版本快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsOpenJdkTable {
    /// 人读清单页（刷新快照时抓的页面，仅供排查）。
    #[serde(default)]
    pub list: String,
    /// 下载模板：`microsoft-jdk-{version}-{os}-{arch}.zip`。
    pub template: String,
    /// 校验模板：同一文件名 + `.sha256sum.txt`。
    pub sha256_template: String,
    /// 官方在发的 GA 大版本（没有 8——微软已停发，官方页面让去用 Temurin 8）。
    #[serde(default)]
    pub majors: Vec<u32>,
    /// 国内镜像（实测为空数组：三家云都没有 MS OpenJDK）。
    #[serde(default)]
    pub domestic_mirrors: Vec<String>,
    /// 实测吞吐（展示用）。
    #[serde(default)]
    pub measured_speed: String,
}

/// Zulu（Azul）的元数据表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZuluTable {
    /// 包列表 API 根。
    pub api: String,
    /// 包详情 API（`{uuid}` 占位）——`latest=true` 的响应不含 sha256，必须再取一次。
    pub detail: String,
    /// CDN host（`cdn.azul.com`，展示用）。
    #[serde(default)]
    pub cdn_host: String,
    /// 实测吞吐（展示用）。
    #[serde(default)]
    pub measured_speed: String,
    /// `size` 字段是否可与真实字节数对拍（实测 `false`，差 133 B）。
    #[serde(default)]
    pub size_field_reliable: bool,
}

impl MirrorTable {
    /// 启用的 Oracle OpenJDK 镜像（清单顺序 = 默认尝试顺序）。
    pub fn oracle_mirrors(&self) -> impl Iterator<Item = &OracleMirror> {
        self.oracle_openjdk.iter().filter(|mirror| mirror.enabled)
    }

    /// 按 id 找 Oracle 镜像（诊断 / `java vendors` 用；停用的也能查到）。
    #[must_use]
    pub fn oracle_mirror(&self, id: &str) -> Option<&OracleMirror> {
        self.oracle_openjdk
            .iter()
            .find(|mirror| mirror.id.eq_ignore_ascii_case(id))
    }

    /// Microsoft OpenJDK 分发表（v2 清单没有 → `None`）。
    #[must_use]
    pub fn microsoft_openjdk(&self) -> Option<&MsOpenJdkTable> {
        self.microsoft_openjdk.as_ref()
    }

    /// Zulu 元数据表。
    #[must_use]
    pub fn zulu(&self) -> Option<&ZuluTable> {
        self.zulu.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::MirrorTable;

    #[test]
    fn builtin_table_is_v3_with_all_four_source_families() {
        let table = MirrorTable::builtin();
        assert_eq!(table.version, 3);
        // Temurin：只剩 TUNA 一家启用，且带 latest_only 标记。
        let enabled: Vec<&str> = table
            .adoptium
            .iter()
            .filter(|mirror| mirror.enabled)
            .map(|mirror| mirror.id.as_str())
            .collect();
        assert_eq!(enabled, vec!["tuna"]);
        assert!(table
            .adoptium
            .iter()
            .filter(|m| m.enabled)
            .all(|m| m.latest_only));
        // 停用的登记项要在（将来恢复时不需要重写清单）。
        for id in ["ustc", "sjtu", "bfsu", "nju", "zju", "163"] {
            assert!(
                table.adoptium.iter().any(|mirror| mirror.id == id),
                "{id} 应登记为 disabled"
            );
        }
        assert!(table.oracle_mirrors().any(|mirror| mirror.id == "huawei"));
        let microsoft = table.microsoft_openjdk().expect("v3 应带 MS 表");
        assert_eq!(microsoft.majors, vec![25, 21, 17, 11]);
        assert!(microsoft.template.contains("{version}"));
        assert!(microsoft.sha256_template.ends_with(".sha256sum.txt"));
        let zulu = table.zulu().expect("v3 应带 Zulu 表");
        assert!(zulu.detail.contains("{uuid}"));
        assert!(!zulu.size_field_reliable, "Zulu 的 size 字段不可对拍");
    }

    #[test]
    fn v2_table_still_parses() {
        // v2 清单（没有三张新表）必须照常解析，回退链退化为「只有 Temurin」。
        let v2 = r#"{
            "version": 2,
            "adoptium": [
                { "id": "tuna", "name": "清华 TUNA",
                  "base": "https://mirrors.tuna.tsinghua.edu.cn/Adoptium", "enabled": true }
            ],
            "gh_proxies": [],
            "rules": []
        }"#;
        let table = MirrorTable::parse(v2).expect("v2 应兼容");
        assert_eq!(table.version, 2);
        assert_eq!(table.adoptium.len(), 1);
        assert!(
            !table.adoptium[0].latest_only,
            "v2 没有 latest_only → 默认 false"
        );
        assert!(table.microsoft_openjdk().is_none());
        assert!(table.zulu().is_none());
        assert_eq!(table.oracle_mirrors().count(), 0);
    }
}
