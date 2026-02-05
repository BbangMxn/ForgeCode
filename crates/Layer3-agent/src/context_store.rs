//! Context Store - 에이전트 간 지식 공유
//!
//! 2025 최신 Deep Agent 아키텍처 패턴:
//! - 에이전트 간 지식 축적 및 재사용
//! - 중복 조사 방지
//! - 필요한 컨텍스트만 주입
//!
//! ## 사용 예시
//! ```text
//! 1. Orchestrator가 컨텍스트 슬롯 생성
//!    → context_refs: [user_model_schema, auth_patterns]
//!
//! 2. Explorer가 조사 후 컨텍스트 채움
//!    → user_model_schema: "User model in /models/user.py..."
//!
//! 3. Coder가 필요한 컨텍스트만 받음
//!    → inject: [user_model_schema]
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 컨텍스트 종류
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContextKind {
    /// 코드 관련 (파일 내용, 함수 시그니처 등)
    Code,
    /// 아키텍처 (시스템 설계, 의존성, 패턴)
    Architecture,
    /// 테스트 (테스트 결과, 커버리지)
    Test,
    /// 버그 (에러 분석, 원인, 재현 단계)
    Bug,
    /// 구현 (변경 사항, 접근 방식, 트레이드오프)
    Implementation,
    /// 검증 (검증 결과)
    Verification,
    /// 계획 (작업 계획, 단계)
    Plan,
    /// 사용자 정의
    Custom(String),
}

/// 저장된 컨텍스트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredContext {
    /// 고유 ID
    pub id: String,
    /// 컨텍스트 종류
    pub kind: ContextKind,
    /// 내용
    pub content: String,
    /// 요약 (짧은 설명)
    pub summary: Option<String>,
    /// 생성자 (에이전트 이름)
    pub created_by: String,
    /// 연관 작업 ID
    pub task_id: Option<String>,
    /// 생성 시간
    pub created_at: DateTime<Utc>,
    /// 마지막 접근 시간
    pub last_accessed: DateTime<Utc>,
    /// 접근 횟수
    pub access_count: u32,
    /// 메타데이터
    pub metadata: HashMap<String, String>,
    /// 관련 파일 경로
    pub related_files: Vec<String>,
    /// 토큰 수 (추정)
    pub estimated_tokens: usize,
}

impl StoredContext {
    pub fn new(id: impl Into<String>, kind: ContextKind, content: impl Into<String>) -> Self {
        let content = content.into();
        let estimated_tokens = content.len() / 4; // 간단한 추정

        Self {
            id: id.into(),
            kind,
            content,
            summary: None,
            created_by: "unknown".to_string(),
            task_id: None,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            metadata: HashMap::new(),
            related_files: Vec::new(),
            estimated_tokens,
        }
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn with_creator(mut self, creator: impl Into<String>) -> Self {
        self.created_by = creator.into();
        self
    }

    pub fn with_task(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.related_files = files;
        self
    }

    pub fn add_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    /// 접근 기록
    pub fn record_access(&mut self) {
        self.last_accessed = Utc::now();
        self.access_count += 1;
    }
}

/// Context Store - 에이전트 간 지식 공유 저장소
#[derive(Debug, Default)]
pub struct ContextStore {
    /// 저장된 컨텍스트들
    contexts: RwLock<HashMap<String, StoredContext>>,
    /// 최대 컨텍스트 수
    max_contexts: usize,
    /// 최대 총 토큰 수
    max_total_tokens: usize,
}

impl ContextStore {
    pub fn new() -> Self {
        Self {
            contexts: RwLock::new(HashMap::new()),
            max_contexts: 100,
            max_total_tokens: 100_000,
        }
    }

    pub fn with_limits(max_contexts: usize, max_total_tokens: usize) -> Self {
        Self {
            contexts: RwLock::new(HashMap::new()),
            max_contexts,
            max_total_tokens,
        }
    }

    /// 컨텍스트 저장
    pub async fn store(&self, context: StoredContext) {
        let mut contexts = self.contexts.write().await;

        // 용량 확인
        if contexts.len() >= self.max_contexts {
            // 가장 오래된 것 제거
            self.evict_oldest(&mut contexts);
        }

        // 토큰 제한 확인
        let total_tokens: usize = contexts.values().map(|c| c.estimated_tokens).sum();
        if total_tokens + context.estimated_tokens > self.max_total_tokens {
            self.evict_by_tokens(&mut contexts, context.estimated_tokens);
        }

        contexts.insert(context.id.clone(), context);
    }

    /// 컨텍스트 조회
    pub async fn get(&self, id: &str) -> Option<StoredContext> {
        let mut contexts = self.contexts.write().await;
        if let Some(ctx) = contexts.get_mut(id) {
            ctx.record_access();
            Some(ctx.clone())
        } else {
            None
        }
    }

    /// 여러 컨텍스트 조회
    pub async fn get_many(&self, ids: &[String]) -> Vec<StoredContext> {
        let mut contexts = self.contexts.write().await;
        ids.iter()
            .filter_map(|id| {
                if let Some(ctx) = contexts.get_mut(id) {
                    ctx.record_access();
                    Some(ctx.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// 종류별 컨텍스트 조회
    pub async fn get_by_kind(&self, kind: &ContextKind) -> Vec<StoredContext> {
        let contexts = self.contexts.read().await;
        contexts
            .values()
            .filter(|c| &c.kind == kind)
            .cloned()
            .collect()
    }

    /// 작업별 컨텍스트 조회
    pub async fn get_by_task(&self, task_id: &str) -> Vec<StoredContext> {
        let contexts = self.contexts.read().await;
        contexts
            .values()
            .filter(|c| c.task_id.as_deref() == Some(task_id))
            .cloned()
            .collect()
    }

    /// 파일 관련 컨텍스트 조회
    pub async fn get_by_file(&self, file_path: &str) -> Vec<StoredContext> {
        let contexts = self.contexts.read().await;
        contexts
            .values()
            .filter(|c| c.related_files.iter().any(|f| f.contains(file_path)))
            .cloned()
            .collect()
    }

    /// 컨텍스트 삭제
    pub async fn remove(&self, id: &str) -> Option<StoredContext> {
        let mut contexts = self.contexts.write().await;
        contexts.remove(id)
    }

    /// 작업 관련 컨텍스트 모두 삭제
    pub async fn remove_by_task(&self, task_id: &str) -> usize {
        let mut contexts = self.contexts.write().await;
        let to_remove: Vec<String> = contexts
            .values()
            .filter(|c| c.task_id.as_deref() == Some(task_id))
            .map(|c| c.id.clone())
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            contexts.remove(&id);
        }
        count
    }

    /// 모든 컨텍스트 목록
    pub async fn list(&self) -> Vec<(String, ContextKind, String)> {
        let contexts = self.contexts.read().await;
        contexts
            .values()
            .map(|c| {
                (
                    c.id.clone(),
                    c.kind.clone(),
                    c.summary.clone().unwrap_or_else(|| {
                        c.content.chars().take(50).collect::<String>() + "..."
                    }),
                )
            })
            .collect()
    }

    /// 컨텍스트 수
    pub async fn len(&self) -> usize {
        let contexts = self.contexts.read().await;
        contexts.len()
    }

    /// 비어있는지
    pub async fn is_empty(&self) -> bool {
        let contexts = self.contexts.read().await;
        contexts.is_empty()
    }

    /// 총 토큰 수
    pub async fn total_tokens(&self) -> usize {
        let contexts = self.contexts.read().await;
        contexts.values().map(|c| c.estimated_tokens).sum()
    }

    /// 클리어
    pub async fn clear(&self) {
        let mut contexts = self.contexts.write().await;
        contexts.clear();
    }

    /// 컨텍스트를 프롬프트 형식으로 포맷
    pub async fn format_for_prompt(&self, ids: &[String]) -> String {
        let contexts = self.get_many(ids).await;
        if contexts.is_empty() {
            return String::new();
        }

        let mut output = String::from("## Available Context\n\n");

        for ctx in contexts {
            output.push_str(&format!("### {} ({})\n", ctx.id, format_kind(&ctx.kind)));
            if let Some(summary) = &ctx.summary {
                output.push_str(&format!("*{}*\n\n", summary));
            }
            output.push_str(&ctx.content);
            output.push_str("\n\n");
        }

        output
    }

    /// 가장 오래된 컨텍스트 제거
    fn evict_oldest(&self, contexts: &mut HashMap<String, StoredContext>) {
        if let Some(oldest) = contexts
            .values()
            .min_by_key(|c| c.last_accessed)
            .map(|c| c.id.clone())
        {
            contexts.remove(&oldest);
        }
    }

    /// 토큰 제한까지 컨텍스트 제거
    fn evict_by_tokens(&self, contexts: &mut HashMap<String, StoredContext>, needed: usize) {
        let mut to_remove = Vec::new();
        let mut freed = 0usize;

        // 접근 횟수가 적은 순으로 정렬
        let mut sorted: Vec<_> = contexts.values().collect();
        sorted.sort_by_key(|c| (c.access_count, c.last_accessed));

        for ctx in sorted {
            if freed >= needed {
                break;
            }
            freed += ctx.estimated_tokens;
            to_remove.push(ctx.id.clone());
        }

        for id in to_remove {
            contexts.remove(&id);
        }
    }
}

/// Arc로 감싼 Context Store
pub type SharedContextStore = Arc<ContextStore>;

/// 새 공유 Context Store 생성
pub fn shared_context_store() -> SharedContextStore {
    Arc::new(ContextStore::new())
}

/// 컨텍스트 종류 포맷
fn format_kind(kind: &ContextKind) -> &str {
    match kind {
        ContextKind::Code => "Code",
        ContextKind::Architecture => "Architecture",
        ContextKind::Test => "Test",
        ContextKind::Bug => "Bug",
        ContextKind::Implementation => "Implementation",
        ContextKind::Verification => "Verification",
        ContextKind::Plan => "Plan",
        ContextKind::Custom(name) => name,
    }
}

/// Context 빌더 - 편리한 컨텍스트 생성
pub struct ContextBuilder {
    id: String,
    kind: ContextKind,
    content: String,
    summary: Option<String>,
    creator: String,
    task_id: Option<String>,
    files: Vec<String>,
    metadata: HashMap<String, String>,
}

impl ContextBuilder {
    pub fn new(id: impl Into<String>, kind: ContextKind) -> Self {
        Self {
            id: id.into(),
            kind,
            content: String::new(),
            summary: None,
            creator: "unknown".to_string(),
            task_id: None,
            files: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = creator.into();
        self
    }

    pub fn task(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.files.push(file.into());
        self
    }

    pub fn files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> StoredContext {
        let mut ctx = StoredContext::new(self.id, self.kind, self.content)
            .with_creator(self.creator)
            .with_files(self.files);

        if let Some(summary) = self.summary {
            ctx = ctx.with_summary(summary);
        }
        if let Some(task_id) = self.task_id {
            ctx = ctx.with_task(task_id);
        }
        for (k, v) in self.metadata {
            ctx.add_metadata(k, v);
        }

        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_store_basic() {
        let store = ContextStore::new();

        let ctx = ContextBuilder::new("auth_patterns", ContextKind::Architecture)
            .content("JWT authentication with HS256...")
            .summary("Authentication patterns in the codebase")
            .creator("explorer")
            .file("/middleware/auth.py")
            .build();

        store.store(ctx).await;

        let retrieved = store.get("auth_patterns").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().created_by, "explorer");
    }

    #[tokio::test]
    async fn test_context_store_by_kind() {
        let store = ContextStore::new();

        store.store(ContextBuilder::new("c1", ContextKind::Code).content("code1").build()).await;
        store.store(ContextBuilder::new("c2", ContextKind::Code).content("code2").build()).await;
        store.store(ContextBuilder::new("t1", ContextKind::Test).content("test1").build()).await;

        let code_contexts = store.get_by_kind(&ContextKind::Code).await;
        assert_eq!(code_contexts.len(), 2);

        let test_contexts = store.get_by_kind(&ContextKind::Test).await;
        assert_eq!(test_contexts.len(), 1);
    }

    #[tokio::test]
    async fn test_context_store_eviction() {
        let store = ContextStore::with_limits(3, 10000);

        store.store(ContextBuilder::new("c1", ContextKind::Code).content("a").build()).await;
        store.store(ContextBuilder::new("c2", ContextKind::Code).content("b").build()).await;
        store.store(ContextBuilder::new("c3", ContextKind::Code).content("c").build()).await;
        
        // Access c2 to make it more recently used
        store.get("c2").await;
        
        // Add c4 - should evict c1 (oldest and least accessed)
        store.store(ContextBuilder::new("c4", ContextKind::Code).content("d").build()).await;

        assert_eq!(store.len().await, 3);
        assert!(store.get("c1").await.is_none()); // c1 should be evicted
    }

    #[tokio::test]
    async fn test_format_for_prompt() {
        let store = ContextStore::new();

        store.store(
            ContextBuilder::new("user_model", ContextKind::Code)
                .content("class User:\n    email: str\n    password_hash: str")
                .summary("User model definition")
                .build()
        ).await;

        let prompt = store.format_for_prompt(&["user_model".to_string()]).await;
        assert!(prompt.contains("## Available Context"));
        assert!(prompt.contains("user_model"));
        assert!(prompt.contains("class User"));
    }
}
