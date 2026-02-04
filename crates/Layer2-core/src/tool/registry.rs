//! Tool Registry - 도구 등록 및 관리
//!
//! Agent가 사용하는 모든 도구를 관리합니다.
//!
//! ## 기능
//! - 도구 등록/조회/제거
//! - Builtin 도구 자동 등록
//! - MCP 도구 통합 (McpBridge 연동)
//! - 카테고리별 그룹화
//!
//! ## Layer1 연동
//! - `Tool` trait으로 모든 도구 통합
//!
//! ## MCP 통합
//!
//! ```ignore
//! let mut registry = ToolRegistry::with_builtins();
//!
//! // MCP 도구 동기화
//! registry.sync_mcp_tools(&mcp_bridge).await;
//!
//! // 또는 개별 서버의 도구만
//! registry.add_mcp_tools("notion", mcp_tools);
//! ```

use super::builtin;
use forge_foundation::Tool;
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// 도구 레지스트리
///
/// ## 사용법
/// ```ignore
/// // 빈 레지스트리
/// let registry = ToolRegistry::new();
///
/// // Builtin 도구 포함
/// let registry = ToolRegistry::with_builtins();
///
/// // 도구 조회
/// if let Some(tool) = registry.get("read") {
///     let result = tool.execute(input, &context).await?;
/// }
/// ```
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// 빈 레지스트리 생성
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Builtin 도구들을 포함한 레지스트리 생성
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();

        // 모든 builtin 도구 등록
        for tool in builtin::all_tools() {
            registry.register(tool);
        }

        registry
    }

    /// 도구 등록
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// 여러 도구 한번에 등록
    pub fn register_all(&mut self, tools: Vec<Arc<dyn Tool>>) {
        for tool in tools {
            self.register(tool);
        }
    }

    /// 도구 조회
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// 도구 존재 여부
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// 도구 제거
    pub fn remove(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.remove(name)
    }

    /// 모든 도구
    pub fn all(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// 모든 도구 이름
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// 도구 개수
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// 도구 목록 (이름, 설명)
    pub fn list(&self) -> Vec<(&str, String)> {
        self.tools
            .iter()
            .map(|(name, tool)| (name.as_str(), tool.meta().description.clone()))
            .collect()
    }

    /// JSON Schema 형식으로 모든 도구 정보 반환 (MCP 호환)
    pub fn schemas(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|tool| {
                let meta = tool.meta();
                let schema = tool.schema();
                serde_json::json!({
                    "name": meta.name,
                    "description": meta.description,
                    "input_schema": schema
                })
            })
            .collect()
    }

    /// 카테고리별 도구 목록
    pub fn by_category(&self) -> HashMap<String, Vec<Arc<dyn Tool>>> {
        let mut result: HashMap<String, Vec<Arc<dyn Tool>>> = HashMap::new();
        for tool in self.tools.values() {
            let category = tool.meta().category.clone();
            result.entry(category).or_default().push(Arc::clone(tool));
        }
        result
    }

    // ========================================================================
    // MCP Tool Integration
    // ========================================================================

    /// MCP 서버의 도구들 추가
    ///
    /// 도구 이름은 `mcp_{server}_{tool}` 형식으로 등록됩니다.
    pub fn add_mcp_tools(&mut self, server_name: &str, tools: Vec<Arc<dyn Tool>>) {
        let count = tools.len();
        for tool in tools {
            let name = tool.meta().name.clone();
            // 이미 mcp_ 접두사가 있으면 그대로, 없으면 추가
            let key = if name.starts_with("mcp_") {
                name
            } else {
                format!("mcp_{}_{}", server_name, name)
            };
            self.tools.insert(key, tool);
        }
        info!("Added {} MCP tools from server '{}'", count, server_name);
    }

    /// 특정 MCP 서버의 도구들 제거
    pub fn remove_mcp_tools(&mut self, server_name: &str) {
        let prefix = format!("mcp_{}_", server_name);
        let to_remove: Vec<String> = self
            .tools
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();

        let count = to_remove.len();
        for key in to_remove {
            self.tools.remove(&key);
        }

        if count > 0 {
            debug!("Removed {} MCP tools from server '{}'", count, server_name);
        }
    }

    /// 모든 MCP 도구 제거
    pub fn remove_all_mcp_tools(&mut self) {
        let to_remove: Vec<String> = self
            .tools
            .keys()
            .filter(|k| k.starts_with("mcp_"))
            .cloned()
            .collect();

        let count = to_remove.len();
        for key in to_remove {
            self.tools.remove(&key);
        }

        if count > 0 {
            debug!("Removed all {} MCP tools", count);
        }
    }

    /// MCP 도구만 조회
    pub fn mcp_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools
            .iter()
            .filter(|(k, _)| k.starts_with("mcp_"))
            .map(|(_, v)| Arc::clone(v))
            .collect()
    }

    /// MCP 도구 개수
    pub fn mcp_tool_count(&self) -> usize {
        self.tools.keys().filter(|k| k.starts_with("mcp_")).count()
    }

    /// Builtin 도구만 조회 (MCP 제외)
    pub fn builtin_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools
            .iter()
            .filter(|(k, _)| !k.starts_with("mcp_"))
            .map(|(_, v)| Arc::clone(v))
            .collect()
    }

    // ========================================================================
    // Agent Integration
    // ========================================================================

    /// 도구 정의 목록 반환 (Agent용)
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| {
                let meta = tool.meta();
                let schema = tool.schema();
                ToolDefinition {
                    name: meta.name.clone(),
                    description: meta.description.clone(),
                    parameters: ToolParameters {
                        schema_type: "object".to_string(),
                        properties: schema.get("properties").cloned().unwrap_or_default(),
                        required: schema
                            .get("required")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    },
                }
            })
            .collect()
    }

    /// 도구 실행 (Agent용)
    pub async fn execute(
        &self,
        name: &str,
        ctx: &dyn forge_foundation::ToolContext,
        args: serde_json::Value,
    ) -> ToolExecuteResult {
        let start = Instant::now();

        let result = match self.get(name) {
            Some(tool) => match tool.execute(args, ctx).await {
                Ok(result) => ToolExecuteResult {
                    success: result.success,
                    content: result.output,
                    error: result.error,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    tool_name: Some(name.to_string()),
                    call_id: None,
                },
                Err(e) => ToolExecuteResult {
                    success: false,
                    content: String::new(),
                    error: Some(e.to_string()),
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    tool_name: Some(name.to_string()),
                    call_id: None,
                },
            },
            None => ToolExecuteResult {
                success: false,
                content: String::new(),
                error: Some(format!("Tool not found: {}", name)),
                duration_ms: Some(start.elapsed().as_millis() as u64),
                tool_name: Some(name.to_string()),
                call_id: None,
            },
        };

        debug!(
            "Tool '{}' executed in {}ms, success: {}",
            name,
            result.duration_ms.unwrap_or(0),
            result.success
        );

        result
    }

    /// 여러 도구를 병렬로 실행
    ///
    /// 의존성이 없는 도구들을 동시에 실행하여 성능을 향상시킵니다.
    ///
    /// ## 예시
    /// ```ignore
    /// let calls = vec![
    ///     ParallelToolCall::new("call-1", "read", json!({"file_path": "/a.txt"})),
    ///     ParallelToolCall::new("call-2", "read", json!({"file_path": "/b.txt"})),
    ///     ParallelToolCall::new("call-3", "glob", json!({"pattern": "*.rs"})),
    /// ];
    ///
    /// let (results, stats) = registry.execute_parallel(calls, &ctx, None).await;
    /// ```
    pub async fn execute_parallel(
        &self,
        calls: Vec<ParallelToolCall>,
        ctx: &dyn forge_foundation::ToolContext,
        config: Option<ParallelExecutionConfig>,
    ) -> (Vec<ToolExecuteResult>, ParallelExecutionStats) {
        let config = config.unwrap_or_default();
        let total_calls = calls.len();
        let start = Instant::now();

        info!(
            "Starting parallel execution of {} tool calls with max_concurrency={}",
            total_calls, config.max_concurrency
        );

        // 세마포어로 동시 실행 수 제한
        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));

        // 각 호출에 대한 Future 생성
        let futures: Vec<_> = calls
            .into_iter()
            .map(|call| {
                let sem = Arc::clone(&semaphore);
                let tool = self.get(&call.tool_name);
                let timeout = call.timeout.unwrap_or(config.default_timeout);

                async move {
                    let call_start = Instant::now();

                    // 세마포어 획득
                    let _permit = sem.acquire().await.ok();

                    let result = match tool {
                        Some(t) => {
                            // 타임아웃 적용
                            match tokio::time::timeout(timeout, t.execute(call.args, ctx)).await {
                                Ok(Ok(res)) => ToolExecuteResult {
                                    success: res.success,
                                    content: res.output,
                                    error: res.error,
                                    duration_ms: Some(call_start.elapsed().as_millis() as u64),
                                    tool_name: Some(call.tool_name.clone()),
                                    call_id: Some(call.call_id.clone()),
                                },
                                Ok(Err(e)) => ToolExecuteResult {
                                    success: false,
                                    content: String::new(),
                                    error: Some(e.to_string()),
                                    duration_ms: Some(call_start.elapsed().as_millis() as u64),
                                    tool_name: Some(call.tool_name.clone()),
                                    call_id: Some(call.call_id.clone()),
                                },
                                Err(_) => {
                                    warn!(
                                        "Tool '{}' timed out after {:?}",
                                        call.tool_name, timeout
                                    );
                                    ToolExecuteResult {
                                        success: false,
                                        content: String::new(),
                                        error: Some(format!(
                                            "Timeout after {}ms",
                                            timeout.as_millis()
                                        )),
                                        duration_ms: Some(call_start.elapsed().as_millis() as u64),
                                        tool_name: Some(call.tool_name.clone()),
                                        call_id: Some(call.call_id.clone()),
                                    }
                                }
                            }
                        }
                        None => ToolExecuteResult {
                            success: false,
                            content: String::new(),
                            error: Some(format!("Tool not found: {}", call.tool_name)),
                            duration_ms: Some(call_start.elapsed().as_millis() as u64),
                            tool_name: Some(call.tool_name.clone()),
                            call_id: Some(call.call_id.clone()),
                        },
                    };

                    result
                }
            })
            .collect();

        // 모든 Future 병렬 실행
        let results = join_all(futures).await;

        // 통계 계산
        let mut stats = ParallelExecutionStats {
            total_calls,
            ..Default::default()
        };

        for result in &results {
            if result.success {
                stats.successful += 1;
            } else if result.error.as_ref().is_some_and(|e| e.contains("Timeout")) {
                stats.timed_out += 1;
                stats.failed += 1;
            } else {
                stats.failed += 1;
            }
        }

        stats.total_duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "Parallel execution completed: {} total, {} success, {} failed, {} timed out, {}ms total",
            stats.total_calls,
            stats.successful,
            stats.failed,
            stats.timed_out,
            stats.total_duration_ms
        );

        (results, stats)
    }

    /// 도구 의존성 분석하여 최적의 실행 순서 결정
    ///
    /// 파일 경로 기반으로 간단한 의존성 분석을 수행합니다.
    /// - 같은 파일을 읽고 쓰는 도구는 순차 실행
    /// - 다른 파일을 다루는 도구는 병렬 실행 가능
    pub fn analyze_dependencies(&self, calls: &[ParallelToolCall]) -> Vec<Vec<usize>> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut file_locks: HashMap<String, usize> = HashMap::new(); // file -> group index

        for (idx, call) in calls.iter().enumerate() {
            // 파일 경로 추출
            let file_path = call
                .args
                .get("file_path")
                .or_else(|| call.args.get("path"))
                .and_then(|v| v.as_str())
                .map(String::from);

            // 쓰기 작업인지 확인
            let is_write = matches!(call.tool_name.as_str(), "write" | "edit");

            if let Some(path) = file_path {
                if is_write {
                    // 쓰기 작업은 해당 파일에 대한 이전 작업 완료 후 실행
                    if let Some(&group_idx) = file_locks.get(&path) {
                        // 새 그룹 생성 (이전 그룹 이후)
                        let new_group_idx = group_idx + 1;
                        while groups.len() <= new_group_idx {
                            groups.push(Vec::new());
                        }
                        groups[new_group_idx].push(idx);
                        file_locks.insert(path, new_group_idx);
                    } else {
                        // 첫 번째 그룹에 추가
                        if groups.is_empty() {
                            groups.push(Vec::new());
                        }
                        groups[0].push(idx);
                        file_locks.insert(path, 0);
                    }
                } else {
                    // 읽기 작업은 같은 파일의 마지막 쓰기 이후 그룹에 추가
                    if let Some(&group_idx) = file_locks.get(&path) {
                        while groups.len() <= group_idx {
                            groups.push(Vec::new());
                        }
                        groups[group_idx].push(idx);
                    } else {
                        // 첫 번째 그룹에 추가
                        if groups.is_empty() {
                            groups.push(Vec::new());
                        }
                        groups[0].push(idx);
                    }
                }
            } else {
                // 파일 경로 없음 - 첫 번째 그룹에 추가 (병렬 실행 가능)
                if groups.is_empty() {
                    groups.push(Vec::new());
                }
                groups[0].push(idx);
            }
        }

        // 빈 그룹 제거
        groups.retain(|g| !g.is_empty());

        if groups.is_empty() {
            groups.push((0..calls.len()).collect());
        }

        debug!(
            "Dependency analysis: {} calls -> {} execution groups",
            calls.len(),
            groups.len()
        );

        groups
    }

    /// 의존성을 고려한 스마트 병렬 실행
    ///
    /// 의존성 분석 후 그룹별로 순차 실행하되, 각 그룹 내에서는 병렬 실행
    pub async fn execute_smart_parallel(
        &self,
        calls: Vec<ParallelToolCall>,
        ctx: &dyn forge_foundation::ToolContext,
        config: Option<ParallelExecutionConfig>,
    ) -> (Vec<ToolExecuteResult>, ParallelExecutionStats) {
        let groups = self.analyze_dependencies(&calls);
        let config = config.unwrap_or_default();
        let total_calls = calls.len();
        let start = Instant::now();

        info!(
            "Smart parallel execution: {} calls in {} groups",
            total_calls,
            groups.len()
        );

        let mut all_results: Vec<ToolExecuteResult> = vec![
            ToolExecuteResult {
                success: false,
                content: String::new(),
                error: Some("Not executed".to_string()),
                duration_ms: None,
                tool_name: None,
                call_id: None,
            };
            total_calls
        ];

        let mut total_stats = ParallelExecutionStats {
            total_calls,
            ..Default::default()
        };

        // 그룹별로 순차 실행
        for group in groups {
            let group_calls: Vec<ParallelToolCall> = group
                .iter()
                .filter_map(|&idx| calls.get(idx).cloned())
                .collect();

            let (results, stats) = self
                .execute_parallel(group_calls, ctx, Some(config.clone()))
                .await;

            // 결과 병합
            for (i, idx) in group.iter().enumerate() {
                if let Some(result) = results.get(i) {
                    all_results[*idx] = result.clone();
                }
            }

            total_stats.successful += stats.successful;
            total_stats.failed += stats.failed;
            total_stats.timed_out += stats.timed_out;
        }

        total_stats.total_duration_ms = start.elapsed().as_millis() as u64;

        (all_results, total_stats)
    }
}

/// 도구 정의 (Agent용)
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: ToolParameters,
}

/// 도구 파라미터 정의
#[derive(Debug, Clone)]
pub struct ToolParameters {
    pub schema_type: String,
    pub properties: serde_json::Value,
    pub required: Vec<String>,
}

/// 도구 실행 결과
#[derive(Debug, Clone)]
pub struct ToolExecuteResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
    /// 실행 시간 (밀리초)
    pub duration_ms: Option<u64>,
    /// 도구 이름 (병렬 실행 시 유용)
    pub tool_name: Option<String>,
    /// 도구 호출 ID (병렬 실행 시 유용)
    pub call_id: Option<String>,
}

/// 병렬 실행 요청
#[derive(Debug, Clone)]
pub struct ParallelToolCall {
    /// 고유 호출 ID
    pub call_id: String,
    /// 도구 이름
    pub tool_name: String,
    /// 도구 입력
    pub args: serde_json::Value,
    /// 타임아웃 (선택)
    pub timeout: Option<Duration>,
}

impl ParallelToolCall {
    pub fn new(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            args,
            timeout: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// 병렬 실행 설정
#[derive(Debug, Clone)]
pub struct ParallelExecutionConfig {
    /// 최대 동시 실행 수
    pub max_concurrency: usize,
    /// 기본 타임아웃
    pub default_timeout: Duration,
    /// 실패 시 계속 진행 여부
    pub continue_on_error: bool,
}

impl Default for ParallelExecutionConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            default_timeout: Duration::from_secs(30),
            continue_on_error: true,
        }
    }
}

/// 병렬 실행 결과 통계
#[derive(Debug, Clone, Default)]
pub struct ParallelExecutionStats {
    /// 총 호출 수
    pub total_calls: usize,
    /// 성공 수
    pub successful: usize,
    /// 실패 수
    pub failed: usize,
    /// 타임아웃 수
    pub timed_out: usize,
    /// 총 실행 시간 (밀리초)
    pub total_duration_ms: u64,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_with_builtins() {
        let registry = ToolRegistry::with_builtins();
        assert!(!registry.is_empty());
        assert!(registry.contains("read"));
    }

    #[test]
    fn test_registry_get() {
        let registry = ToolRegistry::with_builtins();
        let read = registry.get("read");
        assert!(read.is_some());
        assert_eq!(read.unwrap().name(), "read");
    }

    #[test]
    fn test_registry_schemas() {
        let registry = ToolRegistry::with_builtins();
        let schemas = registry.schemas();
        assert!(!schemas.is_empty());

        // 각 스키마는 name, description, input_schema를 가져야 함
        for schema in schemas {
            assert!(schema.get("name").is_some());
            assert!(schema.get("description").is_some());
            assert!(schema.get("input_schema").is_some());
        }
    }

    #[test]
    fn test_registry_by_category() {
        let registry = ToolRegistry::with_builtins();
        let by_cat = registry.by_category();

        // filesystem 카테고리가 있어야 함
        assert!(by_cat.contains_key("filesystem"));
    }
}
