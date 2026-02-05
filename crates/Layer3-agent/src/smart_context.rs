//! Smart Context Management - 2025 최신 기술 적용
//!
//! Claude Opus 4.5 스타일의 효율적인 컨텍스트 관리:
//! - 65% 토큰 절약 목표
//! - 관련성 기반 컨텍스트 선택
//! - 자동 요약 및 압축
//!
//! ## 핵심 개념
//!
//! ### 1. Context Slicing
//! 전체 컨텍스트를 한 번에 주입하지 않고, 필요한 부분만 슬라이스:
//! ```text
//! Full Context (100K tokens) → Relevant Slice (20K tokens)
//! ```
//!
//! ### 2. Relevance Scoring
//! 현재 작업과의 관련성을 점수화:
//! - 파일 경로 유사도
//! - 심볼 참조 관계
//! - 최근 수정/접근
//!
//! ### 3. Progressive Detail
//! 점진적으로 상세 정보 제공:
//! - Level 1: 파일 목록 + 요약
//! - Level 2: 함수/클래스 시그니처
//! - Level 3: 전체 구현

use std::collections::HashMap;
use std::path::Path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 컨텍스트 아이템의 관련성 점수
#[derive(Debug, Clone, Copy, Default)]
pub struct RelevanceScore {
    /// 경로 유사도 (0.0 - 1.0)
    pub path_similarity: f32,
    /// 심볼 참조 점수 (0.0 - 1.0)
    pub symbol_reference: f32,
    /// 시간 가중치 (최근 접근 = 높음)
    pub recency: f32,
    /// 명시적 언급 점수
    pub explicit_mention: f32,
    /// 종합 점수
    pub total: f32,
}

impl RelevanceScore {
    pub fn calculate(
        path_sim: f32,
        symbol_ref: f32,
        recency: f32,
        explicit: f32,
    ) -> Self {
        // 가중치 적용 종합 점수
        let total = path_sim * 0.2 + symbol_ref * 0.3 + recency * 0.2 + explicit * 0.3;
        Self {
            path_similarity: path_sim,
            symbol_reference: symbol_ref,
            recency,
            explicit_mention: explicit,
            total,
        }
    }
}

/// 컨텍스트 상세 레벨
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailLevel {
    /// 요약만 (파일명 + 한 줄 설명)
    Summary,
    /// 시그니처 (함수/클래스 선언부)
    Signature,
    /// 전체 내용
    Full,
}

/// 컨텍스트 아이템
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// 고유 ID (보통 파일 경로)
    pub id: String,
    /// 컨텍스트 종류
    pub kind: ContextItemKind,
    /// 요약
    pub summary: String,
    /// 시그니처 (함수/클래스 선언부)
    pub signatures: Vec<String>,
    /// 전체 내용
    pub full_content: Option<String>,
    /// 토큰 수 (추정)
    pub estimated_tokens: TokenCounts,
    /// 마지막 접근 시간
    pub last_accessed: DateTime<Utc>,
    /// 관련성 점수 (계산됨)
    #[serde(skip)]
    pub relevance: RelevanceScore,
}

/// 토큰 수 추정
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenCounts {
    pub summary: usize,
    pub signatures: usize,
    pub full: usize,
}

impl TokenCounts {
    pub fn for_level(&self, level: DetailLevel) -> usize {
        match level {
            DetailLevel::Summary => self.summary,
            DetailLevel::Signature => self.summary + self.signatures,
            DetailLevel::Full => self.full,
        }
    }
}

/// 컨텍스트 아이템 종류
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContextItemKind {
    /// 소스 파일
    SourceFile { language: String },
    /// 설정 파일
    ConfigFile,
    /// 문서
    Documentation,
    /// Git diff
    GitDiff,
    /// 이전 대화
    ConversationHistory,
    /// 도구 결과
    ToolResult,
}

/// Smart Context Manager
#[derive(Debug, Default)]
pub struct SmartContextManager {
    /// 모든 컨텍스트 아이템
    items: HashMap<String, ContextItem>,
    /// 최대 토큰 예산
    max_tokens: usize,
    /// 현재 작업 컨텍스트 (파일 경로, 심볼 등)
    current_focus: Vec<String>,
}

impl SmartContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            items: HashMap::new(),
            max_tokens,
            current_focus: Vec::new(),
        }
    }

    /// 컨텍스트 아이템 추가
    pub fn add_item(&mut self, item: ContextItem) {
        self.items.insert(item.id.clone(), item);
    }

    /// 현재 포커스 설정 (관련성 계산에 사용)
    pub fn set_focus(&mut self, focus: Vec<String>) {
        self.current_focus = focus;
        self.recalculate_relevance();
    }

    /// 관련성 재계산
    fn recalculate_relevance(&mut self) {
        // 먼저 계산에 필요한 정보 수집
        let updates: Vec<(String, f32, f32, f32, f32)> = self.items.values()
            .map(|item| {
                let path_sim = Self::calc_path_similarity(&self.current_focus, &item.id);
                let symbol_ref = 0.5; // 간단한 기본값
                let recency = Self::calc_recency(&item.last_accessed);
                let explicit = if self.current_focus.contains(&item.id) { 1.0 } else { 0.0 };
                (item.id.clone(), path_sim, symbol_ref, recency, explicit)
            })
            .collect();

        // 계산된 값으로 업데이트
        for (id, path_sim, symbol_ref, recency, explicit) in updates {
            if let Some(item) = self.items.get_mut(&id) {
                item.relevance = RelevanceScore::calculate(path_sim, symbol_ref, recency, explicit);
            }
        }
    }

    /// 경로 유사도 계산 (static)
    fn calc_path_similarity(focus: &[String], item_id: &str) -> f32 {
        if focus.is_empty() {
            return 0.5;
        }

        let item_path = Path::new(item_id);
        let mut max_sim = 0.0f32;

        for f in focus {
            let focus_path = Path::new(f);
            if item_path.parent() == focus_path.parent() {
                max_sim = max_sim.max(0.8);
            } else if item_path.ancestors().any(|a| focus_path.starts_with(a)) {
                max_sim = max_sim.max(0.5);
            }
        }

        max_sim
    }

    /// 최근성 계산 (static)
    fn calc_recency(last_accessed: &DateTime<Utc>) -> f32 {
        let now = Utc::now();
        let duration = now.signed_duration_since(*last_accessed);
        let hours = duration.num_hours() as f32;
        (1.0 - (hours / 24.0).min(0.9)).max(0.1)
    }

    /// 최적의 컨텍스트 슬라이스 생성
    pub fn get_optimal_slice(&self) -> ContextSlice {
        let mut items: Vec<_> = self.items.values().collect();

        // 관련성 점수로 정렬 (높은 순)
        items.sort_by(|a, b| {
            b.relevance.total.partial_cmp(&a.relevance.total).unwrap()
        });

        let mut slice = ContextSlice::new(self.max_tokens);

        for item in items {
            // 토큰 예산 내에서 가능한 상세 레벨 결정
            let remaining = slice.remaining_tokens();

            let level = if remaining >= item.estimated_tokens.full {
                // 관련성 높으면 Full, 아니면 Signature
                if item.relevance.total > 0.7 {
                    DetailLevel::Full
                } else if remaining >= item.estimated_tokens.signatures + item.estimated_tokens.summary {
                    DetailLevel::Signature
                } else {
                    DetailLevel::Summary
                }
            } else if remaining >= item.estimated_tokens.signatures + item.estimated_tokens.summary {
                DetailLevel::Signature
            } else if remaining >= item.estimated_tokens.summary {
                DetailLevel::Summary
            } else {
                continue; // 토큰 부족
            };

            slice.add_item(item.clone(), level);
        }

        slice
    }

    /// 특정 아이템의 상세 내용 요청
    pub fn get_item_detail(&self, id: &str, level: DetailLevel) -> Option<String> {
        let item = self.items.get(id)?;

        match level {
            DetailLevel::Summary => Some(item.summary.clone()),
            DetailLevel::Signature => {
                let mut result = item.summary.clone();
                if !item.signatures.is_empty() {
                    result.push_str("\n\n");
                    result.push_str(&item.signatures.join("\n"));
                }
                Some(result)
            }
            DetailLevel::Full => item.full_content.clone(),
        }
    }
}

/// 컨텍스트 슬라이스 - 실제 LLM에 전달할 컨텍스트
#[derive(Debug, Clone)]
pub struct ContextSlice {
    pub items: Vec<(ContextItem, DetailLevel)>,
    pub max_tokens: usize,
    pub used_tokens: usize,
}

impl ContextSlice {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            items: Vec::new(),
            max_tokens,
            used_tokens: 0,
        }
    }

    pub fn remaining_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.used_tokens)
    }

    pub fn add_item(&mut self, item: ContextItem, level: DetailLevel) {
        let tokens = item.estimated_tokens.for_level(level);
        if self.used_tokens + tokens <= self.max_tokens {
            self.used_tokens += tokens;
            self.items.push((item, level));
        }
    }

    /// 프롬프트 형식으로 변환
    pub fn format_for_prompt(&self) -> String {
        let mut output = String::from("## Project Context\n\n");

        for (item, level) in &self.items {
            output.push_str(&format!("### {}\n", item.id));

            match level {
                DetailLevel::Summary => {
                    output.push_str(&item.summary);
                }
                DetailLevel::Signature => {
                    output.push_str(&item.summary);
                    if !item.signatures.is_empty() {
                        output.push_str("\n\n**Signatures:**\n```\n");
                        output.push_str(&item.signatures.join("\n"));
                        output.push_str("\n```");
                    }
                }
                DetailLevel::Full => {
                    if let Some(content) = &item.full_content {
                        output.push_str(content);
                    } else {
                        output.push_str(&item.summary);
                    }
                }
            }

            output.push_str("\n\n");
        }

        output.push_str(&format!(
            "---\n*Context: {} items, ~{} tokens*\n",
            self.items.len(),
            self.used_tokens
        ));

        output
    }

    /// 토큰 효율성 통계
    pub fn efficiency_stats(&self) -> ContextEfficiency {
        let full_tokens: usize = self.items.iter()
            .map(|(item, _)| item.estimated_tokens.full)
            .sum();

        ContextEfficiency {
            items_included: self.items.len(),
            tokens_used: self.used_tokens,
            tokens_max: self.max_tokens,
            tokens_saved: full_tokens.saturating_sub(self.used_tokens),
            efficiency_percent: if full_tokens > 0 {
                ((full_tokens - self.used_tokens) as f32 / full_tokens as f32 * 100.0) as u32
            } else {
                0
            },
        }
    }
}

/// 컨텍스트 효율성 통계
#[derive(Debug, Clone)]
pub struct ContextEfficiency {
    pub items_included: usize,
    pub tokens_used: usize,
    pub tokens_max: usize,
    pub tokens_saved: usize,
    pub efficiency_percent: u32,
}

/// 파일에서 컨텍스트 아이템 생성
pub fn context_from_file(path: &str, content: &str) -> ContextItem {
    let lines: Vec<&str> = content.lines().collect();
    let _line_count = lines.len(); // 추후 메타데이터에 사용 가능

    // 요약 생성 (첫 몇 줄 또는 문서 주석)
    let summary = extract_summary(&lines);

    // 시그니처 추출
    let signatures = extract_signatures(content, path);

    // 토큰 추정 (대략 4자 = 1토큰)
    let full_tokens = content.len() / 4;
    let sig_tokens = signatures.join("\n").len() / 4;
    let sum_tokens = summary.len() / 4;

    // 언어 감지
    let language = detect_language(path);

    ContextItem {
        id: path.to_string(),
        kind: ContextItemKind::SourceFile { language },
        summary,
        signatures,
        full_content: Some(content.to_string()),
        estimated_tokens: TokenCounts {
            summary: sum_tokens,
            signatures: sig_tokens,
            full: full_tokens,
        },
        last_accessed: Utc::now(),
        relevance: RelevanceScore::default(),
    }
}

/// 요약 추출
fn extract_summary(lines: &[&str]) -> String {
    // 문서 주석 찾기
    let mut summary_lines = Vec::new();

    for line in lines.iter().take(20) {
        let trimmed = line.trim();
        if trimmed.starts_with("//!") || trimmed.starts_with("///") {
            summary_lines.push(trimmed.trim_start_matches("//!").trim_start_matches("///").trim());
        } else if trimmed.starts_with("#") && !trimmed.starts_with("#[") {
            // Markdown 헤더 또는 Python 주석
            summary_lines.push(trimmed.trim_start_matches('#').trim());
        } else if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            // Python docstring
            summary_lines.push(trimmed.trim_matches('"').trim_matches('\'').trim());
        }
    }

    if summary_lines.is_empty() {
        // 요약이 없으면 첫 줄 사용
        lines.first().map(|s| s.to_string()).unwrap_or_default()
    } else {
        summary_lines.join(" ")
    }
}

/// 시그니처 추출 (함수, 클래스, 구조체 등)
fn extract_signatures(content: &str, path: &str) -> Vec<String> {
    let mut signatures = Vec::new();
    let ext = Path::new(path).extension().and_then(|s| s.to_str()).unwrap_or("");

    match ext {
        "rs" => {
            // Rust: pub fn, struct, enum, impl
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("pub struct ")
                    || trimmed.starts_with("pub enum ")
                    || trimmed.starts_with("impl ")
                    || trimmed.starts_with("pub trait ")
                {
                    // 한 줄로 정리
                    let sig = trimmed.split('{').next().unwrap_or(trimmed).trim();
                    if sig.len() < 200 {
                        signatures.push(sig.to_string());
                    }
                }
            }
        }
        "py" => {
            // Python: def, class
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("def ") || trimmed.starts_with("class ") {
                    let sig = trimmed.split(':').next().unwrap_or(trimmed).trim();
                    if sig.len() < 200 {
                        signatures.push(sig.to_string());
                    }
                }
            }
        }
        "ts" | "js" => {
            // TypeScript/JavaScript: function, class, export
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("function ")
                    || trimmed.starts_with("class ")
                    || trimmed.starts_with("export ")
                    || trimmed.contains("const ") && trimmed.contains(" = ")
                {
                    let sig = trimmed.split('{').next().unwrap_or(trimmed).trim();
                    if sig.len() < 200 {
                        signatures.push(sig.to_string());
                    }
                }
            }
        }
        "go" => {
            // Go: func, type
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("func ") || trimmed.starts_with("type ") {
                    let sig = trimmed.split('{').next().unwrap_or(trimmed).trim();
                    if sig.len() < 200 {
                        signatures.push(sig.to_string());
                    }
                }
            }
        }
        _ => {}
    }

    signatures
}

/// 언어 감지
fn detect_language(path: &str) -> String {
    let ext = Path::new(path).extension().and_then(|s| s.to_str()).unwrap_or("");
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" => "kotlin",
        _ => "text",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_from_file() {
        let content = r#"//! Agent module
//! Handles AI agent logic

use std::collections::HashMap;

pub struct Agent {
    name: String,
}

pub fn create_agent(name: &str) -> Agent {
    Agent { name: name.to_string() }
}

impl Agent {
    pub fn run(&self) {
        println!("Running {}", self.name);
    }
}
"#;

        let item = context_from_file("src/agent.rs", content);

        assert_eq!(item.kind, ContextItemKind::SourceFile { language: "rust".to_string() });
        assert!(item.summary.contains("Agent module"));
        assert!(!item.signatures.is_empty());
        assert!(item.signatures.iter().any(|s| s.contains("pub struct Agent")));
        assert!(item.signatures.iter().any(|s| s.contains("pub fn create_agent")));
    }

    #[test]
    fn test_context_slice() {
        let mut manager = SmartContextManager::new(1000);

        let item = ContextItem {
            id: "test.rs".to_string(),
            kind: ContextItemKind::SourceFile { language: "rust".to_string() },
            summary: "Test file".to_string(),
            signatures: vec!["fn test()".to_string()],
            full_content: Some("fn test() { }".to_string()),
            estimated_tokens: TokenCounts { summary: 10, signatures: 20, full: 50 },
            last_accessed: Utc::now(),
            relevance: RelevanceScore::default(),
        };

        manager.add_item(item);
        manager.set_focus(vec!["test.rs".to_string()]);

        let slice = manager.get_optimal_slice();
        assert_eq!(slice.items.len(), 1);
    }

    #[test]
    fn test_efficiency_stats() {
        let slice = ContextSlice {
            items: vec![
                (ContextItem {
                    id: "test.rs".to_string(),
                    kind: ContextItemKind::SourceFile { language: "rust".to_string() },
                    summary: "Test".to_string(),
                    signatures: vec![],
                    full_content: None,
                    estimated_tokens: TokenCounts { summary: 10, signatures: 0, full: 100 },
                    last_accessed: Utc::now(),
                    relevance: RelevanceScore::default(),
                }, DetailLevel::Summary),
            ],
            max_tokens: 1000,
            used_tokens: 10,
        };

        let stats = slice.efficiency_stats();
        assert_eq!(stats.tokens_used, 10);
        assert_eq!(stats.tokens_saved, 90);
        assert_eq!(stats.efficiency_percent, 90);
    }
}
