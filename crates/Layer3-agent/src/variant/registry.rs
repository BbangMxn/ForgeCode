//! Agent Variant Registry
//!
//! Agent 변형들을 등록하고 관리하는 레지스트리입니다.

#![allow(dead_code)]

use crate::runtime::{AgentRuntime, RuntimeConfig};
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Variant Info
// ============================================================================

/// Agent 변형 카테고리
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VariantCategory {
    /// 기본/표준
    Standard,
    /// 추론 중심
    Reasoning,
    /// 계획 중심
    Planning,
    /// 실행 중심
    Execution,
    /// 실험적
    Experimental,
    /// 사용자 정의
    Custom,
}

impl std::fmt::Display for VariantCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VariantCategory::Standard => write!(f, "Standard"),
            VariantCategory::Reasoning => write!(f, "Reasoning"),
            VariantCategory::Planning => write!(f, "Planning"),
            VariantCategory::Execution => write!(f, "Execution"),
            VariantCategory::Experimental => write!(f, "Experimental"),
            VariantCategory::Custom => write!(f, "Custom"),
        }
    }
}

/// Agent 변형 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentVariantInfo {
    /// 변형 ID
    pub id: String,

    /// 표시 이름
    pub name: String,

    /// 버전
    pub version: String,

    /// 설명
    pub description: String,

    /// 카테고리
    pub category: VariantCategory,

    /// 추천 사용 사례
    pub recommended_for: Vec<String>,

    /// 기본 설정
    pub default_config: RuntimeConfig,

    /// 사용하는 전략들
    pub strategies: StrategiesInfo,

    /// 내장 여부
    pub is_builtin: bool,
}

/// 사용하는 전략 정보
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategiesInfo {
    pub reasoning: String,
    pub planning: String,
    pub memory: String,
    pub execution: String,
}

impl AgentVariantInfo {
    /// 새 변형 정보 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: "0.1.0".to_string(),
            description: String::new(),
            category: VariantCategory::Custom,
            recommended_for: Vec::new(),
            default_config: RuntimeConfig::default(),
            strategies: StrategiesInfo::default(),
            is_builtin: false,
        }
    }

    /// 설명 추가
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// 카테고리 설정
    pub fn with_category(mut self, category: VariantCategory) -> Self {
        self.category = category;
        self
    }

    /// 추천 사용 사례 추가
    pub fn recommended_for(mut self, use_case: impl Into<String>) -> Self {
        self.recommended_for.push(use_case.into());
        self
    }

    /// 내장 표시
    pub fn builtin(mut self) -> Self {
        self.is_builtin = true;
        self
    }
}

// ============================================================================
// Agent Factory
// ============================================================================

/// Agent 생성 팩토리 타입
pub type AgentFactory = Arc<dyn Fn(RuntimeConfig) -> Box<dyn AgentRuntime> + Send + Sync>;

// ============================================================================
// Agent Registry
// ============================================================================

/// Agent 변형 레지스트리
pub struct AgentRegistry {
    /// 등록된 변형 정보
    variants: HashMap<String, AgentVariantInfo>,

    /// 변형 생성 팩토리
    factories: HashMap<String, AgentFactory>,
}

impl AgentRegistry {
    /// 새 레지스트리 생성
    pub fn new() -> Self {
        Self {
            variants: HashMap::new(),
            factories: HashMap::new(),
        }
    }

    /// 변형 등록
    pub fn register(&mut self, info: AgentVariantInfo, factory: AgentFactory) {
        let id = info.id.clone();
        self.variants.insert(id.clone(), info);
        self.factories.insert(id, factory);
    }

    /// 내장 변형 등록 (BuiltinVariant 트레이트 사용)
    pub fn register_builtin<V: BuiltinVariant + 'static>(&mut self) {
        let info = V::variant_info();
        let id = info.id.clone();

        let factory: AgentFactory = Arc::new(move |config| Box::new(V::create(config)));

        self.variants.insert(id.clone(), info);
        self.factories.insert(id, factory);
    }

    /// 변형 조회
    pub fn get(&self, id: &str) -> Option<&AgentVariantInfo> {
        self.variants.get(id)
    }

    /// 변형 생성
    pub fn create(&self, id: &str, config: Option<RuntimeConfig>) -> Result<Box<dyn AgentRuntime>> {
        let info = self.variants.get(id).ok_or_else(|| {
            forge_foundation::Error::Config(format!("Unknown agent variant: {}", id))
        })?;

        let factory = self.factories.get(id).ok_or_else(|| {
            forge_foundation::Error::Config(format!("No factory for agent variant: {}", id))
        })?;

        let config = config.unwrap_or_else(|| info.default_config.clone());
        Ok(factory(config))
    }

    /// 모든 변형 ID 조회
    pub fn list_ids(&self) -> Vec<&str> {
        self.variants.keys().map(|s| s.as_str()).collect()
    }

    /// 모든 변형 정보 조회
    pub fn list_all(&self) -> Vec<&AgentVariantInfo> {
        self.variants.values().collect()
    }

    /// 카테고리별 조회
    pub fn list_by_category(&self, category: VariantCategory) -> Vec<&AgentVariantInfo> {
        self.variants
            .values()
            .filter(|v| v.category == category)
            .collect()
    }

    /// 사용 사례로 검색
    pub fn find_by_use_case(&self, use_case: &str) -> Vec<&AgentVariantInfo> {
        let use_case_lower = use_case.to_lowercase();
        self.variants
            .values()
            .filter(|v| {
                v.recommended_for
                    .iter()
                    .any(|u| u.to_lowercase().contains(&use_case_lower))
            })
            .collect()
    }

    /// 변형 제거
    pub fn unregister(&mut self, id: &str) -> bool {
        self.variants.remove(id).is_some() && self.factories.remove(id).is_some()
    }

    /// 변형 존재 여부 확인
    pub fn contains(&self, id: &str) -> bool {
        self.variants.contains_key(id)
    }

    /// 변형 수
    pub fn len(&self) -> usize {
        self.variants.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BuiltinVariant Trait
// ============================================================================

/// 내장 Agent 변형 트레이트
pub trait BuiltinVariant: AgentRuntime + Sized {
    /// 변형 정보 반환
    fn variant_info() -> AgentVariantInfo;

    /// 설정으로 인스턴스 생성
    fn create(config: RuntimeConfig) -> Self;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_basic() {
        let registry = AgentRegistry::new();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_variant_info() {
        let info = AgentVariantInfo::new("test", "Test Agent")
            .with_description("A test agent")
            .with_category(VariantCategory::Experimental)
            .recommended_for("testing");

        assert_eq!(info.id, "test");
        assert_eq!(info.category, VariantCategory::Experimental);
        assert!(!info.recommended_for.is_empty());
    }
}
