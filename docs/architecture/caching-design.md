# ForgeCode Caching Architecture Design

## Overview

ForgeCode의 캐싱 시스템은 **리소스 최소화**와 **LLM 비용 절감**을 핵심 목표로 설계됩니다.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    ForgeCode Caching Architecture                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Layer 1: Context Management (Agent Level)                       │   │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐    │   │
│  │  │ Observation  │ │   Context    │ │    Conversation      │    │   │
│  │  │   Masker     │ │  Compactor   │ │    Summarizer        │    │   │
│  │  └──────────────┘ └──────────────┘ └──────────────────────┘    │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│                                    ▼                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Layer 2: Response Cache (Application Level)                     │   │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐    │   │
│  │  │    Tool      │ │     MCP      │ │       LSP            │    │   │
│  │  │   Cache      │ │    Cache     │ │      Cache           │    │   │
│  │  └──────────────┘ └──────────────┘ └──────────────────────┘    │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                    │                                    │
│                                    ▼                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Layer 3: Provider Cache (API Level)                             │   │
│  │  ┌──────────────────────────────────────────────────────────┐   │   │
│  │  │  Prompt Prefix Cache (Provider Native - Anthropic/OpenAI) │   │   │
│  │  └──────────────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Design Principles

### 1. Minimal Resource Usage
- **Lazy Initialization**: 필요할 때만 캐시 생성
- **Bounded Memory**: 모든 캐시에 크기 상한 설정
- **Aggressive Eviction**: LRU + TTL 기반 자동 정리

### 2. Cost Efficiency
- **Provider Caching First**: Anthropic/OpenAI의 네이티브 캐싱 최대 활용
- **Context Reduction**: LLM 호출 전 컨텍스트 크기 최소화
- **Observation Masking**: 불필요한 과거 데이터 제거

### 3. Simplicity
- **No External Dependencies**: Redis 등 외부 서비스 불필요
- **In-Memory First**: 대부분 in-memory, 필요시 SQLite
- **Opt-in Complexity**: 기본은 단순, 고급 기능은 선택적

---

## Layer 1: Context Management

### 1.1 Observation Masker

가장 효율적인 컨텍스트 관리 방법. JetBrains 연구 결과 52% 비용 절감.

```rust
/// Tool 실행 결과(observation)를 관리
pub struct ObservationMasker {
    /// 최근 N개 observation만 전체 유지
    window_size: usize,
    /// 마스킹된 observation placeholder
    placeholder_template: String,
}

impl ObservationMasker {
    /// 기본값: 최근 10개만 유지
    pub fn new() -> Self {
        Self {
            window_size: 10,
            placeholder_template: "[Previous output truncated]".into(),
        }
    }

    /// Context 메시지 리스트에서 오래된 observation 마스킹
    pub fn mask(&self, messages: &mut Vec<Message>) {
        let observations: Vec<_> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_tool_result())
            .collect();

        // window_size 이전의 observation들을 placeholder로 대체
        for (idx, _) in observations.iter().rev().skip(self.window_size) {
            messages[*idx].content = self.placeholder_template.clone();
        }
    }
}
```

**적용 시점**: Agent가 LLM 호출 전 매번 실행

### 1.2 Context Compactor

파일 내용을 경로 참조로 압축 (가역적).

```rust
/// 큰 컨텐츠를 참조로 압축
pub struct ContextCompactor {
    /// 압축 임계값 (바이트)
    threshold_bytes: usize,
    /// 원본 저장소 (복원용)
    storage: HashMap<ContentId, String>,
}

pub struct CompactedContent {
    /// 압축 여부
    pub is_compacted: bool,
    /// 압축된 경우 참조, 아니면 원본
    pub content: String,
    /// 복원 키 (압축된 경우만)
    pub restore_key: Option<ContentId>,
}

impl ContextCompactor {
    /// 기본 임계값: 4KB
    pub fn new() -> Self {
        Self {
            threshold_bytes: 4096,
            storage: HashMap::new(),
        }
    }

    /// 파일 내용 압축
    pub fn compact_file_content(&mut self, path: &Path, content: &str) -> CompactedContent {
        if content.len() < self.threshold_bytes {
            return CompactedContent {
                is_compacted: false,
                content: content.to_string(),
                restore_key: None,
            };
        }

        let id = ContentId::new();
        self.storage.insert(id, content.to_string());

        CompactedContent {
            is_compacted: true,
            content: format!("[File: {} ({} bytes) - use Read tool to view]", 
                           path.display(), content.len()),
            restore_key: Some(id),
        }
    }

    /// Tool 결과 압축
    pub fn compact_tool_result(&mut self, tool: &str, result: &str) -> CompactedContent {
        if result.len() < self.threshold_bytes {
            return CompactedContent {
                is_compacted: false,
                content: result.to_string(),
                restore_key: None,
            };
        }

        let id = ContentId::new();
        let preview = &result[..200.min(result.len())];
        self.storage.insert(id, result.to_string());

        CompactedContent {
            is_compacted: true,
            content: format!("[{} output truncated: {}... ({} bytes total)]",
                           tool, preview, result.len()),
            restore_key: Some(id),
        }
    }
}
```

### 1.3 Conversation Summarizer

컨텍스트가 임계값을 초과할 때 사용 (마지막 수단).

```rust
/// LLM을 사용한 대화 요약 (비용 발생)
pub struct ConversationSummarizer {
    /// 요약 트리거 임계값 (토큰)
    threshold_tokens: usize,
    /// 최근 유지할 메시지 수
    keep_recent: usize,
    /// 요약에 사용할 모델
    summary_model: String,
}

impl ConversationSummarizer {
    pub fn new() -> Self {
        Self {
            threshold_tokens: 100_000,  // 100K 토큰
            keep_recent: 10,            // 최근 10개 유지
            summary_model: "claude-3-haiku".into(),  // 저렴한 모델
        }
    }

    /// 요약 필요 여부 체크
    pub fn needs_summarization(&self, messages: &[Message]) -> bool {
        let total_tokens: usize = messages.iter()
            .map(|m| estimate_tokens(&m.content))
            .sum();
        total_tokens > self.threshold_tokens
    }

    /// 대화 요약 실행
    pub async fn summarize(&self, messages: &[Message]) -> SummaryResult {
        let to_summarize = &messages[..messages.len() - self.keep_recent];
        let recent = &messages[messages.len() - self.keep_recent..];

        // LLM 호출로 요약 생성
        let summary = self.call_summarizer(to_summarize).await?;

        SummaryResult {
            summary_message: Message::system(format!(
                "[Conversation Summary]\n{}\n[End Summary - {} messages condensed]",
                summary, to_summarize.len()
            )),
            preserved_messages: recent.to_vec(),
        }
    }
}
```

---

## Layer 2: Response Cache

### 2.1 Tool Cache

동일한 입력에 대해 동일한 결과를 반환하는 Tool만 캐싱.

```rust
/// Tool 결과 캐시 (Read, Glob, Grep 등 순수 함수형 Tool)
pub struct ToolCache {
    /// LRU 캐시
    cache: LruCache<ToolCacheKey, CachedToolResult>,
    /// 캐시 가능한 Tool 목록
    cacheable_tools: HashSet<String>,
    /// 파일 변경 감지용
    file_watcher: Option<FileWatcher>,
}

#[derive(Hash, Eq, PartialEq)]
pub struct ToolCacheKey {
    tool_name: String,
    args_hash: u64,
}

pub struct CachedToolResult {
    result: ToolResult,
    cached_at: Instant,
    file_hash: Option<u64>,  // 파일 관련 캐시의 경우
}

impl ToolCache {
    pub fn new(max_entries: usize) -> Self {
        let mut cacheable = HashSet::new();
        // 순수 함수형 Tool만 캐시 가능
        cacheable.insert("Read".into());
        cacheable.insert("Glob".into());
        cacheable.insert("Grep".into());
        // Bash, Write, Edit 등은 부작용이 있으므로 캐시 불가

        Self {
            cache: LruCache::new(max_entries),
            cacheable_tools: cacheable,
            file_watcher: None,
        }
    }

    pub fn get(&mut self, tool: &str, args: &Value) -> Option<&ToolResult> {
        if !self.cacheable_tools.contains(tool) {
            return None;
        }

        let key = ToolCacheKey {
            tool_name: tool.to_string(),
            args_hash: hash_value(args),
        };

        self.cache.get(&key).map(|c| &c.result)
    }

    pub fn insert(&mut self, tool: &str, args: &Value, result: ToolResult) {
        if !self.cacheable_tools.contains(tool) {
            return;
        }

        let key = ToolCacheKey {
            tool_name: tool.to_string(),
            args_hash: hash_value(args),
        };

        self.cache.insert(key, CachedToolResult {
            result,
            cached_at: Instant::now(),
            file_hash: None,
        });
    }

    /// 파일 변경 시 관련 캐시 무효화
    pub fn invalidate_for_file(&mut self, path: &Path) {
        self.cache.retain(|k, _| {
            // Read, Glob 등 파일 관련 캐시 무효화 로직
            !self.is_affected_by_file(k, path)
        });
    }
}
```

### 2.2 MCP Cache

MCP 서버의 Tool 정의와 스키마 캐싱.

```rust
/// MCP Tool 정의 캐시
pub struct McpCache {
    /// 서버별 Tool 정의
    tool_definitions: HashMap<String, Vec<ToolDefinition>>,
    /// 캐시 시간
    cached_at: HashMap<String, Instant>,
    /// TTL (기본 30분)
    ttl: Duration,
}

impl McpCache {
    pub fn new() -> Self {
        Self {
            tool_definitions: HashMap::new(),
            cached_at: HashMap::new(),
            ttl: Duration::from_secs(30 * 60),
        }
    }

    pub fn get_tools(&self, server_id: &str) -> Option<&[ToolDefinition]> {
        let cached_time = self.cached_at.get(server_id)?;
        if cached_time.elapsed() > self.ttl {
            return None;
        }
        self.tool_definitions.get(server_id).map(|v| v.as_slice())
    }

    pub fn set_tools(&mut self, server_id: &str, tools: Vec<ToolDefinition>) {
        self.tool_definitions.insert(server_id.to_string(), tools);
        self.cached_at.insert(server_id.to_string(), Instant::now());
    }

    /// 서버 재시작 시 무효화
    pub fn invalidate(&mut self, server_id: &str) {
        self.tool_definitions.remove(server_id);
        self.cached_at.remove(server_id);
    }
}
```

### 2.3 LSP Cache

LSP 심볼 및 진단 정보 캐싱.

```rust
/// LSP 결과 캐시
pub struct LspCache {
    /// 파일별 심볼 캐시
    symbols: HashMap<PathBuf, CachedSymbols>,
    /// 파일별 진단 캐시
    diagnostics: HashMap<PathBuf, CachedDiagnostics>,
    /// 파일 해시 (변경 감지)
    file_hashes: HashMap<PathBuf, u64>,
}

struct CachedSymbols {
    symbols: Vec<Symbol>,
    file_hash: u64,
}

impl LspCache {
    pub fn get_symbols(&self, path: &Path) -> Option<&[Symbol]> {
        let cached = self.symbols.get(path)?;
        let current_hash = self.file_hashes.get(path)?;
        
        // 파일이 변경되지 않았으면 캐시 반환
        if cached.file_hash == *current_hash {
            Some(&cached.symbols)
        } else {
            None
        }
    }

    pub fn update_file_hash(&mut self, path: &Path, hash: u64) {
        self.file_hashes.insert(path.to_path_buf(), hash);
    }

    pub fn set_symbols(&mut self, path: &Path, symbols: Vec<Symbol>, hash: u64) {
        self.symbols.insert(path.to_path_buf(), CachedSymbols {
            symbols,
            file_hash: hash,
        });
    }
}
```

---

## Layer 3: Provider Cache

Provider 레벨 캐싱은 Anthropic/OpenAI가 자동으로 처리합니다.

### 3.1 Prompt Prefix Optimization

ForgeCode가 할 일: **캐시 히트율 최대화를 위한 프롬프트 구조화**

```rust
/// Provider 캐시 최적화를 위한 프롬프트 구성
pub struct PromptOptimizer {
    /// 시스템 프롬프트 (고정, 캐시됨)
    system_prompt: String,
    /// Tool 정의 (거의 고정, 캐시됨)
    tool_definitions: String,
    /// 컨텍스트 (세션별, 일부 캐시)
    context_template: String,
}

impl PromptOptimizer {
    /// 캐시 친화적 프롬프트 구성
    /// 
    /// Provider 캐시는 prefix 기반이므로:
    /// 1. 고정 부분을 앞에 배치 (system prompt, tools)
    /// 2. 가변 부분을 뒤에 배치 (messages)
    pub fn build_request(&self, messages: &[Message]) -> ChatRequest {
        ChatRequest {
            // 1. 시스템 프롬프트 (항상 동일 = 캐시됨)
            system: self.system_prompt.clone(),
            
            // 2. Tool 정의 (거의 동일 = 캐시됨)
            tools: self.parse_tools(),
            
            // 3. 대화 히스토리 (가변)
            messages: messages.to_vec(),
        }
    }
}
```

### 3.2 Cache Metrics

Provider 캐시 효율성 모니터링.

```rust
/// Provider 캐시 통계
pub struct ProviderCacheMetrics {
    /// 캐시된 입력 토큰
    pub cached_input_tokens: u64,
    /// 캐시되지 않은 입력 토큰
    pub uncached_input_tokens: u64,
    /// 예상 비용 절감액
    pub estimated_savings: f64,
}

impl ProviderCacheMetrics {
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cached_input_tokens + self.uncached_input_tokens;
        if total == 0 {
            0.0
        } else {
            self.cached_input_tokens as f64 / total as f64
        }
    }
}
```

---

## Unified Cache Manager

모든 캐시를 통합 관리하는 매니저.

```rust
/// ForgeCode 통합 캐시 매니저
pub struct CacheManager {
    /// 설정
    config: CacheConfig,
    
    // Layer 1: Context Management
    observation_masker: ObservationMasker,
    context_compactor: ContextCompactor,
    conversation_summarizer: ConversationSummarizer,
    
    // Layer 2: Response Cache
    tool_cache: ToolCache,
    mcp_cache: McpCache,
    lsp_cache: LspCache,
    
    // Metrics
    metrics: CacheMetrics,
}

#[derive(Clone)]
pub struct CacheConfig {
    /// Context Management
    pub observation_window: usize,      // default: 10
    pub compact_threshold: usize,       // default: 4096 bytes
    pub summarize_threshold: usize,     // default: 100_000 tokens
    
    /// Response Cache
    pub tool_cache_size: usize,         // default: 100 entries
    pub mcp_cache_ttl: Duration,        // default: 30 minutes
    
    /// Resource Limits
    pub max_memory_mb: usize,           // default: 100 MB
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            observation_window: 10,
            compact_threshold: 4096,
            summarize_threshold: 100_000,
            tool_cache_size: 100,
            mcp_cache_ttl: Duration::from_secs(30 * 60),
            max_memory_mb: 100,
        }
    }
}

impl CacheManager {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            observation_masker: ObservationMasker::with_window(config.observation_window),
            context_compactor: ContextCompactor::with_threshold(config.compact_threshold),
            conversation_summarizer: ConversationSummarizer::with_threshold(config.summarize_threshold),
            tool_cache: ToolCache::new(config.tool_cache_size),
            mcp_cache: McpCache::with_ttl(config.mcp_cache_ttl),
            lsp_cache: LspCache::new(),
            metrics: CacheMetrics::default(),
            config,
        }
    }

    /// Agent가 LLM 호출 전 컨텍스트 최적화
    pub async fn optimize_context(&mut self, messages: &mut Vec<Message>) {
        // 1. Observation Masking (가장 효율적, 항상 실행)
        self.observation_masker.mask(messages);
        
        // 2. Context Compaction (큰 내용 압축)
        for msg in messages.iter_mut() {
            if let Some(compacted) = self.context_compactor.try_compact(&msg.content) {
                msg.content = compacted;
            }
        }
        
        // 3. Summarization (마지막 수단, 임계값 초과 시만)
        if self.conversation_summarizer.needs_summarization(messages) {
            let result = self.conversation_summarizer.summarize(messages).await;
            *messages = result.into_messages();
        }
    }

    /// Tool 결과 캐시 조회
    pub fn get_tool_result(&mut self, tool: &str, args: &Value) -> Option<ToolResult> {
        self.tool_cache.get(tool, args).cloned()
    }

    /// Tool 결과 캐시 저장
    pub fn cache_tool_result(&mut self, tool: &str, args: &Value, result: ToolResult) {
        self.tool_cache.insert(tool, args, result);
    }

    /// 파일 변경 알림
    pub fn on_file_changed(&mut self, path: &Path) {
        self.tool_cache.invalidate_for_file(path);
        self.lsp_cache.invalidate_for_file(path);
    }

    /// 메모리 사용량 체크 및 정리
    pub fn check_memory_pressure(&mut self) {
        let used_mb = self.estimate_memory_usage() / (1024 * 1024);
        if used_mb > self.config.max_memory_mb {
            self.evict_oldest();
        }
    }

    /// 통계 조회
    pub fn metrics(&self) -> &CacheMetrics {
        &self.metrics
    }
}
```

---

## Module Structure

```
crates/Layer1-foundation/src/cache/
├── mod.rs              # 모듈 exports
├── config.rs           # CacheConfig
├── manager.rs          # CacheManager (통합)
├── context/
│   ├── mod.rs
│   ├── masker.rs       # ObservationMasker
│   ├── compactor.rs    # ContextCompactor
│   └── summarizer.rs   # ConversationSummarizer
├── response/
│   ├── mod.rs
│   ├── tool.rs         # ToolCache
│   ├── mcp.rs          # McpCache
│   └── lsp.rs          # LspCache
├── provider/
│   ├── mod.rs
│   ├── optimizer.rs    # PromptOptimizer
│   └── metrics.rs      # ProviderCacheMetrics
└── util/
    ├── mod.rs
    ├── lru.rs          # LRU Cache implementation
    └── hash.rs         # Hashing utilities
```

---

## Implementation Priority

### Phase 1: Core (Must Have)
1. `ObservationMasker` - 가장 효율적, 구현 단순
2. `ContextCompactor` - 파일 내용 압축
3. `CacheConfig` - 설정 구조

### Phase 2: Optimization (Should Have)
4. `ToolCache` - Read/Glob/Grep 캐싱
5. `McpCache` - Tool 정의 캐싱
6. `CacheManager` - 통합 관리

### Phase 3: Advanced (Nice to Have)
7. `ConversationSummarizer` - LLM 기반 요약
8. `LspCache` - LSP 결과 캐싱
9. `ProviderCacheMetrics` - 캐시 효율 모니터링

---

## Resource Budget

| Component | Memory (Max) | Notes |
|-----------|-------------|-------|
| ObservationMasker | ~0 | 참조만 유지 |
| ContextCompactor | 10 MB | 압축된 원본 저장 |
| ToolCache | 20 MB | 100 entries |
| McpCache | 5 MB | Tool 정의만 |
| LspCache | 10 MB | 심볼/진단 |
| **Total** | **~50 MB** | 실제 사용량은 더 적음 |

---

## Event Integration

캐시 시스템은 EventBus와 연동하여 무효화 이벤트 처리:

```rust
impl EventListener for CacheManager {
    fn on_event(&mut self, event: &ForgeEvent) {
        match event.event_type.as_str() {
            "file.modified" | "file.created" | "file.deleted" => {
                if let Some(path) = event.data.get("path").and_then(|v| v.as_str()) {
                    self.on_file_changed(Path::new(path));
                }
            }
            "mcp.server.restarted" => {
                if let Some(server_id) = event.data.get("server_id").and_then(|v| v.as_str()) {
                    self.mcp_cache.invalidate(server_id);
                }
            }
            _ => {}
        }
    }
}
```

---

## Summary

ForgeCode의 캐싱 전략은 **"Less is More"** 원칙을 따릅니다:

1. **Provider 캐시 최대 활용** - 무료로 제공되는 KV 캐시
2. **Observation Masking 우선** - 가장 효율적, 부작용 없음
3. **선택적 캐싱** - 순수 함수형 Tool만 캐싱
4. **요약은 마지막 수단** - 추가 비용 발생하므로 최소화

이 설계는 **최소 리소스로 최대 효율**을 달성합니다.
