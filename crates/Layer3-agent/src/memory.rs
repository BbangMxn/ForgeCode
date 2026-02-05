//! Agent Memory System
//!
//! 연구 기반: AI Agentic Programming Survey (2025)
//! - SWE-agent: Vector DB retrieval for tool outputs
//! - OpenDevin: RAG over command history
//! - Cursor IDE: Semantic search over project history
//!
//! ## 아키텍처
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    AgentMemory                          │
//! ├─────────────────────────────────────────────────────────┤
//! │  Short-term Memory (Session)                            │
//! │  ├── Recent tool results                                │
//! │  ├── Current task context                               │
//! │  └── Immediate working memory                           │
//! ├─────────────────────────────────────────────────────────┤
//! │  Long-term Memory (Persistent)                          │
//! │  ├── Successful patterns                                │
//! │  ├── Error resolutions                                  │
//! │  └── Project-specific knowledge                         │
//! ├─────────────────────────────────────────────────────────┤
//! │  Semantic Search (RAG)                                  │
//! │  ├── Keyword matching                                   │
//! │  └── TF-IDF based retrieval                             │
//! └─────────────────────────────────────────────────────────┘
//! ```

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// 메모리 항목
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub id: String,
    pub entry_type: MemoryType,
    pub content: String,
    pub metadata: MemoryMetadata,
    pub created_at: Instant,
    pub access_count: usize,
    pub last_accessed: Instant,
}

/// 메모리 유형
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryType {
    /// 도구 실행 결과
    ToolResult,
    /// 파일 내용
    FileContent,
    /// 에러 및 해결 방법
    ErrorResolution,
    /// 성공 패턴
    SuccessPattern,
    /// 사용자 선호도
    UserPreference,
    /// 프로젝트 컨텍스트
    ProjectContext,
}

/// 메모리 메타데이터
#[derive(Debug, Clone, Default)]
pub struct MemoryMetadata {
    pub tool_name: Option<String>,
    pub file_path: Option<String>,
    pub tags: Vec<String>,
    pub importance: f32,
}

impl MemoryEntry {
    pub fn new(
        entry_type: MemoryType,
        content: impl Into<String>,
        metadata: MemoryMetadata,
    ) -> Self {
        let now = Instant::now();
        Self {
            id: uuid_v4(),
            entry_type,
            content: content.into(),
            metadata,
            created_at: now,
            access_count: 0,
            last_accessed: now,
        }
    }

    /// 도구 결과 메모리 생성
    pub fn tool_result(tool_name: &str, input: &str, output: &str, success: bool) -> Self {
        let content = format!(
            "Tool: {}\nInput: {}\nOutput: {}\nSuccess: {}",
            tool_name, input, output, success
        );
        let tags = if success {
            vec!["success".to_string()]
        } else {
            vec!["failure".to_string()]
        };
        
        Self::new(
            MemoryType::ToolResult,
            content,
            MemoryMetadata {
                tool_name: Some(tool_name.to_string()),
                tags,
                importance: if success { 0.5 } else { 0.8 },
                ..Default::default()
            },
        )
    }

    /// 파일 내용 메모리 생성
    pub fn file_content(path: &str, content: &str) -> Self {
        Self::new(
            MemoryType::FileContent,
            content,
            MemoryMetadata {
                file_path: Some(path.to_string()),
                ..Default::default()
            },
        )
    }

    /// 에러 해결 패턴 생성
    pub fn error_resolution(error: &str, resolution: &str) -> Self {
        let content = format!("Error: {}\nResolution: {}", error, resolution);
        Self::new(
            MemoryType::ErrorResolution,
            content,
            MemoryMetadata {
                tags: vec!["error".to_string(), "resolved".to_string()],
                importance: 0.9,
                ..Default::default()
            },
        )
    }

    /// 접근 기록 업데이트
    pub fn touch(&mut self) {
        self.access_count += 1;
        self.last_accessed = Instant::now();
    }
}

/// 단기 메모리 (세션 내)
#[derive(Debug)]
pub struct ShortTermMemory {
    entries: VecDeque<MemoryEntry>,
    max_entries: usize,
    max_age: Duration,
}

impl Default for ShortTermMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortTermMemory {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(100),
            max_entries: 100,
            max_age: Duration::from_secs(3600), // 1시간
        }
    }

    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
            max_age: Duration::from_secs(3600),
        }
    }

    /// 메모리 추가
    pub fn add(&mut self, entry: MemoryEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// 만료된 항목 정리
    pub fn cleanup(&mut self) {
        let now = Instant::now();
        self.entries.retain(|e| now.duration_since(e.created_at) < self.max_age);
    }

    /// 최근 N개 가져오기
    pub fn recent(&self, n: usize) -> Vec<&MemoryEntry> {
        self.entries.iter().rev().take(n).collect()
    }

    /// 유형별 검색
    pub fn by_type(&self, entry_type: MemoryType) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.entry_type == entry_type)
            .collect()
    }

    /// 도구별 결과 검색
    pub fn tool_results(&self, tool_name: &str) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| {
                e.entry_type == MemoryType::ToolResult
                    && e.metadata.tool_name.as_deref() == Some(tool_name)
            })
            .collect()
    }
}

/// 장기 메모리 (세션 간 유지)
#[derive(Debug)]
pub struct LongTermMemory {
    entries: HashMap<String, MemoryEntry>,
    max_entries: usize,
}

impl Default for LongTermMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl LongTermMemory {
    pub fn new() -> Self {
        Self {
            entries: HashMap::with_capacity(1000),
            max_entries: 1000,
        }
    }

    /// 메모리 저장
    pub fn store(&mut self, entry: MemoryEntry) {
        if self.entries.len() >= self.max_entries {
            // LRU 기반으로 가장 오래된 항목 제거
            if let Some(oldest_id) = self.find_oldest() {
                self.entries.remove(&oldest_id);
            }
        }
        self.entries.insert(entry.id.clone(), entry);
    }

    /// ID로 검색
    pub fn get(&mut self, id: &str) -> Option<&mut MemoryEntry> {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.touch();
            Some(entry)
        } else {
            None
        }
    }

    /// 태그로 검색
    pub fn by_tag(&self, tag: &str) -> Vec<&MemoryEntry> {
        self.entries
            .values()
            .filter(|e| e.metadata.tags.contains(&tag.to_string()))
            .collect()
    }

    /// 가장 오래된 항목 ID 찾기
    fn find_oldest(&self) -> Option<String> {
        self.entries
            .iter()
            .min_by_key(|(_, e)| e.last_accessed)
            .map(|(id, _)| id.clone())
    }
}

/// 의미 검색기 (간단한 TF-IDF 기반)
#[derive(Debug)]
pub struct SemanticSearch {
    /// 단어 빈도 (문서별)
    doc_word_freq: HashMap<String, HashMap<String, usize>>,
    /// 역문서 빈도
    idf: HashMap<String, f32>,
    /// 총 문서 수
    doc_count: usize,
}

impl Default for SemanticSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticSearch {
    pub fn new() -> Self {
        Self {
            doc_word_freq: HashMap::new(),
            idf: HashMap::new(),
            doc_count: 0,
        }
    }

    /// 문서 인덱싱
    pub fn index(&mut self, doc_id: &str, content: &str) {
        let words = self.tokenize(content);
        let mut freq: HashMap<String, usize> = HashMap::new();
        
        for word in words {
            *freq.entry(word).or_insert(0) += 1;
        }
        
        self.doc_word_freq.insert(doc_id.to_string(), freq);
        self.doc_count += 1;
        self.update_idf();
    }

    /// 검색
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f32)> {
        let query_words = self.tokenize(query);
        let mut scores: Vec<(String, f32)> = Vec::new();

        for (doc_id, word_freq) in &self.doc_word_freq {
            let mut score = 0.0f32;
            for word in &query_words {
                if let Some(&freq) = word_freq.get(word) {
                    let tf = freq as f32 / word_freq.len() as f32;
                    let idf = self.idf.get(word).copied().unwrap_or(1.0);
                    score += tf * idf;
                }
            }
            if score > 0.0 {
                scores.push((doc_id.clone(), score));
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }

    /// 토큰화 (간단한 공백/구두점 분리)
    fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .map(|s| s.to_string())
            .collect()
    }

    /// IDF 업데이트
    fn update_idf(&mut self) {
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        
        for word_freq in self.doc_word_freq.values() {
            for word in word_freq.keys() {
                *doc_freq.entry(word.clone()).or_insert(0) += 1;
            }
        }

        self.idf.clear();
        for (word, freq) in doc_freq {
            let idf = (self.doc_count as f32 / freq as f32).ln() + 1.0;
            self.idf.insert(word, idf);
        }
    }
}

/// 통합 에이전트 메모리
#[derive(Debug)]
pub struct AgentMemory {
    short_term: ShortTermMemory,
    long_term: LongTermMemory,
    search: SemanticSearch,
}

impl Default for AgentMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentMemory {
    pub fn new() -> Self {
        Self {
            short_term: ShortTermMemory::new(),
            long_term: LongTermMemory::new(),
            search: SemanticSearch::new(),
        }
    }

    /// 도구 결과 저장
    pub fn remember_tool_result(
        &mut self,
        tool_name: &str,
        input: &str,
        output: &str,
        success: bool,
    ) {
        let entry = MemoryEntry::tool_result(tool_name, input, output, success);
        let id = entry.id.clone();
        let content = entry.content.clone();
        
        // 단기 메모리에 추가
        self.short_term.add(entry.clone());
        
        // 실패한 경우 장기 메모리에도 저장 (학습용)
        if !success {
            self.long_term.store(entry);
        }
        
        // 검색 인덱스에 추가
        self.search.index(&id, &content);
    }

    /// 에러 해결 패턴 저장
    pub fn remember_resolution(&mut self, error: &str, resolution: &str) {
        let entry = MemoryEntry::error_resolution(error, resolution);
        let id = entry.id.clone();
        let content = entry.content.clone();
        
        self.long_term.store(entry);
        self.search.index(&id, &content);
    }

    /// 관련 메모리 검색
    pub fn recall(&self, query: &str, top_k: usize) -> Vec<&MemoryEntry> {
        let results = self.search.search(query, top_k);
        let mut memories: Vec<&MemoryEntry> = Vec::new();
        
        for (doc_id, _) in results {
            // 단기 메모리에서 검색
            for entry in self.short_term.entries.iter() {
                if entry.id == doc_id {
                    memories.push(entry);
                    break;
                }
            }
            // 장기 메모리에서 검색
            if let Some(entry) = self.long_term.entries.get(&doc_id) {
                memories.push(entry);
            }
        }
        
        memories
    }

    /// 최근 도구 결과 가져오기
    pub fn recent_tool_results(&self, n: usize) -> Vec<&MemoryEntry> {
        self.short_term.recent(n)
            .into_iter()
            .filter(|e| e.entry_type == MemoryType::ToolResult)
            .collect()
    }

    /// 유사한 에러 해결 방법 찾기
    pub fn find_similar_resolution(&self, error: &str) -> Option<&MemoryEntry> {
        let results = self.search.search(error, 5);
        for (doc_id, score) in results {
            if score > 0.5 {
                if let Some(entry) = self.long_term.entries.get(&doc_id) {
                    if entry.entry_type == MemoryType::ErrorResolution {
                        return Some(entry);
                    }
                }
            }
        }
        None
    }

    /// 메모리 정리
    pub fn cleanup(&mut self) {
        self.short_term.cleanup();
    }

    /// 세션 초기화 (단기 메모리만)
    pub fn clear_session(&mut self) {
        self.short_term = ShortTermMemory::new();
    }
}

/// 간단한 UUID v4 생성 (외부 의존성 없이)
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:032x}", now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_term_memory() {
        let mut stm = ShortTermMemory::with_capacity(3);
        
        stm.add(MemoryEntry::tool_result("Read", "file.rs", "content", true));
        stm.add(MemoryEntry::tool_result("Grep", "pattern", "matches", true));
        stm.add(MemoryEntry::tool_result("Bash", "cargo build", "success", true));
        
        assert_eq!(stm.recent(2).len(), 2);
        assert_eq!(stm.tool_results("Read").len(), 1);
    }

    #[test]
    fn test_semantic_search() {
        let mut search = SemanticSearch::new();
        
        search.index("doc1", "rust cargo build compile");
        search.index("doc2", "python pip install package");
        search.index("doc3", "rust error borrow checker lifetime");
        
        let results = search.search("rust compile error", 2);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_agent_memory() {
        let mut memory = AgentMemory::new();
        
        memory.remember_tool_result("Bash", "cargo build", "error: cannot find value", false);
        memory.remember_resolution("cannot find value in scope", "Add import statement");
        
        // Test recall with exact match
        let results = memory.recall("cannot find value", 5);
        assert!(!results.is_empty(), "Should find matching entries");
        
        // Test recent tool results
        let recent = memory.recent_tool_results(5);
        assert!(!recent.is_empty(), "Should have recent tool results");
    }
}
