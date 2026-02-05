//! Agent Sub-skills - 2025 Cursor 2.4 스타일
//!
//! 에이전트가 사용할 수 있는 특수 기능들:
//! - 웹 검색
//! - 이미지 생성
//! - API 호출
//! - 코드 분석
//!
//! ## 사용 예시
//!
//! ```text
//! Agent: "이 에러를 해결하려면 최신 문서가 필요해요"
//! → SubSkill::WebSearch 자동 호출
//! → 결과를 컨텍스트에 추가
//! → 해결책 제시
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Sub-skill 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubSkillResult {
    /// 성공 여부
    pub success: bool,
    /// 결과 내용
    pub content: String,
    /// 메타데이터
    pub metadata: HashMap<String, String>,
    /// 토큰 수 (추정)
    pub estimated_tokens: usize,
}

impl SubSkillResult {
    pub fn success(content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = content.len() / 4;
        Self {
            success: true,
            content,
            metadata: HashMap::new(),
            estimated_tokens: tokens,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            content: message.into(),
            metadata: HashMap::new(),
            estimated_tokens: 0,
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Sub-skill 트레이트
#[async_trait]
pub trait SubSkill: Send + Sync {
    /// 스킬 이름
    fn name(&self) -> &str;

    /// 스킬 설명
    fn description(&self) -> &str;

    /// 스킬 실행 가능 여부 확인
    fn can_handle(&self, intent: &str) -> bool;

    /// 스킬 실행
    async fn execute(&self, input: &str, context: &SubSkillContext) -> SubSkillResult;
}

/// Sub-skill 실행 컨텍스트
#[derive(Debug, Clone, Default)]
pub struct SubSkillContext {
    /// 현재 작업 디렉토리
    pub working_dir: String,
    /// 현재 파일
    pub current_file: Option<String>,
    /// 환경 변수
    pub env: HashMap<String, String>,
    /// 설정
    pub config: HashMap<String, String>,
}

/// 웹 검색 Sub-skill
pub struct WebSearchSkill {
    /// 검색 엔진 URL (기본: DuckDuckGo)
    search_url: String,
}

impl Default for WebSearchSkill {
    fn default() -> Self {
        Self {
            search_url: "https://api.duckduckgo.com/".to_string(),
        }
    }
}

#[async_trait]
impl SubSkill for WebSearchSkill {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for documentation, solutions, and information"
    }

    fn can_handle(&self, intent: &str) -> bool {
        let keywords = ["search", "find", "look up", "documentation", "how to", "what is"];
        keywords.iter().any(|k| intent.to_lowercase().contains(k))
    }

    async fn execute(&self, input: &str, _context: &SubSkillContext) -> SubSkillResult {
        // TODO: 실제 웹 검색 구현
        // 현재는 플레이스홀더
        SubSkillResult::success(format!(
            "Web search results for '{}' would appear here.\n\
             (Note: Web search not yet implemented)",
            input
        ))
        .with_metadata("query", input)
    }
}

/// 코드 분석 Sub-skill
pub struct CodeAnalysisSkill;

#[async_trait]
impl SubSkill for CodeAnalysisSkill {
    fn name(&self) -> &str {
        "code_analysis"
    }

    fn description(&self) -> &str {
        "Analyze code for complexity, dependencies, and potential issues"
    }

    fn can_handle(&self, intent: &str) -> bool {
        let keywords = ["analyze", "complexity", "dependencies", "issues", "review"];
        keywords.iter().any(|k| intent.to_lowercase().contains(k))
    }

    async fn execute(&self, input: &str, context: &SubSkillContext) -> SubSkillResult {
        // 간단한 코드 분석
        let analysis = analyze_code(input, context);
        SubSkillResult::success(analysis)
    }
}

/// 간단한 코드 분석
fn analyze_code(code: &str, _context: &SubSkillContext) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let line_count = lines.len();
    let char_count = code.len();

    // 간단한 메트릭 계산
    let comment_lines = lines.iter().filter(|l| {
        let t = l.trim();
        t.starts_with("//") || t.starts_with("#") || t.starts_with("/*")
    }).count();

    let blank_lines = lines.iter().filter(|l| l.trim().is_empty()).count();
    let code_lines = line_count - comment_lines - blank_lines;

    // 복잡도 추정 (간단한 휴리스틱)
    let complexity_keywords = ["if", "else", "for", "while", "match", "loop", "?"];
    let complexity: usize = complexity_keywords.iter()
        .map(|k| code.matches(k).count())
        .sum();

    format!(
        "## Code Analysis\n\n\
         - **Lines**: {} total ({} code, {} comments, {} blank)\n\
         - **Characters**: {}\n\
         - **Estimated Complexity**: {} branch points\n\
         - **Functions/Methods**: {} approximate\n",
        line_count, code_lines, comment_lines, blank_lines,
        char_count,
        complexity,
        code.matches("fn ").count() + code.matches("def ").count() + code.matches("function ").count()
    )
}

/// Git 작업 Sub-skill
pub struct GitSkill;

#[async_trait]
impl SubSkill for GitSkill {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Perform Git operations: status, diff, log, commit"
    }

    fn can_handle(&self, intent: &str) -> bool {
        let keywords = ["git", "commit", "diff", "status", "branch", "merge"];
        keywords.iter().any(|k| intent.to_lowercase().contains(k))
    }

    async fn execute(&self, input: &str, context: &SubSkillContext) -> SubSkillResult {
        // Git 명령 실행 (제한적)
        let allowed_commands = ["status", "diff", "log", "branch"];

        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts.first().map(|s| *s).unwrap_or("status");

        if !allowed_commands.contains(&command) {
            return SubSkillResult::error(format!(
                "Git command '{}' not allowed. Allowed: {:?}",
                command, allowed_commands
            ));
        }

        // TODO: 실제 Git 명령 실행
        SubSkillResult::success(format!(
            "Git {} output would appear here.\n\
             Working directory: {}",
            command,
            context.working_dir
        ))
        .with_metadata("command", command)
    }
}

/// 테스트 실행 Sub-skill
pub struct TestRunnerSkill;

#[async_trait]
impl SubSkill for TestRunnerSkill {
    fn name(&self) -> &str {
        "test_runner"
    }

    fn description(&self) -> &str {
        "Run tests and report results"
    }

    fn can_handle(&self, intent: &str) -> bool {
        let keywords = ["test", "run tests", "verify", "check"];
        keywords.iter().any(|k| intent.to_lowercase().contains(k))
    }

    async fn execute(&self, input: &str, context: &SubSkillContext) -> SubSkillResult {
        // TODO: 테스트 프레임워크 감지 및 실행
        SubSkillResult::success(format!(
            "Test execution for '{}' in {}",
            input,
            context.working_dir
        ))
    }
}

/// Sub-skill 레지스트리
pub struct SubSkillRegistry {
    skills: Vec<Arc<dyn SubSkill>>,
}

impl Default for SubSkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SubSkillRegistry {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    /// 기본 스킬들로 초기화
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(WebSearchSkill::default()));
        registry.register(Arc::new(CodeAnalysisSkill));
        registry.register(Arc::new(GitSkill));
        registry.register(Arc::new(TestRunnerSkill));
        registry
    }

    /// 스킬 등록
    pub fn register(&mut self, skill: Arc<dyn SubSkill>) {
        self.skills.push(skill);
    }

    /// 의도에 맞는 스킬 찾기
    pub fn find_skill(&self, intent: &str) -> Option<Arc<dyn SubSkill>> {
        self.skills.iter()
            .find(|s| s.can_handle(intent))
            .cloned()
    }

    /// 모든 스킬 목록
    pub fn list_skills(&self) -> Vec<(&str, &str)> {
        self.skills.iter()
            .map(|s| (s.name(), s.description()))
            .collect()
    }

    /// 스킬 자동 실행 (의도 분석 후)
    pub async fn auto_execute(
        &self,
        intent: &str,
        input: &str,
        context: &SubSkillContext,
    ) -> Option<SubSkillResult> {
        let skill = self.find_skill(intent)?;
        Some(skill.execute(input, context).await)
    }
}

/// 의도 분석기 - 사용자 요청에서 필요한 스킬 감지
pub struct IntentAnalyzer;

impl IntentAnalyzer {
    /// 텍스트에서 의도 추출
    pub fn analyze(text: &str) -> Vec<String> {
        let mut intents = Vec::new();

        // 간단한 키워드 기반 분석
        let keywords_map = [
            (vec!["search", "find", "look up", "google"], "web_search"),
            (vec!["analyze", "review", "check code"], "code_analysis"),
            (vec!["git", "commit", "diff", "branch"], "git"),
            (vec!["test", "run tests", "verify"], "test_runner"),
        ];

        let lower = text.to_lowercase();

        for (keywords, intent) in &keywords_map {
            if keywords.iter().any(|k| lower.contains(k)) {
                intents.push(intent.to_string());
            }
        }

        intents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_analyzer() {
        let intents = IntentAnalyzer::analyze("Can you search for how to fix this error?");
        assert!(intents.contains(&"web_search".to_string()));

        let intents = IntentAnalyzer::analyze("Analyze this code for issues");
        assert!(intents.contains(&"code_analysis".to_string()));

        let intents = IntentAnalyzer::analyze("Show me the git diff");
        assert!(intents.contains(&"git".to_string()));
    }

    #[test]
    fn test_registry() {
        let registry = SubSkillRegistry::with_defaults();
        let skills = registry.list_skills();

        assert!(skills.iter().any(|(name, _)| *name == "web_search"));
        assert!(skills.iter().any(|(name, _)| *name == "code_analysis"));
        assert!(skills.iter().any(|(name, _)| *name == "git"));
    }

    #[tokio::test]
    async fn test_skill_execution() {
        let registry = SubSkillRegistry::with_defaults();
        let context = SubSkillContext::default();

        let result = registry.auto_execute("analyze code", "fn main() {}", &context).await;
        assert!(result.is_some());
        assert!(result.unwrap().success);
    }

    #[test]
    fn test_code_analysis() {
        let code = r#"
// Main function
fn main() {
    if true {
        println!("Hello");
    } else {
        println!("World");
    }
}
"#;
        let analysis = analyze_code(code, &SubSkillContext::default());
        assert!(analysis.contains("Lines"));
        assert!(analysis.contains("Complexity"));
    }
}
