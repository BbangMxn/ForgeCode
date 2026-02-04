//! Dependency Graph - 파일 간 의존성 그래프
//!
//! 파일 간의 import/export 관계를 분석하여 의존성 그래프를 생성합니다.

use super::types::{FileInfo, RepoMap};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// 의존성 그래프
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// 노드 (파일 경로)
    nodes: HashSet<PathBuf>,
    /// 엣지 (from -> to)
    edges: HashMap<PathBuf, HashSet<PathBuf>>,
    /// 역방향 엣지 (to -> from)
    reverse_edges: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl DependencyGraph {
    /// 새 그래프 생성
    pub fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
        }
    }

    /// RepoMap에서 그래프 생성
    pub fn from_repo_map(map: &RepoMap) -> Self {
        let mut graph = Self::new();

        // 모든 파일을 노드로 추가
        for file in &map.files {
            graph.add_node(file.path.clone());
        }

        // 심볼 기반 의존성 추론
        for file in &map.files {
            for import in &file.imports {
                // 임포트에서 파일 경로 추론 시도
                if let Some(target) = graph.resolve_import(import, &map.files) {
                    graph.add_edge(file.path.clone(), target);
                }
            }
        }

        graph
    }

    /// 노드 추가
    pub fn add_node(&mut self, path: PathBuf) {
        self.nodes.insert(path);
    }

    /// 엣지 추가
    pub fn add_edge(&mut self, from: PathBuf, to: PathBuf) {
        self.edges
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
        self.reverse_edges.entry(to).or_default().insert(from);
    }

    /// 특정 파일의 의존성 조회
    pub fn dependencies(&self, path: &PathBuf) -> Vec<&PathBuf> {
        self.edges
            .get(path)
            .map(|deps| deps.iter().collect())
            .unwrap_or_default()
    }

    /// 특정 파일을 의존하는 파일들 조회
    pub fn dependents(&self, path: &PathBuf) -> Vec<&PathBuf> {
        self.reverse_edges
            .get(path)
            .map(|deps| deps.iter().collect())
            .unwrap_or_default()
    }

    /// 관련 파일 조회 (의존성 + 의존자)
    pub fn related_files(&self, path: &PathBuf) -> Vec<&PathBuf> {
        let mut related = HashSet::new();

        for dep in self.dependencies(path) {
            related.insert(dep);
        }
        for dep in self.dependents(path) {
            related.insert(dep);
        }

        related.into_iter().collect()
    }

    /// 의존성 체인 (깊이 우선 탐색)
    pub fn dependency_chain(&self, path: &PathBuf, max_depth: usize) -> Vec<PathBuf> {
        let mut visited = HashSet::new();
        let mut chain = Vec::new();
        self.dfs_dependencies(path, &mut visited, &mut chain, 0, max_depth);
        chain
    }

    fn dfs_dependencies(
        &self,
        path: &PathBuf,
        visited: &mut HashSet<PathBuf>,
        chain: &mut Vec<PathBuf>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth >= max_depth || visited.contains(path) {
            return;
        }

        visited.insert(path.clone());
        chain.push(path.clone());

        if let Some(deps) = self.edges.get(path) {
            for dep in deps {
                self.dfs_dependencies(dep, visited, chain, depth + 1, max_depth);
            }
        }
    }

    /// 노드 수
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 엣지 수
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|e| e.len()).sum()
    }

    /// 임포트에서 파일 경로 추론
    fn resolve_import(&self, import: &str, files: &[FileInfo]) -> Option<PathBuf> {
        // Rust 모듈 경로 (crate::module::item)
        if import.starts_with("crate::") || import.starts_with("super::") {
            let module_path = import
                .replace("crate::", "")
                .replace("super::", "../")
                .replace("::", "/");

            for file in files {
                if file.relative_path.contains(&module_path)
                    || file
                        .relative_path
                        .replace(".rs", "")
                        .ends_with(&module_path)
                {
                    return Some(file.path.clone());
                }
            }
        }

        // Python 임포트 (from module import item)
        if import.starts_with("from ") || import.starts_with("import ") {
            let module = import
                .replace("from ", "")
                .replace("import ", "")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .replace('.', "/");

            for file in files {
                if file.relative_path.contains(&module)
                    || file.relative_path.replace(".py", "").ends_with(&module)
                {
                    return Some(file.path.clone());
                }
            }
        }

        // JavaScript/TypeScript 임포트 (import X from './module')
        if import.contains("from ") {
            let parts: Vec<&str> = import.split("from ").collect();
            if parts.len() >= 2 {
                let module_path = parts[1]
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"' || c == ';')
                    .replace("./", "")
                    .replace("../", "");

                for file in files {
                    let file_stem = file.relative_path.replace(".ts", "").replace(".js", "");
                    if file_stem.ends_with(&module_path) || file_stem.contains(&module_path) {
                        return Some(file.path.clone());
                    }
                }
            }
        }

        None
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_operations() {
        let mut graph = DependencyGraph::new();

        let file_a = PathBuf::from("/src/a.rs");
        let file_b = PathBuf::from("/src/b.rs");
        let file_c = PathBuf::from("/src/c.rs");

        graph.add_node(file_a.clone());
        graph.add_node(file_b.clone());
        graph.add_node(file_c.clone());

        graph.add_edge(file_a.clone(), file_b.clone());
        graph.add_edge(file_a.clone(), file_c.clone());

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        let deps = graph.dependencies(&file_a);
        assert_eq!(deps.len(), 2);

        let dependents = graph.dependents(&file_b);
        assert_eq!(dependents.len(), 1);
    }

    #[test]
    fn test_dependency_chain() {
        let mut graph = DependencyGraph::new();

        let file_a = PathBuf::from("/src/a.rs");
        let file_b = PathBuf::from("/src/b.rs");
        let file_c = PathBuf::from("/src/c.rs");

        graph.add_node(file_a.clone());
        graph.add_node(file_b.clone());
        graph.add_node(file_c.clone());

        graph.add_edge(file_a.clone(), file_b.clone());
        graph.add_edge(file_b.clone(), file_c.clone());

        let chain = graph.dependency_chain(&file_a, 3);
        assert_eq!(chain.len(), 3);
    }
}
