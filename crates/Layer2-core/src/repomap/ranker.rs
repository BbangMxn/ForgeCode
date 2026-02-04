//! File Ranker - 파일 중요도 랭킹
//!
//! PageRank 알고리즘과 휴리스틱을 사용하여 파일의 중요도를 계산합니다.
//! 이를 통해 토큰 예산 내에서 가장 관련 있는 파일을 선택합니다.

use super::graph::DependencyGraph;
use super::types::{FileInfo, RepoMap, SymbolKind};
use std::collections::HashMap;
use std::path::PathBuf;

/// 파일 랭커
pub struct FileRanker {
    /// 감쇠 계수 (PageRank)
    damping_factor: f64,
    /// 최대 반복 횟수
    max_iterations: usize,
    /// 수렴 임계값
    convergence_threshold: f64,
}

impl FileRanker {
    /// 새 랭커 생성
    pub fn new() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            convergence_threshold: 1e-6,
        }
    }

    /// 커스텀 파라미터로 생성
    pub fn with_params(damping_factor: f64, max_iterations: usize) -> Self {
        Self {
            damping_factor,
            max_iterations,
            convergence_threshold: 1e-6,
        }
    }

    /// 파일 중요도 계산 및 업데이트
    pub fn rank(&self, map: &mut RepoMap, graph: &DependencyGraph, focus_files: &[PathBuf]) {
        // 1. PageRank 기반 점수
        let pagerank_scores = self.compute_pagerank(map, graph);

        // 2. 휴리스틱 점수
        let heuristic_scores = self.compute_heuristics(map);

        // 3. 포커스 파일 근접도 점수
        let proximity_scores = self.compute_proximity(map, graph, focus_files);

        // 4. 최종 점수 계산 (가중 합)
        for file in &mut map.files {
            let path = &file.path;

            let pr_score = pagerank_scores.get(path).copied().unwrap_or(0.0);
            let hr_score = heuristic_scores.get(path).copied().unwrap_or(0.0);
            let px_score = proximity_scores.get(path).copied().unwrap_or(0.0);

            // 가중치: PageRank 30%, 휴리스틱 30%, 근접도 40%
            file.importance_score = pr_score * 0.3 + hr_score * 0.3 + px_score * 0.4;
        }

        // 정규화 (0-1 범위)
        self.normalize_scores(map);
    }

    /// 컨텍스트 파일 기반 랭킹 (현재 작업 중인 파일 기준)
    pub fn rank_for_context(
        &self,
        map: &mut RepoMap,
        graph: &DependencyGraph,
        context_files: &[PathBuf],
    ) {
        // 포커스 파일을 현재 작업 컨텍스트로 설정
        self.rank(map, graph, context_files);
    }

    /// PageRank 계산
    fn compute_pagerank(&self, map: &RepoMap, graph: &DependencyGraph) -> HashMap<PathBuf, f64> {
        let n = map.files.len() as f64;
        if n == 0.0 {
            return HashMap::new();
        }

        let mut scores: HashMap<PathBuf, f64> = map
            .files
            .iter()
            .map(|f| (f.path.clone(), 1.0 / n))
            .collect();

        for _ in 0..self.max_iterations {
            let mut new_scores = HashMap::new();
            let mut max_diff = 0.0_f64;

            for file in &map.files {
                let path = &file.path;

                // 이 파일을 가리키는 파일들의 점수 합산
                let incoming_score: f64 = graph
                    .dependents(path)
                    .iter()
                    .map(|dep| {
                        let dep_score = scores.get(*dep).copied().unwrap_or(0.0);
                        let out_degree = graph.dependencies(*dep).len() as f64;
                        if out_degree > 0.0 {
                            dep_score / out_degree
                        } else {
                            0.0
                        }
                    })
                    .sum();

                let new_score =
                    (1.0 - self.damping_factor) / n + self.damping_factor * incoming_score;

                let old_score = scores.get(path).copied().unwrap_or(0.0);
                max_diff = max_diff.max((new_score - old_score).abs());

                new_scores.insert(path.clone(), new_score);
            }

            scores = new_scores;

            // 수렴 체크
            if max_diff < self.convergence_threshold {
                break;
            }
        }

        scores
    }

    /// 휴리스틱 점수 계산
    fn compute_heuristics(&self, map: &RepoMap) -> HashMap<PathBuf, f64> {
        let mut scores = HashMap::new();

        for file in &map.files {
            let mut score = 0.0;

            // 1. 심볼 수 (복잡도)
            let symbol_count = file.symbols.len() as f64;
            score += (symbol_count.ln() + 1.0).min(3.0) / 3.0;

            // 2. public 심볼 비율 (API 노출도)
            let public_count = file
                .symbols
                .iter()
                .filter(|s| s.visibility.as_deref() == Some("pub"))
                .count() as f64;
            if symbol_count > 0.0 {
                score += public_count / symbol_count * 0.5;
            }

            // 3. 중요한 심볼 타입 (struct, trait, enum)
            let important_types = file
                .symbols
                .iter()
                .filter(|s| {
                    matches!(
                        s.kind,
                        SymbolKind::Struct
                            | SymbolKind::Interface
                            | SymbolKind::Enum
                            | SymbolKind::Class
                    )
                })
                .count() as f64;
            score += important_types * 0.2;

            // 4. 파일 이름 휴리스틱
            let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // 진입점/핵심 파일
            if file_name == "lib.rs"
                || file_name == "mod.rs"
                || file_name == "main.rs"
                || file_name == "index.ts"
                || file_name == "index.js"
                || file_name == "__init__.py"
            {
                score += 0.5;
            }

            // 테스트/예제 파일은 낮은 점수
            if file_name.contains("test")
                || file_name.contains("spec")
                || file_name.contains("example")
            {
                score *= 0.5;
            }

            // 5. 파일 크기 (너무 크면 감점)
            let line_factor = if file.line_count > 1000 {
                0.8
            } else if file.line_count > 500 {
                0.9
            } else {
                1.0
            };
            score *= line_factor;

            scores.insert(file.path.clone(), score.min(1.0));
        }

        scores
    }

    /// 포커스 파일 근접도 계산
    fn compute_proximity(
        &self,
        map: &RepoMap,
        graph: &DependencyGraph,
        focus_files: &[PathBuf],
    ) -> HashMap<PathBuf, f64> {
        let mut scores = HashMap::new();

        if focus_files.is_empty() {
            // 포커스 파일이 없으면 균등 점수
            for file in &map.files {
                scores.insert(file.path.clone(), 0.5);
            }
            return scores;
        }

        // 포커스 파일은 최고 점수
        for focus in focus_files {
            scores.insert(focus.clone(), 1.0);
        }

        // BFS로 거리 계산
        for focus in focus_files {
            let chain = graph.dependency_chain(focus, 5);

            for (distance, path) in chain.iter().enumerate() {
                let distance_score = 1.0 / (distance as f64 + 1.0);
                let entry = scores.entry(path.clone()).or_insert(0.0);
                *entry = (*entry).max(distance_score);
            }

            // 역방향 (이 파일을 사용하는 파일들)
            for dependent in graph.dependents(focus) {
                let entry = scores.entry(dependent.clone()).or_insert(0.0);
                *entry = (*entry).max(0.8);
            }
        }

        // 점수가 없는 파일은 낮은 점수
        for file in &map.files {
            scores.entry(file.path.clone()).or_insert(0.1);
        }

        scores
    }

    /// 점수 정규화 (0-1 범위)
    fn normalize_scores(&self, map: &mut RepoMap) {
        let max_score = map
            .files
            .iter()
            .map(|f| f.importance_score)
            .fold(0.0_f64, |a, b| a.max(b));

        if max_score > 0.0 {
            for file in &mut map.files {
                file.importance_score /= max_score;
            }
        }
    }
}

impl Default for FileRanker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::SymbolDef;
    use super::*;

    #[test]
    fn test_ranker_creation() {
        let ranker = FileRanker::new();
        assert!((ranker.damping_factor - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_heuristic_scores() {
        let ranker = FileRanker::new();

        let mut map = RepoMap::new(PathBuf::from("/project"));

        // lib.rs - should have higher score
        let mut lib_file = FileInfo::new(
            PathBuf::from("/project/src/lib.rs"),
            "src/lib.rs".to_string(),
            "rust".to_string(),
        );
        lib_file
            .add_symbol(SymbolDef::new("MyStruct", SymbolKind::Struct, 1).with_visibility("pub"));
        map.add_file(lib_file);

        // test.rs - should have lower score
        let mut test_file = FileInfo::new(
            PathBuf::from("/project/src/test.rs"),
            "src/test.rs".to_string(),
            "rust".to_string(),
        );
        test_file.add_symbol(SymbolDef::new("test_func", SymbolKind::Function, 1));
        map.add_file(test_file);

        let scores = ranker.compute_heuristics(&map);

        let lib_score = scores
            .get(&PathBuf::from("/project/src/lib.rs"))
            .copied()
            .unwrap_or(0.0);
        let test_score = scores
            .get(&PathBuf::from("/project/src/test.rs"))
            .copied()
            .unwrap_or(0.0);

        assert!(lib_score > test_score);
    }
}
