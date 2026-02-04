//! Registry Snapshot - 스냅샷 및 롤백 지원
//!
//! Hot-reload 시 안전하게 상태를 저장하고 복원하는 기능 제공

use super::entry::EntryMetadata;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// RegistrySnapshot - 레지스트리 스냅샷
// ============================================================================

/// 레지스트리 스냅샷 - 특정 시점의 상태를 저장
pub struct RegistrySnapshot<T: ?Sized + Send + Sync> {
    /// 스냅샷 ID
    pub id: String,

    /// 스냅샷 생성 시간
    pub created_at: DateTime<Utc>,

    /// 스냅샷 설명 (옵션)
    pub description: Option<String>,

    /// 저장된 항목들
    entries: HashMap<String, SnapshotEntry<T>>,

    /// 카테고리 인덱스
    categories: HashMap<String, Vec<String>>,
}

/// 스냅샷 내 항목
struct SnapshotEntry<T: ?Sized> {
    value: Arc<T>,
    metadata: EntryMetadata,
}

impl<T: ?Sized + Send + Sync + 'static> RegistrySnapshot<T> {
    /// 새 스냅샷 생성 (빈 상태)
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            created_at: Utc::now(),
            description: None,
            entries: HashMap::new(),
            categories: HashMap::new(),
        }
    }

    /// 설명 추가
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// 항목 추가
    pub fn add_entry(&mut self, key: String, value: Arc<T>, metadata: EntryMetadata) {
        let category = metadata.category.clone();

        self.entries.insert(key.clone(), SnapshotEntry { value, metadata });
        self.categories.entry(category).or_default().push(key);
    }

    /// 항목 수
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 모든 키
    pub fn keys(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// 항목 가져오기
    pub fn get(&self, key: &str) -> Option<(Arc<T>, EntryMetadata)> {
        self.entries.get(key).map(|e| (Arc::clone(&e.value), e.metadata.clone()))
    }

    /// 모든 항목 반환 (복원용)
    pub fn into_entries(self) -> impl Iterator<Item = (String, Arc<T>, EntryMetadata)> {
        self.entries.into_iter().map(|(k, e)| (k, e.value, e.metadata))
    }

    /// 스냅샷 정보
    pub fn info(&self) -> SnapshotInfo {
        SnapshotInfo {
            id: self.id.clone(),
            created_at: self.created_at,
            description: self.description.clone(),
            entry_count: self.entries.len(),
            category_count: self.categories.len(),
        }
    }
}

impl<T: ?Sized + Send + Sync> Clone for RegistrySnapshot<T> {
    fn clone(&self) -> Self {
        let mut entries = HashMap::new();
        for (k, e) in &self.entries {
            entries.insert(k.clone(), SnapshotEntry {
                value: Arc::clone(&e.value),
                metadata: e.metadata.clone(),
            });
        }

        Self {
            id: self.id.clone(),
            created_at: self.created_at,
            description: self.description.clone(),
            entries,
            categories: self.categories.clone(),
        }
    }
}

/// 스냅샷 정보 (메타데이터만)
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub description: Option<String>,
    pub entry_count: usize,
    pub category_count: usize,
}

// ============================================================================
// SnapshotManager - 스냅샷 관리자
// ============================================================================

/// 스냅샷 관리자 - 여러 스냅샷을 관리하고 롤백 지원
pub struct SnapshotManager<T: ?Sized + Send + Sync> {
    /// 저장된 스냅샷들
    snapshots: Vec<RegistrySnapshot<T>>,

    /// 최대 스냅샷 수
    max_snapshots: usize,

    /// 자동 스냅샷 활성화
    auto_snapshot: bool,
}

impl<T: ?Sized + Send + Sync + 'static> SnapshotManager<T> {
    /// 새 매니저 생성
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
            max_snapshots: 10,
            auto_snapshot: true,
        }
    }

    /// 최대 스냅샷 수 설정
    pub fn with_max_snapshots(mut self, max: usize) -> Self {
        self.max_snapshots = max;
        self
    }

    /// 자동 스냅샷 설정
    pub fn with_auto_snapshot(mut self, enabled: bool) -> Self {
        self.auto_snapshot = enabled;
        self
    }

    /// 자동 스냅샷 활성화 여부
    pub fn is_auto_snapshot_enabled(&self) -> bool {
        self.auto_snapshot
    }

    /// 스냅샷 저장
    pub fn save(&mut self, snapshot: RegistrySnapshot<T>) {
        // 최대 수 초과 시 가장 오래된 것 제거
        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.remove(0);
        }

        self.snapshots.push(snapshot);
    }

    /// 가장 최근 스냅샷 가져오기
    pub fn latest(&self) -> Option<&RegistrySnapshot<T>> {
        self.snapshots.last()
    }

    /// ID로 스냅샷 가져오기
    pub fn get(&self, id: &str) -> Option<&RegistrySnapshot<T>> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// 스냅샷 목록
    pub fn list(&self) -> Vec<SnapshotInfo> {
        self.snapshots.iter().map(|s| s.info()).collect()
    }

    /// 스냅샷 수
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// 특정 스냅샷 삭제
    pub fn remove(&mut self, id: &str) -> Option<RegistrySnapshot<T>> {
        if let Some(pos) = self.snapshots.iter().position(|s| s.id == id) {
            Some(self.snapshots.remove(pos))
        } else {
            None
        }
    }

    /// 모든 스냅샷 삭제
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    /// 가장 최근 스냅샷으로 롤백 (pop하여 반환)
    pub fn pop_latest(&mut self) -> Option<RegistrySnapshot<T>> {
        self.snapshots.pop()
    }
}

impl<T: ?Sized + Send + Sync + 'static> Default for SnapshotManager<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// HotReloadContext - Hot-reload 컨텍스트
// ============================================================================

/// Hot-reload 작업의 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotReloadState {
    /// 대기 중
    Idle,
    /// 스냅샷 생성 중
    Snapshotting,
    /// 교체 중
    Replacing,
    /// 검증 중
    Validating,
    /// 완료
    Completed,
    /// 롤백 중
    RollingBack,
    /// 실패
    Failed,
}

/// Hot-reload 결과
#[derive(Debug)]
pub struct HotReloadResult {
    /// 성공 여부
    pub success: bool,
    /// 교체된 항목 수
    pub replaced_count: usize,
    /// 추가된 항목 수
    pub added_count: usize,
    /// 제거된 항목 수
    pub removed_count: usize,
    /// 롤백 여부
    pub rolled_back: bool,
    /// 에러 메시지 (실패 시)
    pub error: Option<String>,
    /// 소요 시간 (밀리초)
    pub duration_ms: u64,
}

impl HotReloadResult {
    /// 성공 결과 생성
    pub fn success(replaced: usize, added: usize, removed: usize, duration_ms: u64) -> Self {
        Self {
            success: true,
            replaced_count: replaced,
            added_count: added,
            removed_count: removed,
            rolled_back: false,
            error: None,
            duration_ms,
        }
    }

    /// 롤백 결과 생성
    pub fn rolled_back(error: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            success: false,
            replaced_count: 0,
            added_count: 0,
            removed_count: 0,
            rolled_back: true,
            error: Some(error.into()),
            duration_ms,
        }
    }

    /// 실패 결과 생성
    pub fn failed(error: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            success: false,
            replaced_count: 0,
            added_count: 0,
            removed_count: 0,
            rolled_back: false,
            error: Some(error.into()),
            duration_ms,
        }
    }
}

/// Hot-reload 설정
#[derive(Debug, Clone)]
pub struct HotReloadConfig {
    /// 자동 스냅샷 생성
    pub auto_snapshot: bool,
    /// 검증 활성화
    pub validate: bool,
    /// 실패 시 자동 롤백
    pub auto_rollback: bool,
    /// 타임아웃 (밀리초)
    pub timeout_ms: u64,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            auto_snapshot: true,
            validate: true,
            auto_rollback: true,
            timeout_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_creation() {
        let snapshot: RegistrySnapshot<String> = RegistrySnapshot::new("test-1")
            .with_description("Test snapshot");

        assert_eq!(snapshot.id, "test-1");
        assert_eq!(snapshot.description, Some("Test snapshot".into()));
        assert!(snapshot.is_empty());
    }

    #[test]
    fn test_snapshot_add_entry() {
        let mut snapshot: RegistrySnapshot<String> = RegistrySnapshot::new("test-1");

        let metadata = EntryMetadata::new("key1", "category", "1.0.0");
        snapshot.add_entry("key1".into(), Arc::new("value1".to_string()), metadata);

        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.get("key1").is_some());
    }

    #[test]
    fn test_snapshot_manager() {
        let mut manager: SnapshotManager<String> = SnapshotManager::new()
            .with_max_snapshots(3);

        // 스냅샷 추가
        for i in 0..5 {
            let snapshot = RegistrySnapshot::new(format!("snapshot-{}", i));
            manager.save(snapshot);
        }

        // 최대 3개만 유지
        assert_eq!(manager.len(), 3);

        // 가장 오래된 것이 제거됨
        assert!(manager.get("snapshot-0").is_none());
        assert!(manager.get("snapshot-1").is_none());
        assert!(manager.get("snapshot-2").is_some());
    }

    #[test]
    fn test_hot_reload_result() {
        let result = HotReloadResult::success(5, 2, 1, 100);
        assert!(result.success);
        assert_eq!(result.replaced_count, 5);

        let result = HotReloadResult::rolled_back("Test error", 50);
        assert!(!result.success);
        assert!(result.rolled_back);
    }
}
