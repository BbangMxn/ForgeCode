//! Registry Entry - 레지스트리 항목 정의

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ============================================================================
// EntryState - 항목 상태
// ============================================================================

/// 레지스트리 항목의 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryState {
    /// 활성화됨
    Active,

    /// 비활성화됨
    Inactive,

    /// 로딩 중
    Loading,

    /// 오류 상태
    Error,

    /// 더 이상 사용되지 않음 (deprecated)
    Deprecated,
}

impl Default for EntryState {
    fn default() -> Self {
        Self::Active
    }
}

impl std::fmt::Display for EntryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Loading => write!(f, "loading"),
            Self::Error => write!(f, "error"),
            Self::Deprecated => write!(f, "deprecated"),
        }
    }
}

// ============================================================================
// EntryMetadata - 항목 메타데이터
// ============================================================================

/// 레지스트리 항목의 메타데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryMetadata {
    /// 고유 키 (이름)
    pub key: String,

    /// 카테고리
    pub category: String,

    /// 버전
    pub version: String,

    /// 제공자 (plugin 이름 등)
    pub provider: Option<String>,

    /// 우선순위
    pub priority: i32,

    /// 등록 시간
    pub registered_at: DateTime<Utc>,

    /// 마지막 업데이트 시간
    pub updated_at: DateTime<Utc>,

    /// 현재 상태
    pub state: EntryState,

    /// 교체 횟수
    pub replace_count: u32,

    /// 태그
    pub tags: Vec<String>,

    /// 추가 속성
    pub attributes: std::collections::HashMap<String, String>,
}

impl EntryMetadata {
    /// 새 메타데이터 생성
    pub fn new(key: impl Into<String>, category: impl Into<String>, version: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            key: key.into(),
            category: category.into(),
            version: version.into(),
            provider: None,
            priority: 0,
            registered_at: now,
            updated_at: now,
            state: EntryState::Active,
            replace_count: 0,
            tags: vec![],
            attributes: std::collections::HashMap::new(),
        }
    }

    /// 제공자 설정
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// 우선순위 설정
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// 태그 추가
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 속성 추가
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// 교체로 인한 업데이트
    pub fn mark_replaced(&mut self, new_version: impl Into<String>) {
        self.version = new_version.into();
        self.updated_at = Utc::now();
        self.replace_count += 1;
    }

    /// 상태 변경
    pub fn set_state(&mut self, state: EntryState) {
        self.state = state;
        self.updated_at = Utc::now();
    }

    /// 활성화 여부
    pub fn is_active(&self) -> bool {
        self.state == EntryState::Active
    }
}

// ============================================================================
// RegistryEntry - 레지스트리 항목
// ============================================================================

/// 레지스트리 항목 - 값과 메타데이터를 함께 보관
pub struct RegistryEntry<T: ?Sized> {
    /// 실제 값
    pub value: Arc<T>,

    /// 메타데이터
    pub metadata: EntryMetadata,
}

impl<T: ?Sized> RegistryEntry<T> {
    /// 새 항목 생성
    pub fn new(value: Arc<T>, metadata: EntryMetadata) -> Self {
        Self { value, metadata }
    }

    /// 값만으로 생성 (기본 메타데이터)
    pub fn from_value(value: Arc<T>, key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            value,
            metadata: EntryMetadata::new(&key, "default", "1.0.0"),
        }
    }

    /// 값 교체
    pub fn replace(&mut self, new_value: Arc<T>, new_version: impl Into<String>) {
        self.value = new_value;
        self.metadata.mark_replaced(new_version);
    }

    /// 활성화
    pub fn enable(&mut self) {
        self.metadata.set_state(EntryState::Active);
    }

    /// 비활성화
    pub fn disable(&mut self) {
        self.metadata.set_state(EntryState::Inactive);
    }

    /// 활성화 여부
    pub fn is_active(&self) -> bool {
        self.metadata.is_active()
    }

    /// 키 반환
    pub fn key(&self) -> &str {
        &self.metadata.key
    }

    /// 버전 반환
    pub fn version(&self) -> &str {
        &self.metadata.version
    }
}

impl<T: ?Sized> Clone for RegistryEntry<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            metadata: self.metadata.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_metadata() {
        let meta = EntryMetadata::new("test", "category", "1.0.0")
            .with_provider("my_plugin")
            .with_priority(10)
            .with_tag("important");

        assert_eq!(meta.key, "test");
        assert_eq!(meta.provider, Some("my_plugin".into()));
        assert_eq!(meta.priority, 10);
        assert!(meta.tags.contains(&"important".to_string()));
    }

    #[test]
    fn test_entry_replace() {
        let mut meta = EntryMetadata::new("test", "category", "1.0.0");
        assert_eq!(meta.replace_count, 0);

        meta.mark_replaced("2.0.0");
        assert_eq!(meta.version, "2.0.0");
        assert_eq!(meta.replace_count, 1);
    }

    #[test]
    fn test_entry_state() {
        let mut meta = EntryMetadata::new("test", "category", "1.0.0");
        assert!(meta.is_active());

        meta.set_state(EntryState::Inactive);
        assert!(!meta.is_active());
    }
}
