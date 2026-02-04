//! Registry Traits - 동적 레지스트리 인터페이스

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

// ============================================================================
// Registerable - 레지스트리에 등록 가능한 항목의 trait
// ============================================================================

/// 레지스트리에 등록 가능한 항목이 구현해야 하는 trait
pub trait Registerable: Send + Sync {
    /// 고유 식별자 (이름)
    fn registry_key(&self) -> String;

    /// 카테고리 (그룹화용)
    fn registry_category(&self) -> String {
        "default".to_string()
    }

    /// 버전 정보
    fn registry_version(&self) -> String {
        "1.0.0".to_string()
    }

    /// 제공자 정보 (plugin 이름 등)
    fn registry_provider(&self) -> Option<String> {
        None
    }

    /// 우선순위 (같은 키의 항목이 여러 개일 때)
    fn registry_priority(&self) -> i32 {
        0
    }

    /// 활성화 상태
    fn is_enabled(&self) -> bool {
        true
    }
}

// ============================================================================
// RegistryEvent - 레지스트리 변경 이벤트
// ============================================================================

/// 레지스트리 변경 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegistryEvent {
    /// 항목 등록됨
    Registered {
        key: String,
        category: String,
        version: String,
        provider: Option<String>,
    },

    /// 항목 등록 해제됨
    Unregistered { key: String, reason: Option<String> },

    /// 항목 교체됨
    Replaced {
        key: String,
        old_version: String,
        new_version: String,
    },

    /// 항목 활성화됨
    Enabled { key: String },

    /// 항목 비활성화됨
    Disabled { key: String },

    /// 전체 초기화
    Cleared,

    /// 벌크 변경 (여러 항목 동시 변경)
    BulkChange {
        added: Vec<String>,
        removed: Vec<String>,
        replaced: Vec<String>,
    },
}

impl RegistryEvent {
    /// 등록 이벤트 생성
    pub fn registered(
        key: impl Into<String>,
        category: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self::Registered {
            key: key.into(),
            category: category.into(),
            version: version.into(),
            provider: None,
        }
    }

    /// 등록 해제 이벤트 생성
    pub fn unregistered(key: impl Into<String>) -> Self {
        Self::Unregistered {
            key: key.into(),
            reason: None,
        }
    }

    /// 교체 이벤트 생성
    pub fn replaced(
        key: impl Into<String>,
        old_version: impl Into<String>,
        new_version: impl Into<String>,
    ) -> Self {
        Self::Replaced {
            key: key.into(),
            old_version: old_version.into(),
            new_version: new_version.into(),
        }
    }

    /// 이벤트 키 반환
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Registered { key, .. } => Some(key),
            Self::Unregistered { key, .. } => Some(key),
            Self::Replaced { key, .. } => Some(key),
            Self::Enabled { key } => Some(key),
            Self::Disabled { key } => Some(key),
            _ => None,
        }
    }
}

// ============================================================================
// RegistryEventHandler - 이벤트 핸들러 trait
// ============================================================================

/// 레지스트리 이벤트 핸들러
#[async_trait]
pub trait RegistryEventHandler: Send + Sync {
    /// 핸들러 이름
    fn name(&self) -> &str;

    /// 이벤트 처리
    async fn handle(&self, event: &RegistryEvent);
}

// ============================================================================
// RegistryCapability - 레지스트리가 지원하는 기능
// ============================================================================

/// 레지스트리 기능 플래그
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RegistryCapability {
    /// 항목 등록
    Register,

    /// 항목 등록 해제
    Unregister,

    /// 항목 교체
    Replace,

    /// 항목 활성화/비활성화
    EnableDisable,

    /// 버전 관리
    Versioning,

    /// 이벤트 발행
    Events,

    /// 벌크 연산
    BulkOperations,

    /// 롤백 지원
    Rollback,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_event() {
        let event = RegistryEvent::registered("test_tool", "filesystem", "1.0.0");
        assert_eq!(event.key(), Some("test_tool"));

        if let RegistryEvent::Registered {
            key,
            category,
            version,
            ..
        } = event
        {
            assert_eq!(key, "test_tool");
            assert_eq!(category, "filesystem");
            assert_eq!(version, "1.0.0");
        }
    }
}
