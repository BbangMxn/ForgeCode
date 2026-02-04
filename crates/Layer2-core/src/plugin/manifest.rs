//! Plugin Manifest - 플러그인 메타데이터 정의

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 플러그인 버전
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PluginVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    /// 버전 문자열 파싱 (예: "1.2.3")
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    /// 호환성 검사
    pub fn is_compatible_with(&self, other: &PluginVersion) -> bool {
        // 같은 메이저 버전이면 호환
        self.major == other.major
    }
}

impl std::fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Default for PluginVersion {
    fn default() -> Self {
        Self::new(1, 0, 0)
    }
}

/// 플러그인 의존성
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// 의존하는 플러그인 이름
    pub name: String,

    /// 최소 필요 버전
    pub min_version: PluginVersion,

    /// 선택적 의존성 여부
    pub optional: bool,
}

impl PluginDependency {
    pub fn new(name: impl Into<String>, min_version: PluginVersion) -> Self {
        Self {
            name: name.into(),
            min_version,
            optional: false,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

/// 플러그인 매니페스트 - 플러그인의 모든 메타데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// 고유 플러그인 ID (예: "forge.git-enhanced")
    pub id: String,

    /// 표시 이름
    pub name: String,

    /// 버전
    pub version: PluginVersion,

    /// 설명
    pub description: String,

    /// 작성자
    pub author: Option<String>,

    /// 라이선스
    pub license: Option<String>,

    /// 홈페이지/리포지토리 URL
    pub homepage: Option<String>,

    /// 의존성 목록
    pub dependencies: Vec<PluginDependency>,

    /// 제공하는 기능 목록
    pub provides: PluginProvides,

    /// 설정 스키마
    pub config_schema: Option<serde_json::Value>,

    /// 플러그인 타입
    pub plugin_type: PluginType,

    /// 추가 메타데이터
    pub metadata: HashMap<String, String>,
}

impl PluginManifest {
    /// 새 매니페스트 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: PluginVersion::default(),
            description: String::new(),
            author: None,
            license: None,
            homepage: None,
            dependencies: vec![],
            provides: PluginProvides::default(),
            config_schema: None,
            plugin_type: PluginType::Native,
            metadata: HashMap::new(),
        }
    }

    /// 빌더 패턴: 버전 설정
    pub fn with_version(mut self, version: PluginVersion) -> Self {
        self.version = version;
        self
    }

    /// 빌더 패턴: 설명 설정
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// 빌더 패턴: 작성자 설정
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// 빌더 패턴: 의존성 추가
    pub fn with_dependency(mut self, dep: PluginDependency) -> Self {
        self.dependencies.push(dep);
        self
    }

    /// 빌더 패턴: 제공 기능 설정
    pub fn with_provides(mut self, provides: PluginProvides) -> Self {
        self.provides = provides;
        self
    }

    /// 빌더 패턴: 플러그인 타입 설정
    pub fn with_type(mut self, plugin_type: PluginType) -> Self {
        self.plugin_type = plugin_type;
        self
    }

    /// 빌더 패턴: 메타데이터 추가
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// 플러그인이 제공하는 기능 목록
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginProvides {
    /// 제공하는 Tool 이름들
    pub tools: Vec<String>,

    /// 제공하는 Skill 이름들
    pub skills: Vec<String>,

    /// 제공하는 이벤트 핸들러
    pub event_handlers: Vec<String>,

    /// 시스템 프롬프트 수정 여부
    pub modifies_system_prompt: bool,
}

impl PluginProvides {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tools.push(tool.into());
        self
    }

    pub fn with_skill(mut self, skill: impl Into<String>) -> Self {
        self.skills.push(skill.into());
        self
    }

    pub fn with_event_handler(mut self, handler: impl Into<String>) -> Self {
        self.event_handlers.push(handler.into());
        self
    }

    pub fn modifies_prompt(mut self) -> Self {
        self.modifies_system_prompt = true;
        self
    }
}

/// 플러그인 타입
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    /// 네이티브 Rust 플러그인
    Native,

    /// WebAssembly 플러그인
    Wasm,

    /// 스크립트 플러그인 (JS/Lua)
    Script,

    /// 원격 플러그인 (MCP 서버 등)
    Remote,
}

impl Default for PluginType {
    fn default() -> Self {
        Self::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parse() {
        let v = PluginVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_version_compatibility() {
        let v1 = PluginVersion::new(1, 0, 0);
        let v2 = PluginVersion::new(1, 2, 0);
        let v3 = PluginVersion::new(2, 0, 0);

        assert!(v1.is_compatible_with(&v2));
        assert!(!v1.is_compatible_with(&v3));
    }

    #[test]
    fn test_manifest_builder() {
        let manifest = PluginManifest::new("test.plugin", "Test Plugin")
            .with_version(PluginVersion::new(1, 0, 0))
            .with_description("A test plugin")
            .with_author("Test Author")
            .with_provides(
                PluginProvides::new()
                    .with_tool("custom_tool")
                    .with_skill("custom_skill")
            );

        assert_eq!(manifest.id, "test.plugin");
        assert_eq!(manifest.provides.tools.len(), 1);
        assert_eq!(manifest.provides.skills.len(), 1);
    }
}
