//! 권한 타입 정의 (동적 등록 방식)
//!
//! 각 모듈에서 자신의 권한을 등록하고, foundation은 저장/조회만 담당

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// 권한 정의
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDef {
    /// 권한 이름 (예: "file.read", "bash.execute", "mcp.tool_call")
    pub name: String,

    /// 카테고리 (예: "filesystem", "execute", "network", "mcp")
    pub category: String,

    /// 위험도 (0-10)
    pub risk_level: u8,

    /// 설명
    pub description: String,

    /// 사용자 확인 필요 여부 (기본: risk_level >= 5)
    #[serde(default)]
    pub requires_confirmation: Option<bool>,
}

impl PermissionDef {
    pub fn new(name: impl Into<String>, category: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            category: category.into(),
            risk_level: 5,
            description: String::new(),
            requires_confirmation: None,
        }
    }

    pub fn risk_level(mut self, level: u8) -> Self {
        self.risk_level = level.min(10);
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn requires_confirmation(mut self, required: bool) -> Self {
        self.requires_confirmation = Some(required);
        self
    }

    /// 확인 필요 여부 (설정값 또는 risk_level 기반)
    pub fn needs_confirmation(&self) -> bool {
        self.requires_confirmation.unwrap_or(self.risk_level >= 5)
    }
}

/// 권한 레지스트리 (전역 싱글톤)
pub struct PermissionRegistry {
    /// 등록된 권한 정의들 (name -> PermissionDef)
    definitions: RwLock<HashMap<String, PermissionDef>>,
}

impl PermissionRegistry {
    /// 새 레지스트리 생성
    pub fn new() -> Self {
        Self {
            definitions: RwLock::new(HashMap::new()),
        }
    }

    /// 권한 등록
    pub fn register(&self, def: PermissionDef) {
        if let Ok(mut defs) = self.definitions.write() {
            defs.insert(def.name.clone(), def);
        }
    }

    /// 여러 권한 등록
    pub fn register_all(&self, defs: Vec<PermissionDef>) {
        if let Ok(mut definitions) = self.definitions.write() {
            for def in defs {
                definitions.insert(def.name.clone(), def);
            }
        }
    }

    /// 권한 조회
    pub fn get(&self, name: &str) -> Option<PermissionDef> {
        self.definitions.read().ok()?.get(name).cloned()
    }

    /// 카테고리별 권한 조회
    pub fn by_category(&self, category: &str) -> Vec<PermissionDef> {
        self.definitions
            .read()
            .map(|defs| {
                defs.values()
                    .filter(|d| d.category == category)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 모든 권한 조회
    pub fn all(&self) -> Vec<PermissionDef> {
        self.definitions
            .read()
            .map(|defs| defs.values().cloned().collect())
            .unwrap_or_default()
    }

    /// 모든 카테고리 조회
    pub fn categories(&self) -> Vec<String> {
        self.definitions
            .read()
            .map(|defs| {
                let mut cats: Vec<_> = defs.values().map(|d| d.category.clone()).collect();
                cats.sort();
                cats.dedup();
                cats
            })
            .unwrap_or_default()
    }

    /// 위험도 이상의 권한 조회
    pub fn by_risk_level(&self, min_level: u8) -> Vec<PermissionDef> {
        self.definitions
            .read()
            .map(|defs| {
                defs.values()
                    .filter(|d| d.risk_level >= min_level)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 확인이 필요한 권한 조회
    pub fn requiring_confirmation(&self) -> Vec<PermissionDef> {
        self.definitions
            .read()
            .map(|defs| {
                defs.values()
                    .filter(|d| d.needs_confirmation())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 권한 존재 여부
    pub fn exists(&self, name: &str) -> bool {
        self.definitions
            .read()
            .map(|defs| defs.contains_key(name))
            .unwrap_or(false)
    }

    /// 등록된 권한 개수
    pub fn count(&self) -> usize {
        self.definitions.read().map(|d| d.len()).unwrap_or(0)
    }
}

impl Default for PermissionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// 전역 레지스트리
static REGISTRY: std::sync::OnceLock<PermissionRegistry> = std::sync::OnceLock::new();

/// 전역 권한 레지스트리 접근
pub fn registry() -> &'static PermissionRegistry {
    REGISTRY.get_or_init(PermissionRegistry::new)
}

/// 권한 등록 (편의 함수)
pub fn register(def: PermissionDef) {
    registry().register(def);
}

/// 여러 권한 등록 (편의 함수)
pub fn register_all(defs: Vec<PermissionDef>) {
    registry().register_all(defs);
}

// ============================================================
// 표준 카테고리 상수 (권장, 필수 아님)
// ============================================================

pub mod categories {
    pub const FILESYSTEM: &str = "filesystem";
    pub const EXECUTE: &str = "execute";
    pub const NETWORK: &str = "network";
    pub const MCP: &str = "mcp";
    pub const SYSTEM: &str = "system";
}

// ============================================================
// 유틸리티
// ============================================================

/// 민감한 경로 패턴 (참고용)
pub fn sensitive_paths() -> Vec<&'static str> {
    vec![
        "**/.env",
        "**/.env.*",
        "**/credentials*",
        "**/secrets*",
        "**/*.pem",
        "**/*.key",
        "**/*_rsa",
        "~/.ssh/**",
        "~/.aws/**",
        "~/.config/**",
    ]
}

/// 위험한 명령어 패턴 (참고용)
pub fn dangerous_commands() -> Vec<&'static str> {
    vec![
        "rm -rf /*",
        "rm -rf /",
        ":(){ :|:& };:",
        "dd if=*of=/dev/*",
        "mkfs.*",
        "> /dev/sda",
        "chmod -R 777 /",
        "sudo rm -rf",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_def_builder() {
        let def = PermissionDef::new("file.read", "filesystem")
            .risk_level(2)
            .description("Read file contents");

        assert_eq!(def.name, "file.read");
        assert_eq!(def.category, "filesystem");
        assert_eq!(def.risk_level, 2);
        assert!(!def.needs_confirmation());
    }

    #[test]
    fn test_permission_registry() {
        let registry = PermissionRegistry::new();

        registry.register(
            PermissionDef::new("test.read", "test")
                .risk_level(2)
                .description("Test read"),
        );

        registry.register(
            PermissionDef::new("test.write", "test")
                .risk_level(6)
                .description("Test write"),
        );

        registry.register(
            PermissionDef::new("other.action", "other")
                .risk_level(8)
                .description("Other action"),
        );

        assert_eq!(registry.count(), 3);
        assert!(registry.exists("test.read"));
        assert!(!registry.exists("nonexistent"));

        let test_perms = registry.by_category("test");
        assert_eq!(test_perms.len(), 2);

        let high_risk = registry.by_risk_level(6);
        assert_eq!(high_risk.len(), 2);

        let need_confirm = registry.requiring_confirmation();
        assert_eq!(need_confirm.len(), 2); // risk >= 5
    }
}
