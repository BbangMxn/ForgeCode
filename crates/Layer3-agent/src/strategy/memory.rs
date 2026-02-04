//! Memory Strategies
//!
//! 다양한 메모리/컨텍스트 관리 전략 구현입니다.
//!
//! - Sliding Window: 최근 N개 메시지만 유지
//! - Summarizing: 오래된 내용 요약
//! - RAG: 검색 증강 메모리

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Memory Types
// ============================================================================

/// 메모리 엔트리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// 엔트리 ID
    pub id: String,

    /// 내용
    pub content: String,

    /// 타입
    pub entry_type: MemoryEntryType,

    /// 중요도 (0.0 ~ 1.0)
    pub importance: f32,

    /// 생성 시간
    pub created_at: DateTime<Utc>,

    /// 마지막 접근 시간
    pub last_accessed: DateTime<Utc>,

    /// 접근 횟수
    pub access_count: u32,

    /// 메타데이터
    pub metadata: HashMap<String, String>,

    /// 임베딩 (RAG용)
    pub embedding: Option<Vec<f32>>,
}

/// 메모리 엔트리 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryEntryType {
    /// 사용자 메시지
    UserMessage,
    /// 어시스턴트 응답
    AssistantResponse,
    /// Tool 결과
    ToolResult,
    /// 요약
    Summary,
    /// 사실/지식
    Fact,
    /// 결정
    Decision,
    /// 코드 스니펫
    CodeSnippet,
}

impl MemoryEntry {
    /// 새 엔트리 생성
    pub fn new(content: impl Into<String>, entry_type: MemoryEntryType) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            entry_type,
            importance: 0.5,
            created_at: now,
            last_accessed: now,
            access_count: 0,
            metadata: HashMap::new(),
            embedding: None,
        }
    }

    /// 중요도 설정
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// 메타데이터 추가
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// 접근 기록
    pub fn mark_accessed(&mut self) {
        self.last_accessed = Utc::now();
        self.access_count += 1;
    }

    /// 토큰 수 추정 (단순 계산)
    pub fn estimated_tokens(&self) -> usize {
        self.content.len() / 4 // 대략적인 추정
    }
}

/// 메모리 쿼리
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    /// 쿼리 텍스트
    pub query: String,

    /// 필터: 엔트리 타입
    pub entry_types: Option<Vec<MemoryEntryType>>,

    /// 필터: 최소 중요도
    pub min_importance: Option<f32>,

    /// 필터: 시작 시간
    pub after: Option<DateTime<Utc>>,

    /// 필터: 종료 시간
    pub before: Option<DateTime<Utc>>,

    /// 최대 결과 수
    pub limit: usize,
}

impl MemoryQuery {
    /// 새 쿼리 생성
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            entry_types: None,
            min_importance: None,
            after: None,
            before: None,
            limit: 10,
        }
    }

    /// 엔트리 타입 필터
    pub fn with_types(mut self, types: Vec<MemoryEntryType>) -> Self {
        self.entry_types = Some(types);
        self
    }

    /// 최소 중요도 필터
    pub fn with_min_importance(mut self, importance: f32) -> Self {
        self.min_importance = Some(importance);
        self
    }

    /// 결과 제한
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// 메모리 검색 결과
#[derive(Debug, Clone)]
pub struct MemoryResult {
    /// 찾은 엔트리들
    pub entries: Vec<MemoryEntry>,

    /// 관련도 점수 (각 엔트리별)
    pub relevance_scores: Vec<f32>,

    /// 총 메모리 크기 (토큰)
    pub total_tokens: usize,
}

// ============================================================================
// MemoryStrategy - 메모리 전략 트레이트
// ============================================================================

/// 메모리 전략 트레이트
#[async_trait]
pub trait MemoryStrategy: Send + Sync {
    /// 전략 이름
    fn name(&self) -> &str;

    /// 전략 설명
    fn description(&self) -> &str;

    /// 엔트리 추가
    async fn add(&mut self, entry: MemoryEntry) -> Result<()>;

    /// 쿼리로 검색
    async fn query(&self, query: &MemoryQuery) -> Result<MemoryResult>;

    /// 최근 N개 가져오기
    async fn recent(&self, n: usize) -> Result<Vec<MemoryEntry>>;

    /// 현재 토큰 수
    fn current_tokens(&self) -> usize;

    /// 최대 토큰 수
    fn max_tokens(&self) -> usize;

    /// 압축/정리 실행
    async fn compact(&mut self) -> Result<()>;

    /// 메모리 클리어
    async fn clear(&mut self) -> Result<()>;

    /// 스냅샷 생성
    fn snapshot(&self) -> Vec<MemoryEntry>;

    /// 스냅샷에서 복원
    async fn restore(&mut self, entries: Vec<MemoryEntry>) -> Result<()>;
}

// ============================================================================
// SlidingWindowMemory - 슬라이딩 윈도우 메모리
// ============================================================================

/// 슬라이딩 윈도우 메모리
///
/// 최근 N개 토큰만 유지합니다.
pub struct SlidingWindowMemory {
    entries: Vec<MemoryEntry>,
    max_tokens: usize,
    current_tokens: usize,
}

impl SlidingWindowMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_tokens,
            current_tokens: 0,
        }
    }

    /// 오래된 엔트리 제거
    fn trim(&mut self) {
        while self.current_tokens > self.max_tokens && !self.entries.is_empty() {
            let removed = self.entries.remove(0);
            self.current_tokens = self
                .current_tokens
                .saturating_sub(removed.estimated_tokens());
        }
    }
}

#[async_trait]
impl MemoryStrategy for SlidingWindowMemory {
    fn name(&self) -> &str {
        "sliding-window"
    }

    fn description(&self) -> &str {
        "Keeps only the most recent entries within token limit"
    }

    async fn add(&mut self, entry: MemoryEntry) -> Result<()> {
        self.current_tokens += entry.estimated_tokens();
        self.entries.push(entry);
        self.trim();
        Ok(())
    }

    async fn query(&self, query: &MemoryQuery) -> Result<MemoryResult> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                // 타입 필터
                if let Some(ref types) = query.entry_types {
                    if !types.contains(&e.entry_type) {
                        return false;
                    }
                }
                // 중요도 필터
                if let Some(min) = query.min_importance {
                    if e.importance < min {
                        return false;
                    }
                }
                // 시간 필터
                if let Some(after) = query.after {
                    if e.created_at < after {
                        return false;
                    }
                }
                if let Some(before) = query.before {
                    if e.created_at > before {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        entries.truncate(query.limit);

        let relevance_scores = vec![1.0; entries.len()]; // 단순 구현
        let total_tokens = entries.iter().map(|e| e.estimated_tokens()).sum();

        Ok(MemoryResult {
            entries,
            relevance_scores,
            total_tokens,
        })
    }

    async fn recent(&self, n: usize) -> Result<Vec<MemoryEntry>> {
        let start = self.entries.len().saturating_sub(n);
        Ok(self.entries[start..].to_vec())
    }

    fn current_tokens(&self) -> usize {
        self.current_tokens
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    async fn compact(&mut self) -> Result<()> {
        self.trim();
        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.current_tokens = 0;
        Ok(())
    }

    fn snapshot(&self) -> Vec<MemoryEntry> {
        self.entries.clone()
    }

    async fn restore(&mut self, entries: Vec<MemoryEntry>) -> Result<()> {
        self.entries = entries;
        self.current_tokens = self.entries.iter().map(|e| e.estimated_tokens()).sum();
        Ok(())
    }
}

// ============================================================================
// SummarizingMemory - 요약 기반 메모리
// ============================================================================

/// 요약 기반 메모리
///
/// 오래된 내용을 요약하여 압축합니다.
pub struct SummarizingMemory {
    entries: Vec<MemoryEntry>,
    summaries: Vec<MemoryEntry>,
    max_tokens: usize,
    current_tokens: usize,
    summarize_threshold: f32, // 이 비율 이상 차면 요약
}

impl SummarizingMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            entries: Vec::new(),
            summaries: Vec::new(),
            max_tokens,
            current_tokens: 0,
            summarize_threshold: 0.8,
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.summarize_threshold = threshold.clamp(0.5, 0.95);
        self
    }

    /// 요약 필요 여부 확인
    fn needs_summarization(&self) -> bool {
        let usage = self.current_tokens as f32 / self.max_tokens as f32;
        usage >= self.summarize_threshold
    }

    /// 요약 생성 (실제로는 LLM 호출 필요)
    async fn create_summary(&self, entries: &[MemoryEntry]) -> MemoryEntry {
        let content = entries
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>()
            .join("\n---\n");

        // 실제 구현에서는 LLM으로 요약
        let summary_content = format!(
            "[Summary of {} entries]: {}",
            entries.len(),
            content.chars().take(500).collect::<String>()
        );

        MemoryEntry::new(summary_content, MemoryEntryType::Summary).with_importance(0.7)
    }
}

#[async_trait]
impl MemoryStrategy for SummarizingMemory {
    fn name(&self) -> &str {
        "summarizing"
    }

    fn description(&self) -> &str {
        "Summarizes older entries to compress memory"
    }

    async fn add(&mut self, entry: MemoryEntry) -> Result<()> {
        self.current_tokens += entry.estimated_tokens();
        self.entries.push(entry);

        // 필요시 요약
        if self.needs_summarization() {
            self.compact().await?;
        }

        Ok(())
    }

    async fn query(&self, query: &MemoryQuery) -> Result<MemoryResult> {
        let mut all_entries: Vec<_> = self
            .summaries
            .iter()
            .chain(self.entries.iter())
            .filter(|e| {
                if let Some(ref types) = query.entry_types {
                    if !types.contains(&e.entry_type) {
                        return false;
                    }
                }
                if let Some(min) = query.min_importance {
                    if e.importance < min {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        all_entries.truncate(query.limit);

        let relevance_scores = vec![1.0; all_entries.len()];
        let total_tokens = all_entries.iter().map(|e| e.estimated_tokens()).sum();

        Ok(MemoryResult {
            entries: all_entries,
            relevance_scores,
            total_tokens,
        })
    }

    async fn recent(&self, n: usize) -> Result<Vec<MemoryEntry>> {
        let start = self.entries.len().saturating_sub(n);
        Ok(self.entries[start..].to_vec())
    }

    fn current_tokens(&self) -> usize {
        self.current_tokens
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    async fn compact(&mut self) -> Result<()> {
        if self.entries.len() < 10 {
            return Ok(());
        }

        // 오래된 절반을 요약
        let split_point = self.entries.len() / 2;
        let to_summarize: Vec<_> = self.entries.drain(..split_point).collect();

        let summary = self.create_summary(&to_summarize).await;
        let summary_tokens = summary.estimated_tokens();
        let removed_tokens: usize = to_summarize.iter().map(|e| e.estimated_tokens()).sum();

        self.summaries.push(summary);
        self.current_tokens = self.current_tokens.saturating_sub(removed_tokens) + summary_tokens;

        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.summaries.clear();
        self.current_tokens = 0;
        Ok(())
    }

    fn snapshot(&self) -> Vec<MemoryEntry> {
        self.summaries
            .iter()
            .chain(self.entries.iter())
            .cloned()
            .collect()
    }

    async fn restore(&mut self, entries: Vec<MemoryEntry>) -> Result<()> {
        self.entries.clear();
        self.summaries.clear();

        for entry in entries {
            if entry.entry_type == MemoryEntryType::Summary {
                self.summaries.push(entry);
            } else {
                self.entries.push(entry);
            }
        }

        self.current_tokens = self
            .summaries
            .iter()
            .chain(self.entries.iter())
            .map(|e| e.estimated_tokens())
            .sum();

        Ok(())
    }
}

// ============================================================================
// RAGMemory - 검색 증강 메모리
// ============================================================================

/// RAG 메모리
///
/// 임베딩 기반 유사도 검색을 사용합니다.
pub struct RAGMemory {
    entries: Vec<MemoryEntry>,
    max_tokens: usize,
    current_tokens: usize,
    /// 임베딩 함수 (실제로는 외부 서비스 호출)
    embedding_dimension: usize,
}

impl RAGMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_tokens,
            current_tokens: 0,
            embedding_dimension: 1536, // OpenAI ada-002 dimension
        }
    }

    /// 간단한 코사인 유사도 계산
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    /// 임베딩 생성 (실제로는 외부 API 호출 필요)
    async fn create_embedding(&self, _text: &str) -> Vec<f32> {
        // 실제 구현에서는 OpenAI 등의 임베딩 API 호출
        vec![0.0; self.embedding_dimension]
    }
}

#[async_trait]
impl MemoryStrategy for RAGMemory {
    fn name(&self) -> &str {
        "rag"
    }

    fn description(&self) -> &str {
        "Retrieval-Augmented Generation memory with embedding-based search"
    }

    async fn add(&mut self, mut entry: MemoryEntry) -> Result<()> {
        // 임베딩 생성
        entry.embedding = Some(self.create_embedding(&entry.content).await);

        self.current_tokens += entry.estimated_tokens();
        self.entries.push(entry);

        // 초과시 중요도 낮은 것부터 제거
        while self.current_tokens > self.max_tokens {
            if let Some(idx) = self
                .entries
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.importance.partial_cmp(&b.importance).unwrap())
                .map(|(i, _)| i)
            {
                let removed = self.entries.remove(idx);
                self.current_tokens = self
                    .current_tokens
                    .saturating_sub(removed.estimated_tokens());
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn query(&self, query: &MemoryQuery) -> Result<MemoryResult> {
        let query_embedding = self.create_embedding(&query.query).await;

        let mut scored: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                if let Some(ref types) = query.entry_types {
                    if !types.contains(&e.entry_type) {
                        return false;
                    }
                }
                if let Some(min) = query.min_importance {
                    if e.importance < min {
                        return false;
                    }
                }
                true
            })
            .map(|e| {
                let score = if let Some(ref emb) = e.embedding {
                    Self::cosine_similarity(&query_embedding, emb)
                } else {
                    0.0
                };
                (e.clone(), score)
            })
            .collect();

        // 유사도 순 정렬
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(query.limit);

        let entries: Vec<_> = scored.iter().map(|(e, _)| e.clone()).collect();
        let relevance_scores: Vec<_> = scored.iter().map(|(_, s)| *s).collect();
        let total_tokens = entries.iter().map(|e| e.estimated_tokens()).sum();

        Ok(MemoryResult {
            entries,
            relevance_scores,
            total_tokens,
        })
    }

    async fn recent(&self, n: usize) -> Result<Vec<MemoryEntry>> {
        let mut sorted: Vec<_> = self.entries.clone();
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sorted.truncate(n);
        Ok(sorted)
    }

    fn current_tokens(&self) -> usize {
        self.current_tokens
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    async fn compact(&mut self) -> Result<()> {
        // 접근 빈도와 중요도 기반으로 정리
        self.entries.sort_by(|a, b| {
            let score_a = a.importance * 0.5 + (a.access_count as f32).min(10.0) / 10.0 * 0.5;
            let score_b = b.importance * 0.5 + (b.access_count as f32).min(10.0) / 10.0 * 0.5;
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 상위 80%만 유지
        let keep_count = (self.entries.len() as f32 * 0.8) as usize;
        self.entries.truncate(keep_count);
        self.current_tokens = self.entries.iter().map(|e| e.estimated_tokens()).sum();

        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.current_tokens = 0;
        Ok(())
    }

    fn snapshot(&self) -> Vec<MemoryEntry> {
        self.entries.clone()
    }

    async fn restore(&mut self, entries: Vec<MemoryEntry>) -> Result<()> {
        self.entries = entries;
        self.current_tokens = self.entries.iter().map(|e| e.estimated_tokens()).sum();
        Ok(())
    }
}

// ============================================================================
// 유틸리티
// ============================================================================

/// 메모리 전략 팩토리
pub fn create_memory_strategy(name: &str, max_tokens: usize) -> Box<dyn MemoryStrategy> {
    match name {
        "sliding-window" | "sliding" => Box::new(SlidingWindowMemory::new(max_tokens)),
        "summarizing" | "summary" => Box::new(SummarizingMemory::new(max_tokens)),
        "rag" => Box::new(RAGMemory::new(max_tokens)),
        _ => Box::new(SlidingWindowMemory::new(max_tokens)),
    }
}
