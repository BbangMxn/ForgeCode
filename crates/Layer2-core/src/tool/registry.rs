//! Tool Registry - 도구 등록 및 관리
//!
//! Agent가 사용하는 모든 도구를 관리합니다.
//!
//! ## 기능
//! - 도구 등록/조회
//! - Builtin 도구 자동 등록
//! - MCP 도구 통합 (TODO)
//!
//! ## Layer1 연동
//! - `Tool` trait으로 모든 도구 통합

use super::builtin;
use forge_foundation::Tool;
use std::collections::HashMap;
use std::sync::Arc;

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
