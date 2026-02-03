//! LSP Manager - 경량 LSP 서버 관리
//!
//! 효율성 전략:
//! 1. Lazy Loading: 요청 시에만 LSP 서버 시작
//! 2. 타임아웃 기반 자동 종료: 일정 시간 미사용 시 서버 종료
//! 3. 최소 메모리: 필요할 때만 클라이언트 생성
//! 4. 서버 가용성 캐싱: which 결과 캐시

use super::{LspClient, LspClientState, LspServerConfig};
use forge_foundation::Result;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 클라이언트 자동 종료 타임아웃 (기본 10분)
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// 서버 가용성 체크 캐시 TTL (5분)
const AVAILABILITY_CACHE_TTL: Duration = Duration::from_secs(300);

// ============================================================================
// 클라이언트 래퍼 (마지막 사용 시간 추적)
// ============================================================================

struct ManagedClient {
    client: Arc<LspClient>,
    last_used: RwLock<Instant>,
}

impl ManagedClient {
    fn new(client: LspClient) -> Self {
        Self {
            client: Arc::new(client),
            last_used: RwLock::new(Instant::now()),
        }
    }

    async fn touch(&self) {
        *self.last_used.write().await = Instant::now();
    }

    async fn is_idle(&self, timeout: Duration) -> bool {
        self.last_used.read().await.elapsed() > timeout
    }
}

// ============================================================================
// 서버 가용성 캐시
// ============================================================================

struct AvailabilityCache {
    cache: HashMap<String, (bool, Instant)>,
}

impl AvailabilityCache {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    fn get(&self, command: &str) -> Option<bool> {
        self.cache.get(command).and_then(|(available, checked_at)| {
            if checked_at.elapsed() < AVAILABILITY_CACHE_TTL {
                Some(*available)
            } else {
                None
            }
        })
    }

    fn set(&mut self, command: &str, available: bool) {
        self.cache.insert(command.to_string(), (available, Instant::now()));
    }
}

// ============================================================================
// LSP 매니저
// ============================================================================

/// LSP 관리자 - 언어별 서버 관리
///
/// ## 효율성 특징
/// - **Lazy Start**: `get_or_start()` 호출 시에만 서버 시작
/// - **Auto Shutdown**: 일정 시간 미사용 시 자동 종료
/// - **Availability Cache**: 서버 설치 여부 캐싱
/// - **Minimal Memory**: 사용하지 않는 언어는 메모리 차지 안함
pub struct LspManager {
    /// 언어별 클라이언트
    clients: RwLock<HashMap<String, Arc<ManagedClient>>>,

    /// 서버 설정
    configs: Vec<LspServerConfig>,

    /// LSP 기능 활성화 여부
    enabled: RwLock<bool>,

    /// 유휴 타임아웃
    idle_timeout: Duration,

    /// 서버 가용성 캐시
    availability_cache: RwLock<AvailabilityCache>,
}

impl LspManager {
    /// 기본 설정으로 생성
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            configs: super::types::default_lsp_configs(),
            enabled: RwLock::new(true),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            availability_cache: RwLock::new(AvailabilityCache::new()),
        }
    }

    /// 사용자 설정으로 생성
    pub fn with_configs(configs: Vec<LspServerConfig>) -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            configs,
            enabled: RwLock::new(true),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            availability_cache: RwLock::new(AvailabilityCache::new()),
        }
    }

    /// 유휴 타임아웃 설정
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// LSP 비활성화 상태로 생성 (성능 모드)
    pub fn disabled() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            configs: vec![],
            enabled: RwLock::new(false),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            availability_cache: RwLock::new(AvailabilityCache::new()),
        }
    }

    /// LSP 활성화/비활성화
    pub async fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().await = enabled;
        if !enabled {
            // 비활성화 시 모든 클라이언트 종료
            let _ = self.shutdown_all().await;
        }
    }

    /// 활성화 여부
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    // ========================================================================
    // 클라이언트 관리
    // ========================================================================

    /// 언어에 대한 클라이언트 가져오기 또는 시작 (Lazy Loading)
    ///
    /// 서버가 설치되어 있지 않으면 에러 반환
    pub async fn get_or_start(
        &self,
        language_id: &str,
        root_path: &Path,
    ) -> Result<Arc<LspClient>> {
        // LSP 비활성화 확인
        if !*self.enabled.read().await {
            return Err(forge_foundation::Error::Internal("LSP is disabled".to_string()));
        }

        // 이미 실행 중인 클라이언트 확인
        if let Some(managed) = self.clients.read().await.get(language_id) {
            let state = managed.client.state().await;
            if state == LspClientState::Ready {
                managed.touch().await;
                return Ok(Arc::clone(&managed.client));
            }
        }

        // 설정 찾기
        let config = self
            .configs
            .iter()
            .find(|c| c.language_id == language_id)
            .ok_or_else(|| {
                forge_foundation::Error::NotFound(format!(
                    "No LSP config for language: {}",
                    language_id
                ))
            })?
            .clone();

        // 서버 가용성 확인 (캐시 사용)
        if !self.is_server_available(&config.command).await {
            return Err(forge_foundation::Error::NotFound(format!(
                "LSP server not installed: {}",
                config.command
            )));
        }

        // 새 클라이언트 생성 및 시작
        let client = LspClient::new(config);
        client.start(root_path).await?;

        let managed = Arc::new(ManagedClient::new(client));
        let client_arc = Arc::clone(&managed.client);

        self.clients
            .write()
            .await
            .insert(language_id.to_string(), managed);

        info!("LSP server started for language: {}", language_id);

        Ok(client_arc)
    }

    /// 언어 클라이언트 가져오기 (시작하지 않음)
    pub async fn get(&self, language_id: &str) -> Option<Arc<LspClient>> {
        let clients = self.clients.read().await;
        clients.get(language_id).map(|m| {
            // tokio runtime 없이 동기적으로 touch 불가
            // 실제 사용 시 touch 호출 필요
            Arc::clone(&m.client)
        })
    }

    /// 파일에 대한 클라이언트 가져오기 (언어 자동 감지)
    pub async fn get_for_file(&self, file_path: &Path) -> Result<Arc<LspClient>> {
        let language = self.detect_language(file_path).ok_or_else(|| {
            forge_foundation::Error::NotFound(format!(
                "Unknown language for file: {}",
                file_path.display()
            ))
        })?;

        // 프로젝트 루트 찾기
        let root = self.find_project_root(file_path, &language).await;

        self.get_or_start(&language, &root).await
    }

    /// 모든 클라이언트 종료
    pub async fn shutdown_all(&self) -> Result<()> {
        let mut clients = self.clients.write().await;

        for (lang, managed) in clients.drain() {
            if let Err(e) = managed.client.shutdown().await {
                warn!("Failed to shutdown LSP for {}: {}", lang, e);
            }
        }

        debug!("All LSP servers shutdown");
        Ok(())
    }

    /// 특정 언어 클라이언트 종료
    pub async fn shutdown(&self, language_id: &str) -> Result<()> {
        if let Some(managed) = self.clients.write().await.remove(language_id) {
            managed.client.shutdown().await?;
            debug!("LSP server shutdown for language: {}", language_id);
        }
        Ok(())
    }

    /// 유휴 클라이언트 정리 (백그라운드 태스크에서 주기적 호출)
    pub async fn cleanup_idle(&self) {
        let mut to_remove = Vec::new();

        {
            let clients = self.clients.read().await;
            for (lang, managed) in clients.iter() {
                if managed.is_idle(self.idle_timeout).await {
                    to_remove.push(lang.clone());
                }
            }
        }

        for lang in to_remove {
            if let Err(e) = self.shutdown(&lang).await {
                warn!("Failed to cleanup idle LSP for {}: {}", lang, e);
            } else {
                info!("Cleaned up idle LSP server for: {}", lang);
            }
        }
    }

    // ========================================================================
    // 언어 감지
    // ========================================================================

    /// 파일 확장자로 언어 감지
    pub fn detect_language(&self, file_path: &Path) -> Option<String> {
        let extension = file_path.extension()?.to_str()?;

        match extension.to_lowercase().as_str() {
            // Rust
            "rs" => Some("rust".to_string()),

            // TypeScript/JavaScript
            "ts" | "tsx" | "mts" | "cts" => Some("typescript".to_string()),
            "js" | "jsx" | "mjs" | "cjs" => Some("javascript".to_string()),

            // Python
            "py" | "pyw" | "pyi" => Some("python".to_string()),

            // Go
            "go" => Some("go".to_string()),

            // Java
            "java" => Some("java".to_string()),

            // C/C++
            "c" | "h" => Some("c".to_string()),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some("cpp".to_string()),

            // C#
            "cs" => Some("csharp".to_string()),

            // Ruby
            "rb" | "rake" | "gemspec" => Some("ruby".to_string()),

            // PHP
            "php" => Some("php".to_string()),

            // Swift
            "swift" => Some("swift".to_string()),

            // Kotlin
            "kt" | "kts" => Some("kotlin".to_string()),

            // Zig
            "zig" => Some("zig".to_string()),

            _ => None,
        }
    }

    /// 지원 언어 목록
    pub fn supported_languages(&self) -> Vec<&str> {
        self.configs.iter().map(|c| c.language_id.as_str()).collect()
    }

    /// 설정 추가
    pub fn add_config(&mut self, config: LspServerConfig) {
        // 기존 설정 교체
        self.configs.retain(|c| c.language_id != config.language_id);
        self.configs.push(config);
    }

    // ========================================================================
    // 내부 메서드
    // ========================================================================

    /// 서버 설치 여부 확인 (캐시 사용)
    async fn is_server_available(&self, command: &str) -> bool {
        // 캐시 확인
        if let Some(available) = self.availability_cache.read().await.get(command) {
            return available;
        }

        // which로 확인
        let available = which::which(command).is_ok();

        // 캐시 저장
        self.availability_cache.write().await.set(command, available);

        available
    }

    /// 프로젝트 루트 찾기
    async fn find_project_root(&self, file_path: &Path, language_id: &str) -> std::path::PathBuf {
        // 설정에서 root_patterns 가져오기
        let patterns: Vec<&str> = self
            .configs
            .iter()
            .find(|c| c.language_id == language_id)
            .map(|c| c.root_patterns.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();

        // 상위 디렉토리 탐색
        let mut current = file_path.parent();
        while let Some(dir) = current {
            for pattern in &patterns {
                if dir.join(pattern).exists() {
                    return dir.to_path_buf();
                }
            }
            current = dir.parent();
        }

        // 찾지 못하면 파일의 부모 디렉토리 반환
        file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    }
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_new() {
        let manager = LspManager::new();
        assert!(manager.supported_languages().contains(&"rust"));
        assert!(manager.supported_languages().contains(&"typescript"));
    }

    #[test]
    fn test_detect_language() {
        let manager = LspManager::new();

        assert_eq!(
            manager.detect_language(Path::new("main.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            manager.detect_language(Path::new("app.ts")),
            Some("typescript".to_string())
        );
        assert_eq!(
            manager.detect_language(Path::new("script.py")),
            Some("python".to_string())
        );
        assert_eq!(
            manager.detect_language(Path::new("main.go")),
            Some("go".to_string())
        );
        assert_eq!(manager.detect_language(Path::new("unknown.xyz")), None);
    }

    #[test]
    fn test_disabled_manager() {
        let manager = LspManager::disabled();
        assert!(manager.configs.is_empty());
    }

    #[tokio::test]
    async fn test_manager_enabled() {
        let manager = LspManager::new();
        assert!(manager.is_enabled().await);

        manager.set_enabled(false).await;
        assert!(!manager.is_enabled().await);
    }
}
