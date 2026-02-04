//! Dynamic Registry - 동적으로 변경 가능한 레지스트리

use super::entry::{EntryMetadata, EntryState, RegistryEntry};
use super::snapshot::{HotReloadConfig, HotReloadResult, HotReloadState, RegistrySnapshot, SnapshotManager, SnapshotInfo};
use super::traits::{RegistryEvent, RegistryEventHandler};
use crate::skill::Skill;
use crate::tool::Tool;
use forge_foundation::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

// ============================================================================
// DynamicRegistry<T> - 제네릭 동적 레지스트리
// ============================================================================

/// 동적 레지스트리 - Interior Mutability 패턴으로 Thread-safe한 런타임 변경 지원
pub struct DynamicRegistry<T: ?Sized + Send + Sync> {
    /// 항목 저장소 (RwLock으로 내부 가변성)
    entries: RwLock<HashMap<String, RegistryEntry<T>>>,

    /// 카테고리별 인덱스
    categories: RwLock<HashMap<String, Vec<String>>>,

    /// 이벤트 채널
    event_tx: broadcast::Sender<RegistryEvent>,

    /// 이벤트 핸들러
    handlers: RwLock<Vec<Arc<dyn RegistryEventHandler>>>,

    /// 스냅샷 매니저
    snapshot_manager: RwLock<SnapshotManager<T>>,

    /// Hot-reload 설정
    hot_reload_config: RwLock<HotReloadConfig>,

    /// 현재 Hot-reload 상태
    hot_reload_state: RwLock<HotReloadState>,

    /// 레지스트리 이름 (디버깅용)
    name: String,
}

impl<T: ?Sized + Send + Sync + 'static> DynamicRegistry<T> {
    /// 새 레지스트리 생성
    pub fn new(name: impl Into<String>) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            entries: RwLock::new(HashMap::new()),
            categories: RwLock::new(HashMap::new()),
            event_tx,
            handlers: RwLock::new(Vec::new()),
            snapshot_manager: RwLock::new(SnapshotManager::new()),
            hot_reload_config: RwLock::new(HotReloadConfig::default()),
            hot_reload_state: RwLock::new(HotReloadState::Idle),
            name: name.into(),
        }
    }

    // ========================================================================
    // 등록 / 해제
    // ========================================================================

    /// 항목 등록
    pub async fn register(&self, key: impl Into<String>, value: Arc<T>, metadata: EntryMetadata) -> Result<()> {
        let key = key.into();
        let category = metadata.category.clone();
        let version = metadata.version.clone();
        let provider = metadata.provider.clone();

        let entry = RegistryEntry::new(value, metadata);

        // 저장소에 추가
        {
            let mut entries = self.entries.write().await;
            if entries.contains_key(&key) {
                warn!("[{}] Key '{}' already exists, use replace() instead", self.name, key);
            }
            entries.insert(key.clone(), entry);
        }

        // 카테고리 인덱스 업데이트
        {
            let mut categories = self.categories.write().await;
            categories.entry(category.clone()).or_default().push(key.clone());
        }

        debug!("[{}] Registered: {} (v{})", self.name, key, version);

        // 이벤트 발행
        self.emit_event(RegistryEvent::Registered {
            key,
            category,
            version,
            provider,
        }).await;

        Ok(())
    }

    /// 간단한 등록 (메타데이터 자동 생성)
    pub async fn register_simple(&self, key: impl Into<String>, value: Arc<T>) -> Result<()> {
        let key = key.into();
        let metadata = EntryMetadata::new(&key, "default", "1.0.0");
        self.register(key, value, metadata).await
    }

    /// 항목 등록 해제
    pub async fn unregister(&self, key: &str) -> Option<Arc<T>> {
        let entry = {
            let mut entries = self.entries.write().await;
            entries.remove(key)
        };

        if let Some(ref e) = entry {
            // 카테고리 인덱스 업데이트
            let mut categories = self.categories.write().await;
            if let Some(keys) = categories.get_mut(&e.metadata.category) {
                keys.retain(|k| k != key);
            }

            debug!("[{}] Unregistered: {}", self.name, key);

            // 이벤트 발행
            self.emit_event(RegistryEvent::unregistered(key)).await;

            return Some(Arc::clone(&e.value));
        }

        None
    }

    /// 항목 교체
    pub async fn replace(&self, key: &str, new_value: Arc<T>, new_version: impl Into<String>) -> Option<Arc<T>> {
        let new_version = new_version.into();
        let old_version;

        let old_value = {
            let mut entries = self.entries.write().await;
            if let Some(entry) = entries.get_mut(key) {
                old_version = entry.version().to_string();
                let old = Arc::clone(&entry.value);
                entry.replace(new_value, &new_version);
                Some(old)
            } else {
                return None;
            }
        };

        if old_value.is_some() {
            info!("[{}] Replaced: {} (v{} -> v{})", self.name, key, old_version, new_version);

            // 이벤트 발행
            self.emit_event(RegistryEvent::replaced(key, old_version, new_version)).await;
        }

        old_value
    }

    // ========================================================================
    // 조회
    // ========================================================================

    /// 항목 조회
    pub async fn get(&self, key: &str) -> Option<Arc<T>> {
        let entries = self.entries.read().await;
        entries.get(key).filter(|e| e.is_active()).map(|e| Arc::clone(&e.value))
    }

    /// 항목 조회 (비활성화 포함)
    pub async fn get_any(&self, key: &str) -> Option<Arc<T>> {
        let entries = self.entries.read().await;
        entries.get(key).map(|e| Arc::clone(&e.value))
    }

    /// 항목 메타데이터 조회
    pub async fn get_metadata(&self, key: &str) -> Option<EntryMetadata> {
        let entries = self.entries.read().await;
        entries.get(key).map(|e| e.metadata.clone())
    }

    /// 항목 존재 여부
    pub async fn contains(&self, key: &str) -> bool {
        let entries = self.entries.read().await;
        entries.contains_key(key)
    }

    /// 모든 활성 항목
    pub async fn all(&self) -> Vec<Arc<T>> {
        let entries = self.entries.read().await;
        entries
            .values()
            .filter(|e| e.is_active())
            .map(|e| Arc::clone(&e.value))
            .collect()
    }

    /// 모든 항목 (비활성화 포함)
    pub async fn all_including_inactive(&self) -> Vec<Arc<T>> {
        let entries = self.entries.read().await;
        entries.values().map(|e| Arc::clone(&e.value)).collect()
    }

    /// 모든 키
    pub async fn keys(&self) -> Vec<String> {
        let entries = self.entries.read().await;
        entries.keys().cloned().collect()
    }

    /// 활성 항목 수
    pub async fn len(&self) -> usize {
        let entries = self.entries.read().await;
        entries.values().filter(|e| e.is_active()).count()
    }

    /// 전체 항목 수
    pub async fn total_len(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }

    /// 비어있는지 확인
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    // ========================================================================
    // 카테고리
    // ========================================================================

    /// 카테고리별 항목 조회
    pub async fn by_category(&self, category: &str) -> Vec<Arc<T>> {
        let categories = self.categories.read().await;
        let entries = self.entries.read().await;

        categories
            .get(category)
            .map(|keys| {
                keys.iter()
                    .filter_map(|k| entries.get(k))
                    .filter(|e| e.is_active())
                    .map(|e| Arc::clone(&e.value))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 모든 카테고리
    pub async fn categories(&self) -> Vec<String> {
        let categories = self.categories.read().await;
        categories.keys().cloned().collect()
    }

    // ========================================================================
    // 활성화 / 비활성화
    // ========================================================================

    /// 항목 활성화
    pub async fn enable(&self, key: &str) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            entry.enable();
            self.emit_event(RegistryEvent::Enabled { key: key.into() }).await;
            true
        } else {
            false
        }
    }

    /// 항목 비활성화
    pub async fn disable(&self, key: &str) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            entry.disable();
            self.emit_event(RegistryEvent::Disabled { key: key.into() }).await;
            true
        } else {
            false
        }
    }

    /// 항목 상태 변경
    pub async fn set_state(&self, key: &str, state: EntryState) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            entry.metadata.set_state(state);
            true
        } else {
            false
        }
    }

    // ========================================================================
    // 벌크 연산
    // ========================================================================

    /// 전체 클리어
    pub async fn clear(&self) {
        {
            let mut entries = self.entries.write().await;
            entries.clear();
        }
        {
            let mut categories = self.categories.write().await;
            categories.clear();
        }

        info!("[{}] Cleared all entries", self.name);
        self.emit_event(RegistryEvent::Cleared).await;
    }

    /// 여러 항목 한번에 등록
    pub async fn register_bulk(&self, items: Vec<(String, Arc<T>, EntryMetadata)>) -> Result<()> {
        let mut added = Vec::new();

        for (key, value, metadata) in items {
            let entry = RegistryEntry::new(value, metadata.clone());

            {
                let mut entries = self.entries.write().await;
                entries.insert(key.clone(), entry);
            }
            {
                let mut categories = self.categories.write().await;
                categories.entry(metadata.category).or_default().push(key.clone());
            }

            added.push(key);
        }

        self.emit_event(RegistryEvent::BulkChange {
            added,
            removed: vec![],
            replaced: vec![],
        }).await;

        Ok(())
    }

    // ========================================================================
    // 이벤트
    // ========================================================================

    /// 이벤트 구독
    pub fn subscribe(&self) -> broadcast::Receiver<RegistryEvent> {
        self.event_tx.subscribe()
    }

    /// 이벤트 핸들러 등록
    pub async fn add_handler(&self, handler: Arc<dyn RegistryEventHandler>) {
        let mut handlers = self.handlers.write().await;
        handlers.push(handler);
    }

    /// 이벤트 발행
    async fn emit_event(&self, event: RegistryEvent) {
        // 브로드캐스트 채널로 발행
        let _ = self.event_tx.send(event.clone());

        // 핸들러 호출
        let handlers = self.handlers.read().await;
        for handler in handlers.iter() {
            handler.handle(&event).await;
        }
    }

    // ========================================================================
    // 통계
    // ========================================================================

    /// 레지스트리 통계
    pub async fn stats(&self) -> RegistryStats {
        let entries = self.entries.read().await;
        let categories = self.categories.read().await;

        let active = entries.values().filter(|e| e.is_active()).count();
        let inactive = entries.len() - active;

        RegistryStats {
            name: self.name.clone(),
            total: entries.len(),
            active,
            inactive,
            categories: categories.len(),
        }
    }

    // ========================================================================
    // 스냅샷 / 롤백
    // ========================================================================

    /// 현재 상태의 스냅샷 생성
    pub async fn create_snapshot(&self, id: impl Into<String>) -> RegistrySnapshot<T> {
        let id = id.into();
        let entries = self.entries.read().await;
        let _categories = self.categories.read().await;

        let mut snapshot = RegistrySnapshot::new(&id);

        for (key, entry) in entries.iter() {
            snapshot.add_entry(
                key.clone(),
                Arc::clone(&entry.value),
                entry.metadata.clone(),
            );
        }

        debug!("[{}] Created snapshot '{}' with {} entries", self.name, id, snapshot.len());
        snapshot
    }

    /// 스냅샷 생성 후 저장
    pub async fn save_snapshot(&self, id: impl Into<String>) -> SnapshotInfo {
        let snapshot = self.create_snapshot(id).await;
        let info = snapshot.info();

        let mut manager = self.snapshot_manager.write().await;
        manager.save(snapshot);

        info!("[{}] Saved snapshot: {}", self.name, info.id);
        info
    }

    /// 스냅샷으로 복원
    pub async fn restore_snapshot(&self, snapshot: RegistrySnapshot<T>) -> Result<()> {
        let snapshot_id = snapshot.id.clone();
        let entry_count = snapshot.len();

        info!("[{}] Restoring snapshot '{}' ({} entries)", self.name, snapshot_id, entry_count);

        // 현재 상태 클리어
        {
            let mut entries = self.entries.write().await;
            entries.clear();
        }
        {
            let mut categories = self.categories.write().await;
            categories.clear();
        }

        // 스냅샷에서 복원
        for (key, value, metadata) in snapshot.into_entries() {
            let category = metadata.category.clone();

            {
                let mut entries = self.entries.write().await;
                entries.insert(key.clone(), RegistryEntry::new(value, metadata));
            }
            {
                let mut categories = self.categories.write().await;
                categories.entry(category).or_default().push(key);
            }
        }

        // 이벤트 발행
        self.emit_event(RegistryEvent::BulkChange {
            added: vec![],
            removed: vec![],
            replaced: vec![format!("restored_from_{}", snapshot_id)],
        }).await;

        info!("[{}] Restored from snapshot '{}'", self.name, snapshot_id);
        Ok(())
    }

    /// ID로 스냅샷 찾아서 복원
    pub async fn restore_by_id(&self, snapshot_id: &str) -> Result<()> {
        let manager = self.snapshot_manager.read().await;
        let snapshot = manager.get(snapshot_id).cloned();
        drop(manager);

        if let Some(snapshot) = snapshot {
            self.restore_snapshot(snapshot).await
        } else {
            Err(forge_foundation::Error::NotFound(format!(
                "Snapshot '{}' not found", snapshot_id
            )))
        }
    }

    /// 가장 최근 스냅샷으로 롤백
    pub async fn rollback(&self) -> Result<()> {
        let mut manager = self.snapshot_manager.write().await;
        let snapshot = manager.pop_latest();
        drop(manager);

        if let Some(snapshot) = snapshot {
            info!("[{}] Rolling back to snapshot '{}'", self.name, snapshot.id);
            self.restore_snapshot(snapshot).await
        } else {
            Err(forge_foundation::Error::NotFound(
                "No snapshot available for rollback".into()
            ))
        }
    }

    /// 스냅샷 목록 조회
    pub async fn list_snapshots(&self) -> Vec<SnapshotInfo> {
        let manager = self.snapshot_manager.read().await;
        manager.list()
    }

    // ========================================================================
    // Hot-reload
    // ========================================================================

    /// Hot-reload 설정 변경
    pub async fn configure_hot_reload(&self, config: HotReloadConfig) {
        let mut cfg = self.hot_reload_config.write().await;
        *cfg = config;
    }

    /// Hot-reload 상태 조회
    pub async fn hot_reload_state(&self) -> HotReloadState {
        *self.hot_reload_state.read().await
    }

    /// 안전한 Hot-reload: 새 항목으로 교체하면서 롤백 지원
    ///
    /// 1. 현재 상태 스냅샷
    /// 2. 새 항목들로 교체
    /// 3. 검증 (옵션)
    /// 4. 실패 시 롤백
    pub async fn hot_reload(
        &self,
        new_items: Vec<(String, Arc<T>, EntryMetadata)>,
        validate: Option<Box<dyn Fn(&Self) -> bool + Send + Sync>>,
    ) -> HotReloadResult {
        let start = Instant::now();
        let config = self.hot_reload_config.read().await.clone();

        // 상태 변경: Snapshotting
        *self.hot_reload_state.write().await = HotReloadState::Snapshotting;

        // 1. 스냅샷 생성 (자동 스냅샷이 활성화된 경우)
        if config.auto_snapshot {
            let id = format!("hot_reload_{}", chrono::Utc::now().timestamp_millis());
            let snapshot = self.create_snapshot(&id).await;
            let mut manager = self.snapshot_manager.write().await;
            manager.save(snapshot);
        }

        // 상태 변경: Replacing
        *self.hot_reload_state.write().await = HotReloadState::Replacing;

        // 2. 기존 항목 수집 (통계용)
        let old_keys: Vec<String> = self.keys().await;
        let new_keys: Vec<String> = new_items.iter().map(|(k, _, _)| k.clone()).collect();

        // 3. 클리어 후 새 항목 등록
        self.clear().await;

        let mut added = 0;
        let mut replaced = 0;

        for (key, value, metadata) in new_items {
            let was_existing = old_keys.contains(&key);
            if let Err(e) = self.register(&key, value, metadata).await {
                error!("[{}] Failed to register '{}': {}", self.name, key, e);
                continue;
            }

            if was_existing {
                replaced += 1;
            } else {
                added += 1;
            }
        }

        let removed = old_keys.iter().filter(|k| !new_keys.contains(k)).count();

        // 상태 변경: Validating
        *self.hot_reload_state.write().await = HotReloadState::Validating;

        // 4. 검증 (옵션)
        if config.validate {
            if let Some(ref validator) = validate {
                if !validator(self) {
                    // 검증 실패 - 롤백
                    if config.auto_rollback {
                        *self.hot_reload_state.write().await = HotReloadState::RollingBack;
                        if let Err(e) = self.rollback().await {
                            error!("[{}] Rollback failed: {}", self.name, e);
                            *self.hot_reload_state.write().await = HotReloadState::Failed;
                            return HotReloadResult::failed(
                                format!("Validation failed and rollback also failed: {}", e),
                                start.elapsed().as_millis() as u64,
                            );
                        }
                        *self.hot_reload_state.write().await = HotReloadState::Completed;
                        return HotReloadResult::rolled_back(
                            "Validation failed, rolled back",
                            start.elapsed().as_millis() as u64,
                        );
                    } else {
                        *self.hot_reload_state.write().await = HotReloadState::Failed;
                        return HotReloadResult::failed(
                            "Validation failed",
                            start.elapsed().as_millis() as u64,
                        );
                    }
                }
            }
        }

        // 성공
        *self.hot_reload_state.write().await = HotReloadState::Completed;

        info!(
            "[{}] Hot-reload completed: {} added, {} replaced, {} removed ({}ms)",
            self.name, added, replaced, removed,
            start.elapsed().as_millis()
        );

        HotReloadResult::success(replaced, added, removed, start.elapsed().as_millis() as u64)
    }

    /// 단일 항목 안전 교체 (스냅샷 + 롤백 지원)
    pub async fn safe_replace(&self, key: &str, new_value: Arc<T>, new_version: impl Into<String>) -> Result<Arc<T>> {
        let new_version = new_version.into();
        let config = self.hot_reload_config.read().await.clone();

        // 스냅샷 생성
        if config.auto_snapshot {
            let snapshot_id = format!("replace_{}_{}", key, chrono::Utc::now().timestamp_millis());
            self.save_snapshot(snapshot_id).await;
        }

        // 교체
        match self.replace(key, new_value, &new_version).await {
            Some(old_value) => {
                info!("[{}] Safely replaced '{}' to v{}", self.name, key, new_version);
                Ok(old_value)
            }
            None => Err(forge_foundation::Error::NotFound(format!(
                "Key '{}' not found for replacement", key
            ))),
        }
    }
}

/// 레지스트리 통계
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub name: String,
    pub total: usize,
    pub active: usize,
    pub inactive: usize,
    pub categories: usize,
}

// ============================================================================
// DynamicToolRegistry - Tool 전용 동적 레지스트리
// ============================================================================

/// Tool 전용 동적 레지스트리
pub struct DynamicToolRegistry {
    inner: DynamicRegistry<dyn Tool>,
}

impl DynamicToolRegistry {
    /// 새 레지스트리 생성
    pub fn new() -> Self {
        Self {
            inner: DynamicRegistry::new("tools"),
        }
    }

    /// 빌트인 도구 포함하여 생성
    pub fn with_builtins() -> Self {
        let registry = Self::new();
        // 빌트인 도구 등록은 별도로 수행
        registry
    }

    /// Tool 등록 (Registerable trait 사용)
    pub async fn register(&self, tool: Arc<dyn Tool>) -> Result<()> {
        let meta = tool.meta();
        let key = meta.name.clone();
        let category = meta.category.clone();

        let metadata = EntryMetadata::new(&key, &category, "1.0.0");
        self.inner.register(key, tool, metadata).await
    }

    /// Tool 등록 해제
    pub async fn unregister(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.inner.unregister(name).await
    }

    /// Tool 교체
    pub async fn replace(&self, name: &str, new_tool: Arc<dyn Tool>, version: impl Into<String>) -> Option<Arc<dyn Tool>> {
        self.inner.replace(name, new_tool, version).await
    }

    /// Tool 조회
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.inner.get(name).await
    }

    /// Tool 존재 여부
    pub async fn contains(&self, name: &str) -> bool {
        self.inner.contains(name).await
    }

    /// 모든 Tool
    pub async fn all(&self) -> Vec<Arc<dyn Tool>> {
        self.inner.all().await
    }

    /// Tool 수
    pub async fn len(&self) -> usize {
        self.inner.len().await
    }

    /// 비어있는지 확인
    pub async fn is_empty(&self) -> bool {
        self.inner.is_empty().await
    }

    /// 이벤트 구독
    pub fn subscribe(&self) -> broadcast::Receiver<RegistryEvent> {
        self.inner.subscribe()
    }

    /// 통계
    pub async fn stats(&self) -> RegistryStats {
        self.inner.stats().await
    }
}

impl Default for DynamicToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DynamicSkillRegistry - Skill 전용 동적 레지스트리
// ============================================================================

/// Skill 전용 동적 레지스트리
pub struct DynamicSkillRegistry {
    inner: DynamicRegistry<dyn Skill>,
    /// 명령어 -> 이름 매핑
    command_map: RwLock<HashMap<String, String>>,
}

impl DynamicSkillRegistry {
    /// 새 레지스트리 생성
    pub fn new() -> Self {
        Self {
            inner: DynamicRegistry::new("skills"),
            command_map: RwLock::new(HashMap::new()),
        }
    }

    /// 빌트인 스킬 포함하여 생성
    pub fn with_builtins() -> Self {
        let registry = Self::new();
        // 빌트인 스킬 등록은 별도로 수행
        registry
    }

    /// Skill 등록
    pub async fn register(&self, skill: Arc<dyn Skill>) -> Result<()> {
        let def = skill.definition();
        let key = def.name.clone();
        let command = def.command.clone();
        let category = def.category.clone();

        let metadata = EntryMetadata::new(&key, &category, "1.0.0");
        self.inner.register(&key, skill, metadata).await?;

        // 명령어 매핑 추가
        {
            let mut cmd_map = self.command_map.write().await;
            cmd_map.insert(command, key);
        }

        Ok(())
    }

    /// Skill 등록 해제
    pub async fn unregister(&self, name: &str) -> Option<Arc<dyn Skill>> {
        let skill = self.inner.unregister(name).await?;

        // 명령어 매핑 제거
        let command = skill.definition().command.clone();
        {
            let mut cmd_map = self.command_map.write().await;
            cmd_map.remove(&command);
        }

        Some(skill)
    }

    /// Skill 교체
    pub async fn replace(&self, name: &str, new_skill: Arc<dyn Skill>, version: impl Into<String>) -> Option<Arc<dyn Skill>> {
        self.inner.replace(name, new_skill, version).await
    }

    /// 이름으로 Skill 조회
    pub async fn get_by_name(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.inner.get(name).await
    }

    /// 명령어로 Skill 조회
    pub async fn get_by_command(&self, command: &str) -> Option<Arc<dyn Skill>> {
        let cmd_map = self.command_map.read().await;
        let normalized = if command.starts_with('/') {
            command.to_string()
        } else {
            format!("/{}", command)
        };

        if let Some(name) = cmd_map.get(&normalized) {
            self.inner.get(name).await
        } else {
            None
        }
    }

    /// 입력에서 Skill 찾기
    pub async fn find_for_input(&self, input: &str) -> Option<Arc<dyn Skill>> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let command = trimmed.split_whitespace().next()?;
        self.get_by_command(command).await
    }

    /// 입력이 Skill 명령어인지 확인
    pub async fn is_skill_command(&self, input: &str) -> bool {
        self.find_for_input(input).await.is_some()
    }

    /// 모든 Skill
    pub async fn all(&self) -> Vec<Arc<dyn Skill>> {
        self.inner.all().await
    }

    /// Skill 수
    pub async fn len(&self) -> usize {
        self.inner.len().await
    }

    /// 비어있는지 확인
    pub async fn is_empty(&self) -> bool {
        self.inner.is_empty().await
    }

    /// 이벤트 구독
    pub fn subscribe(&self) -> broadcast::Receiver<RegistryEvent> {
        self.inner.subscribe()
    }

    /// 통계
    pub async fn stats(&self) -> RegistryStats {
        self.inner.stats().await
    }
}

impl Default for DynamicSkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::builtin::CommitSkill;
    use crate::tool::builtin::ReadTool;
    use crate::tool::builtin::WriteTool;

    #[tokio::test]
    async fn test_dynamic_tool_registry() {
        let registry = DynamicToolRegistry::new();

        // 등록
        let tool: Arc<dyn Tool> = Arc::new(ReadTool::new());
        registry.register(tool).await.unwrap();

        assert!(registry.contains("read").await);
        assert!(!registry.is_empty().await);
    }

    #[tokio::test]
    async fn test_dynamic_skill_registry() {
        let registry = DynamicSkillRegistry::new();

        // 등록
        let skill: Arc<dyn Skill> = Arc::new(CommitSkill::new());
        registry.register(skill).await.unwrap();

        assert!(registry.get_by_name("commit").await.is_some());
        assert!(registry.get_by_command("/commit").await.is_some());
        assert!(registry.is_skill_command("/commit -m test").await);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");
        let mut rx = registry.subscribe();

        // 등록 후 이벤트 확인
        let tool: Arc<dyn Tool> = Arc::new(ReadTool::new());
        let metadata = EntryMetadata::new("read", "filesystem", "1.0.0");
        registry.register("read", tool, metadata).await.unwrap();

        // 이벤트 수신
        if let Ok(event) = rx.try_recv() {
            if let RegistryEvent::Registered { key, .. } = event {
                assert_eq!(key, "read");
            }
        }
    }

    #[tokio::test]
    async fn test_snapshot_and_rollback() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");

        // Tool 등록
        let tool1: Arc<dyn Tool> = Arc::new(ReadTool::new());
        let metadata1 = EntryMetadata::new("read", "filesystem", "1.0.0");
        registry.register("read", tool1, metadata1).await.unwrap();

        let tool2: Arc<dyn Tool> = Arc::new(WriteTool::new());
        let metadata2 = EntryMetadata::new("write", "filesystem", "1.0.0");
        registry.register("write", tool2, metadata2).await.unwrap();

        assert_eq!(registry.len().await, 2);

        // 스냅샷 저장
        let snapshot_info = registry.save_snapshot("test-snapshot").await;
        assert_eq!(snapshot_info.entry_count, 2);

        // Tool 추가
        registry.register_simple("extra", Arc::new(ReadTool::new()) as Arc<dyn Tool>).await.unwrap();
        assert_eq!(registry.len().await, 3);

        // 롤백
        registry.rollback().await.unwrap();
        assert_eq!(registry.len().await, 2);
        assert!(registry.contains("read").await);
        assert!(registry.contains("write").await);
        assert!(!registry.contains("extra").await);
    }

    #[tokio::test]
    async fn test_hot_reload() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");

        // 초기 Tool 등록
        let tool1: Arc<dyn Tool> = Arc::new(ReadTool::new());
        let metadata1 = EntryMetadata::new("read", "filesystem", "1.0.0");
        registry.register("read", tool1, metadata1).await.unwrap();

        assert_eq!(registry.len().await, 1);

        // Hot-reload로 새 Tool 집합으로 교체
        let new_items: Vec<(String, Arc<dyn Tool>, EntryMetadata)> = vec![
            (
                "write".into(),
                Arc::new(WriteTool::new()) as Arc<dyn Tool>,
                EntryMetadata::new("write", "filesystem", "1.0.0"),
            ),
        ];

        let result = registry.hot_reload(new_items, None).await;

        assert!(result.success);
        assert_eq!(result.added_count, 1);  // write 추가
        assert_eq!(result.removed_count, 1); // read 제거

        // Hot-reload 후 상태 확인
        assert!(registry.contains("write").await);
        assert!(!registry.contains("read").await);
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn test_hot_reload_with_validation_rollback() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");

        // 초기 Tool 등록
        let tool1: Arc<dyn Tool> = Arc::new(ReadTool::new());
        let metadata1 = EntryMetadata::new("read", "filesystem", "1.0.0");
        registry.register("read", tool1, metadata1).await.unwrap();

        // Hot-reload 설정: 검증 및 자동 롤백 활성화
        registry.configure_hot_reload(HotReloadConfig {
            auto_snapshot: true,
            validate: true,
            auto_rollback: true,
            timeout_ms: 5000,
        }).await;

        // 항상 실패하는 검증 함수
        let validator = Box::new(|_reg: &DynamicRegistry<dyn Tool>| false);

        let new_items: Vec<(String, Arc<dyn Tool>, EntryMetadata)> = vec![
            (
                "write".into(),
                Arc::new(WriteTool::new()) as Arc<dyn Tool>,
                EntryMetadata::new("write", "filesystem", "1.0.0"),
            ),
        ];

        let result = registry.hot_reload(new_items, Some(validator)).await;

        // 검증 실패로 롤백됨
        assert!(!result.success);
        assert!(result.rolled_back);

        // 롤백 후 원래 상태로 복원됨
        assert!(registry.contains("read").await);
        assert!(!registry.contains("write").await);
    }

    #[tokio::test]
    async fn test_replace_and_version_tracking() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");

        // Tool 등록
        let tool1: Arc<dyn Tool> = Arc::new(ReadTool::new());
        let metadata1 = EntryMetadata::new("read", "filesystem", "1.0.0");
        registry.register("read", tool1, metadata1).await.unwrap();

        // 메타데이터 확인
        let meta = registry.get_metadata("read").await.unwrap();
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.replace_count, 0);

        // Tool 교체
        let tool2: Arc<dyn Tool> = Arc::new(ReadTool::new());
        registry.replace("read", tool2, "2.0.0").await;

        // 메타데이터 업데이트 확인
        let meta = registry.get_metadata("read").await.unwrap();
        assert_eq!(meta.version, "2.0.0");
        assert_eq!(meta.replace_count, 1);
    }

    #[tokio::test]
    async fn test_enable_disable() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");

        // Tool 등록
        let tool: Arc<dyn Tool> = Arc::new(ReadTool::new());
        let metadata = EntryMetadata::new("read", "filesystem", "1.0.0");
        registry.register("read", tool, metadata).await.unwrap();

        // 활성 상태 확인
        assert!(registry.get("read").await.is_some());
        assert_eq!(registry.len().await, 1);

        // 비활성화
        registry.disable("read").await;

        // 일반 get은 활성 항목만 반환
        assert!(registry.get("read").await.is_none());
        assert_eq!(registry.len().await, 0);

        // get_any는 비활성 항목도 반환
        assert!(registry.get_any("read").await.is_some());
        assert_eq!(registry.total_len().await, 1);

        // 다시 활성화
        registry.enable("read").await;
        assert!(registry.get("read").await.is_some());
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn test_category_indexing() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");

        // 다양한 카테고리의 Tool 등록
        let tool1: Arc<dyn Tool> = Arc::new(ReadTool::new());
        let meta1 = EntryMetadata::new("read", "filesystem", "1.0.0");
        registry.register("read", tool1, meta1).await.unwrap();

        let tool2: Arc<dyn Tool> = Arc::new(WriteTool::new());
        let meta2 = EntryMetadata::new("write", "filesystem", "1.0.0");
        registry.register("write", tool2, meta2).await.unwrap();

        // 카테고리별 조회
        let filesystem_tools = registry.by_category("filesystem").await;
        assert_eq!(filesystem_tools.len(), 2);

        // 카테고리 목록
        let categories = registry.categories().await;
        assert!(categories.contains(&"filesystem".to_string()));
    }

    #[tokio::test]
    async fn test_snapshot_list() {
        let registry: DynamicRegistry<dyn Tool> = DynamicRegistry::new("test");

        // 여러 스냅샷 저장
        registry.save_snapshot("snapshot-1").await;
        registry.save_snapshot("snapshot-2").await;
        registry.save_snapshot("snapshot-3").await;

        // 스냅샷 목록 조회
        let snapshots = registry.list_snapshots().await;
        assert_eq!(snapshots.len(), 3);
    }
}
