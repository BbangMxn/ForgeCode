//! Parallel tool execution with dependency analysis
//!
//! Enables concurrent execution of independent tool calls while
//! respecting data dependencies between operations.
//!
//! ## Dependency Analysis
//!
//! Tools are analyzed for dependencies based on:
//! - File paths (read/write conflicts)
//! - Resource access patterns
//! - Explicit ordering requirements
//!
//! ## Example
//!
//! ```ignore
//! use std::sync::Arc;
//! 
//! let executor = ParallelToolExecutor::new(4);
//! let ctx = Arc::new(RuntimeContext::new(...));
//!
//! let calls = vec![
//!     ToolCall { id: "1".into(), name: "read".into(), arguments: json!({"path": "a.txt"}) },
//!     ToolCall { id: "2".into(), name: "read".into(), arguments: json!({"path": "b.txt"}) },
//!     ToolCall { id: "3".into(), name: "write".into(), arguments: json!({"path": "c.txt", "content": "..."}) },
//! ];
//!
//! let results = executor.execute(ctx, &registry, &calls).await;
//! ```

use super::context::RuntimeContext;
use super::registry::ToolRegistry;
use forge_foundation::ToolContext;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Result of a tool execution
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// Tool name
    pub tool_name: String,
    /// Tool call ID
    pub call_id: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Output content
    pub output: String,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub duration_ms: u64,
}

/// A tool call to execute
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Unique call ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: Value,
}

/// Parallel tool executor with dependency analysis
pub struct ParallelToolExecutor {
    /// Maximum concurrent executions
    max_concurrent: usize,
    /// Semaphore for limiting concurrency
    semaphore: Arc<Semaphore>,
}

impl ParallelToolExecutor {
    /// Create new executor with specified concurrency limit
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Execute multiple tool calls with automatic parallelization
    ///
    /// Context and Registry are passed as Arc for safe sharing across spawn tasks.
    pub async fn execute(
        &self,
        ctx: Arc<RuntimeContext>,
        registry: Arc<ToolRegistry>,
        calls: &[ToolCall],
    ) -> Vec<ToolExecutionResult> {
        if calls.is_empty() {
            return Vec::new();
        }

        if calls.len() == 1 {
            // Single call, no parallelization needed
            return vec![self.execute_single(ctx.as_ref(), registry.as_ref(), &calls[0]).await];
        }

        // Build dependency graph
        let graph = self.build_dependency_graph(calls);

        // Get execution levels (topological sort)
        let levels = graph.topological_levels();

        info!(
            "Executing {} tool calls in {} parallel levels",
            calls.len(),
            levels.len()
        );

        let mut all_results = Vec::with_capacity(calls.len());

        // Execute each level in parallel
        for (level_idx, level) in levels.into_iter().enumerate() {
            debug!(
                "Executing level {} with {} calls",
                level_idx,
                level.len()
            );

            let level_calls: Vec<&ToolCall> = level.iter().map(|&i| &calls[i]).collect();

            let level_results = self
                .execute_level(Arc::clone(&ctx), Arc::clone(&registry), &level_calls)
                .await;

            all_results.extend(level_results);
        }

        all_results
    }

    /// Execute a single tool call
    async fn execute_single(
        &self,
        ctx: &dyn ToolContext,
        registry: &ToolRegistry,
        call: &ToolCall,
    ) -> ToolExecutionResult {
        let start = std::time::Instant::now();

        match registry.get(&call.name) {
            Some(tool) => {
                // Execute: (input: Value, context: &dyn ToolContext)
                match tool.execute(call.arguments.clone(), ctx).await {
                    Ok(result) => ToolExecutionResult {
                        tool_name: call.name.clone(),
                        call_id: call.id.clone(),
                        success: result.success,
                        output: result.output,
                        error: result.error,
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(e) => ToolExecutionResult {
                        tool_name: call.name.clone(),
                        call_id: call.id.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(e.to_string()),
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }
            None => ToolExecutionResult {
                tool_name: call.name.clone(),
                call_id: call.id.clone(),
                success: false,
                output: String::new(),
                error: Some(format!("Tool '{}' not found", call.name)),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    /// Execute a level of calls in parallel
    async fn execute_level(
        &self,
        ctx: Arc<RuntimeContext>,
        registry: Arc<ToolRegistry>,
        calls: &[&ToolCall],
    ) -> Vec<ToolExecutionResult> {
        let mut handles = Vec::with_capacity(calls.len());

        for call in calls {
            let ctx = Arc::clone(&ctx);
            let registry = Arc::clone(&registry);
            let call = (*call).clone();
            let semaphore = Arc::clone(&self.semaphore);

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let start = std::time::Instant::now();

                match registry.get(&call.name) {
                    Some(tool) => {
                        // Execute: (input: Value, context: &dyn ToolContext)
                        match tool.execute(call.arguments.clone(), ctx.as_ref()).await {
                            Ok(result) => ToolExecutionResult {
                                tool_name: call.name.clone(),
                                call_id: call.id.clone(),
                                success: result.success,
                                output: result.output,
                                error: result.error,
                                duration_ms: start.elapsed().as_millis() as u64,
                            },
                            Err(e) => ToolExecutionResult {
                                tool_name: call.name.clone(),
                                call_id: call.id.clone(),
                                success: false,
                                output: String::new(),
                                error: Some(e.to_string()),
                                duration_ms: start.elapsed().as_millis() as u64,
                            },
                        }
                    }
                    None => ToolExecutionResult {
                        tool_name: call.name.clone(),
                        call_id: call.id.clone(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("Tool '{}' not found", call.name)),
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                }
            });

            handles.push(handle);
        }

        // Collect results
        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Task panicked: {}", e);
                }
            }
        }

        results
    }

    /// Build dependency graph for tool calls
    fn build_dependency_graph(&self, calls: &[ToolCall]) -> DependencyGraph {
        let mut graph = DependencyGraph::new(calls.len());
        let mut written_paths: HashMap<String, usize> = HashMap::new();
        let mut read_paths: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, call) in calls.iter().enumerate() {
            let paths = self.extract_paths(call);
            let is_write = self.is_write_operation(&call.name);

            for path in paths {
                // Write-after-write dependency
                if is_write {
                    if let Some(&prev_writer) = written_paths.get(&path) {
                        graph.add_edge(prev_writer, i);
                    }
                    written_paths.insert(path.clone(), i);
                }

                // Read-after-write dependency
                if !is_write {
                    if let Some(&writer) = written_paths.get(&path) {
                        graph.add_edge(writer, i);
                    }
                    read_paths.entry(path.clone()).or_default().push(i);
                }

                // Write-after-read dependency
                if is_write {
                    if let Some(readers) = read_paths.get(&path) {
                        for &reader in readers {
                            if reader != i {
                                graph.add_edge(reader, i);
                            }
                        }
                    }
                }
            }
        }

        graph
    }

    /// Extract file paths from tool arguments
    fn extract_paths(&self, call: &ToolCall) -> Vec<String> {
        let mut paths = Vec::new();

        if let Some(obj) = call.arguments.as_object() {
            // Common path field names
            for field in &["path", "file_path", "file", "directory", "dir", "target"] {
                if let Some(Value::String(p)) = obj.get(*field) {
                    paths.push(p.clone());
                }
            }

            // Array of paths
            if let Some(Value::Array(arr)) = obj.get("paths") {
                for v in arr {
                    if let Value::String(p) = v {
                        paths.push(p.clone());
                    }
                }
            }
        }

        paths
    }

    /// Check if tool is a write operation
    fn is_write_operation(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "write" | "edit" | "bash" | "shell" | "create_file" | "delete" | "move" | "copy"
        )
    }
}

impl Default for ParallelToolExecutor {
    fn default() -> Self {
        Self::new(4)
    }
}

/// Dependency graph for tool calls
#[derive(Debug)]
struct DependencyGraph {
    /// Number of nodes
    size: usize,
    /// Edges: from -> [to]
    edges: Vec<Vec<usize>>,
    /// In-degree for each node
    in_degree: Vec<usize>,
}

impl DependencyGraph {
    fn new(size: usize) -> Self {
        Self {
            size,
            edges: vec![Vec::new(); size],
            in_degree: vec![0; size],
        }
    }

    fn add_edge(&mut self, from: usize, to: usize) {
        if from < self.size && to < self.size && from != to {
            if !self.edges[from].contains(&to) {
                self.edges[from].push(to);
                self.in_degree[to] += 1;
            }
        }
    }

    /// Topological sort returning execution levels
    fn topological_levels(&self) -> Vec<Vec<usize>> {
        let mut levels = Vec::new();
        let mut in_degree = self.in_degree.clone();
        let mut remaining: HashSet<usize> = (0..self.size).collect();

        while !remaining.is_empty() {
            // Find all nodes with in_degree 0
            let level: Vec<usize> = remaining
                .iter()
                .filter(|&&i| in_degree[i] == 0)
                .copied()
                .collect();

            if level.is_empty() {
                // Cycle detected, just run remaining sequentially
                warn!("Dependency cycle detected, falling back to sequential execution");
                levels.push(remaining.into_iter().collect());
                break;
            }

            // Remove from remaining and update in_degrees
            for &node in &level {
                remaining.remove(&node);
                for &neighbor in &self.edges[node] {
                    if in_degree[neighbor] > 0 {
                        in_degree[neighbor] -= 1;
                    }
                }
            }

            levels.push(level);
        }

        levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_graph() {
        let mut graph = DependencyGraph::new(4);

        // 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
        graph.add_edge(0, 1);
        graph.add_edge(0, 2);
        graph.add_edge(1, 3);
        graph.add_edge(2, 3);

        let levels = graph.topological_levels();

        // Level 0: [0]
        // Level 1: [1, 2]
        // Level 2: [3]
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![0]);
        assert!(levels[1].contains(&1) && levels[1].contains(&2));
        assert_eq!(levels[2], vec![3]);
    }

    #[test]
    fn test_extract_paths() {
        let executor = ParallelToolExecutor::new(4);

        let call = ToolCall {
            id: "1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        };

        let paths = executor.extract_paths(&call);
        assert_eq!(paths, vec!["/tmp/test.txt"]);
    }

    #[test]
    fn test_write_operation_detection() {
        let executor = ParallelToolExecutor::new(4);

        assert!(executor.is_write_operation("write"));
        assert!(executor.is_write_operation("edit"));
        assert!(executor.is_write_operation("bash"));
        assert!(!executor.is_write_operation("read"));
        assert!(!executor.is_write_operation("glob"));
    }

    #[test]
    fn test_parallel_read_operations() {
        let executor = ParallelToolExecutor::new(4);

        // Multiple reads should be in same level (parallel)
        let calls = vec![
            ToolCall {
                id: "1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": "a.txt"}),
            },
            ToolCall {
                id: "2".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": "b.txt"}),
            },
            ToolCall {
                id: "3".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": "c.txt"}),
            },
        ];

        let graph = executor.build_dependency_graph(&calls);
        let levels = graph.topological_levels();

        // All reads can be parallel
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].len(), 3);
    }

    #[test]
    fn test_write_dependency() {
        let executor = ParallelToolExecutor::new(4);

        // Write then read same file should be sequential
        let calls = vec![
            ToolCall {
                id: "1".to_string(),
                name: "write".to_string(),
                arguments: serde_json::json!({"path": "a.txt", "content": "hello"}),
            },
            ToolCall {
                id: "2".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": "a.txt"}),
            },
        ];

        let graph = executor.build_dependency_graph(&calls);
        let levels = graph.topological_levels();

        // Write before read
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], vec![0]);
        assert_eq!(levels[1], vec![1]);
    }
}
