//! Skill traits and core types

use async_trait::async_trait;
use forge_foundation::{Result, ToolContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// SkillDefinition - 스킬 메타데이터 정의
// ============================================================================

/// 스킬 정의 메타데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    /// 스킬 이름 (예: "commit", "review-pr")
    pub name: String,

    /// 호출 명령어 (예: "/commit", "/review-pr")
    pub command: String,

    /// 짧은 설명
    pub description: String,

    /// 상세 사용법
    pub usage: String,

    /// 인자 정의
    pub arguments: Vec<SkillArgument>,

    /// 카테고리 (git, code, explain 등)
    pub category: String,

    /// 사용자 호출 가능 여부 (일부 스킬은 내부 전용)
    pub user_invocable: bool,
}

/// 스킬 인자 정의
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillArgument {
    /// 인자 이름
    pub name: String,

    /// 설명
    pub description: String,

    /// 필수 여부
    pub required: bool,

    /// 기본값
    pub default: Option<String>,

    /// 짧은 플래그 (예: "-m")
    pub short_flag: Option<String>,

    /// 긴 플래그 (예: "--message")
    pub long_flag: Option<String>,
}

// ============================================================================
// SkillMetadata - 런타임 메타데이터
// ============================================================================

/// 스킬 런타임 메타데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// 스킬 이름
    pub name: String,

    /// 버전
    pub version: String,

    /// 작성자
    pub author: Option<String>,

    /// 소스 (파일 경로, 플러그인 이름 등)
    pub source: Option<String>,

    /// 필요한 도구들
    pub required_tools: Vec<String>,

    /// 필요한 권한
    pub required_permissions: Vec<String>,

    /// 태그
    pub tags: Vec<String>,

    /// 숨김 여부 (/ 메뉴에서 표시 안 함)
    pub hidden: bool,
}

impl Default for SkillMetadata {
    fn default() -> Self {
        Self {
            name: "unnamed".into(),
            version: "1.0.0".into(),
            author: None,
            source: None,
            required_tools: vec![],
            required_permissions: vec![],
            tags: vec![],
            hidden: false,
        }
    }
}

// ============================================================================
// SkillInput / SkillOutput - I/O 타입
// ============================================================================

/// 스킬 실행 입력
#[derive(Debug, Clone)]
pub struct SkillInput {
    /// 원본 명령어 문자열 (예: "/commit -m 'fix bug'")
    pub raw_command: String,

    /// 파싱된 인자들
    pub arguments: HashMap<String, String>,

    /// 위치 인자들 (named 아닌 것)
    pub positional_args: Vec<String>,

    /// 추가 컨텍스트 데이터
    pub context_data: Value,
}

impl SkillInput {
    /// 새 입력 생성
    pub fn new(raw_command: impl Into<String>) -> Self {
        Self {
            raw_command: raw_command.into(),
            arguments: HashMap::new(),
            positional_args: vec![],
            context_data: Value::Null,
        }
    }

    /// 인자 추가
    pub fn with_arg(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.arguments.insert(key.into(), value.into());
        self
    }

    /// 위치 인자 추가
    pub fn with_positional(mut self, arg: impl Into<String>) -> Self {
        self.positional_args.push(arg.into());
        self
    }

    /// 컨텍스트 데이터 추가
    pub fn with_context(mut self, data: Value) -> Self {
        self.context_data = data;
        self
    }

    /// 인자 가져오기
    pub fn get(&self, key: &str) -> Option<&String> {
        self.arguments.get(key)
    }

    /// 인자 가져오기 (기본값 포함)
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.arguments.get(key).cloned().unwrap_or_else(|| default.to_string())
    }

    /// 플래그 존재 여부
    pub fn has_flag(&self, flag: &str) -> bool {
        self.arguments.contains_key(flag)
    }
}

/// 스킬 실행 결과
#[derive(Debug, Clone)]
pub struct SkillOutput {
    /// 성공 여부
    pub success: bool,

    /// 결과 메시지
    pub message: String,

    /// 상세 데이터
    pub data: Value,

    /// 사용자에게 표시할 요약
    pub summary: Option<String>,

    /// 실행된 액션들
    pub actions: Vec<SkillAction>,
}

impl SkillOutput {
    /// 성공 결과 생성
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Value::Null,
            summary: None,
            actions: vec![],
        }
    }

    /// 실패 결과 생성
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: Value::Null,
            summary: None,
            actions: vec![],
        }
    }

    /// 데이터 추가
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    /// 요약 추가
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// 액션 추가
    pub fn with_action(mut self, action: SkillAction) -> Self {
        self.actions.push(action);
        self
    }
}

/// 스킬이 수행한 액션
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAction {
    /// 액션 타입 (file_created, file_modified, command_run, etc.)
    pub action_type: String,

    /// 상세 정보
    pub details: String,

    /// 관련 경로 (있는 경우)
    pub path: Option<String>,
}

// ============================================================================
// SkillContext - 스킬 실행 컨텍스트
// ============================================================================

/// 스킬 실행 컨텍스트
///
/// Tool 컨텍스트를 확장하여 스킬 전용 기능 제공
pub struct SkillContext<'a> {
    /// 기본 Tool 컨텍스트
    pub tool_ctx: &'a dyn ToolContext,

    /// 세션 ID
    pub session_id: String,

    /// 스킬 전용 시스템 프롬프트
    pub system_prompt: Option<String>,

    /// 현재 대화 히스토리 요약 (컨텍스트용)
    pub conversation_summary: Option<String>,

    /// Git 정보 (현재 브랜치, 상태 등)
    pub git_info: Option<GitInfo>,

    /// 추가 메타데이터
    pub metadata: HashMap<String, Value>,
}

impl<'a> SkillContext<'a> {
    /// 새 컨텍스트 생성
    pub fn new(tool_ctx: &'a dyn ToolContext, session_id: impl Into<String>) -> Self {
        Self {
            tool_ctx,
            session_id: session_id.into(),
            system_prompt: None,
            conversation_summary: None,
            git_info: None,
            metadata: HashMap::new(),
        }
    }

    /// 시스템 프롬프트 설정
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Git 정보 설정
    pub fn with_git_info(mut self, info: GitInfo) -> Self {
        self.git_info = Some(info);
        self
    }

    /// 대화 요약 설정
    pub fn with_conversation_summary(mut self, summary: impl Into<String>) -> Self {
        self.conversation_summary = Some(summary.into());
        self
    }

    /// 메타데이터 추가
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Git 저장소 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    /// 현재 브랜치
    pub branch: String,

    /// 메인 브랜치
    pub main_branch: String,

    /// 변경된 파일 수
    pub changed_files: usize,

    /// 스테이지된 파일 수
    pub staged_files: usize,

    /// 마지막 커밋 메시지
    pub last_commit_message: Option<String>,

    /// 원격 저장소 URL
    pub remote_url: Option<String>,
}

// ============================================================================
// Skill Trait - 핵심 스킬 인터페이스
// ============================================================================

/// 스킬 트레이트 - 모든 스킬이 구현해야 함
#[async_trait]
pub trait Skill: Send + Sync {
    /// 스킬 정의 반환
    fn definition(&self) -> SkillDefinition;

    /// 스킬 메타데이터 반환
    fn metadata(&self) -> SkillMetadata {
        SkillMetadata {
            name: self.definition().name,
            ..Default::default()
        }
    }

    /// 스킬 전용 시스템 프롬프트 (에이전트 루프 사용 시)
    fn system_prompt(&self) -> Option<String> {
        None
    }

    /// 스킬 실행
    ///
    /// 간단한 스킬은 직접 실행하고,
    /// 복잡한 스킬은 에이전트 루프를 통해 실행
    async fn execute(&self, ctx: &SkillContext<'_>, input: SkillInput) -> Result<SkillOutput>;

    /// 인자 파싱 헬퍼 (기본 구현 제공)
    fn parse_input(&self, raw: &str) -> SkillInput {
        let mut input = SkillInput::new(raw);
        let parts: Vec<&str> = raw.split_whitespace().collect();

        if parts.is_empty() {
            return input;
        }

        // 첫 번째는 명령어, 나머지 파싱
        let mut i = 1;
        while i < parts.len() {
            let part = parts[i];

            if part.starts_with("--") {
                // 긴 플래그
                let key = &part[2..];
                if i + 1 < parts.len() && !parts[i + 1].starts_with('-') {
                    input.arguments.insert(key.to_string(), parts[i + 1].to_string());
                    i += 2;
                } else {
                    input.arguments.insert(key.to_string(), "true".to_string());
                    i += 1;
                }
            } else if part.starts_with('-') {
                // 짧은 플래그
                let key = &part[1..];
                if i + 1 < parts.len() && !parts[i + 1].starts_with('-') {
                    input.arguments.insert(key.to_string(), parts[i + 1].to_string());
                    i += 2;
                } else {
                    input.arguments.insert(key.to_string(), "true".to_string());
                    i += 1;
                }
            } else {
                // 위치 인자
                input.positional_args.push(part.to_string());
                i += 1;
            }
        }

        input
    }

    /// 에이전트 루프가 필요한지 여부
    fn requires_agent_loop(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_input_parsing() {
        // Mock skill for testing
        struct TestSkill;

        #[async_trait]
        impl Skill for TestSkill {
            fn definition(&self) -> SkillDefinition {
                SkillDefinition {
                    name: "test".into(),
                    command: "/test".into(),
                    description: "Test skill".into(),
                    usage: "/test [args]".into(),
                    arguments: vec![],
                    category: "test".into(),
                    user_invocable: true,
                }
            }

            async fn execute(&self, _ctx: &SkillContext<'_>, _input: SkillInput) -> Result<SkillOutput> {
                Ok(SkillOutput::success("done"))
            }
        }

        let skill = TestSkill;

        // Test parsing
        let input = skill.parse_input("/test -m \"fix bug\" --verbose file.rs");
        assert_eq!(input.arguments.get("m"), Some(&"\"fix".to_string())); // Basic parsing
        assert!(input.arguments.contains_key("verbose"));
    }

    #[test]
    fn test_skill_output() {
        let output = SkillOutput::success("Commit created")
            .with_summary("Created commit abc123")
            .with_action(SkillAction {
                action_type: "git_commit".into(),
                details: "abc123".into(),
                path: None,
            });

        assert!(output.success);
        assert_eq!(output.summary, Some("Created commit abc123".into()));
        assert_eq!(output.actions.len(), 1);
    }
}
