//! Skill Loader - 파일 기반 Skill 로딩 (Claude Code 호환)
//!
//! SKILL.md 파일을 파싱하여 Skill로 변환합니다.
//! Claude Code와 동일한 형식을 지원합니다.

use super::traits::{Skill, SkillContext, SkillDefinition, SkillInput, SkillOutput, SkillMetadata, SkillArgument};
use async_trait::async_trait;
use forge_foundation::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ============================================================================
// SkillConfig - YAML frontmatter에서 파싱된 설정
// ============================================================================

/// SKILL.md의 YAML frontmatter 설정
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillConfig {
    /// Skill 이름 (슬래시 명령어로 사용)
    pub name: String,

    /// 설명
    pub description: Option<String>,

    /// 허용된 Tool 목록
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<Vec<String>>,

    /// 모델만 호출 가능 (사용자 직접 호출 불가)
    #[serde(rename = "disable-model-invocation")]
    pub disable_model_invocation: Option<bool>,

    /// 사용자가 / 메뉴에서 볼 수 있는지
    #[serde(rename = "user-invocable")]
    pub user_invocable: Option<bool>,

    /// 실행 컨텍스트 ("fork"면 subagent)
    pub context: Option<String>,

    /// Subagent 타입 ("Explore", "Plan", 커스텀)
    pub agent: Option<String>,

    /// 모델 오버라이드
    pub model: Option<String>,

    /// 인자 힌트 (자동완성용)
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<Vec<String>>,

    /// 스킬별 Hook 정의
    pub hooks: Option<SkillHooks>,
}

/// 스킬별 Hook 정의
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillHooks {
    #[serde(rename = "PreToolUse")]
    pub pre_tool_use: Option<Vec<HookMatcher>>,

    #[serde(rename = "PostToolUse")]
    pub post_tool_use: Option<Vec<HookMatcher>>,
}

/// Hook 매처
#[derive(Debug, Clone, Deserialize)]
pub struct HookMatcher {
    /// 매칭 패턴 (tool 이름 또는 "*")
    pub matcher: String,

    /// 실행할 Hook들
    pub hooks: Vec<HookAction>,
}

/// Hook 액션
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum HookAction {
    /// 셸 명령어 실행
    #[serde(rename = "command")]
    Command {
        command: String,
        #[serde(default = "default_timeout")]
        timeout: u64,
    },

    /// LLM 프롬프트
    #[serde(rename = "prompt")]
    Prompt {
        prompt: String,
    },
}

fn default_timeout() -> u64 {
    600
}

// ============================================================================
// FileBasedSkill - 파일 기반 Skill
// ============================================================================

/// SKILL.md 파일로부터 로드된 Skill
pub struct FileBasedSkill {
    /// 설정 (frontmatter)
    config: SkillConfig,

    /// 시스템 프롬프트 (Markdown body)
    system_prompt: String,

    /// 소스 파일 경로
    source_path: PathBuf,
}

impl FileBasedSkill {
    /// SKILL.md 파일에서 Skill 생성
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content, path.to_path_buf())
    }

    /// 문자열에서 Skill 파싱
    pub fn parse(content: &str, source_path: PathBuf) -> Result<Self> {
        // YAML frontmatter 추출
        let (config, body) = parse_frontmatter(content)?;

        Ok(Self {
            config,
            system_prompt: body,
            source_path,
        })
    }

    /// 설정 가져오기
    pub fn config(&self) -> &SkillConfig {
        &self.config
    }

    /// 소스 경로 가져오기
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// Agent Loop이 필요한지 (context: fork)
    fn needs_agent_loop(&self) -> bool {
        self.config.context.as_deref() == Some("fork")
    }
}

#[async_trait]
impl Skill for FileBasedSkill {
    fn definition(&self) -> SkillDefinition {
        let command = if self.config.name.starts_with('/') {
            self.config.name.clone()
        } else {
            format!("/{}", self.config.name)
        };

        // argument-hint에서 인자 생성
        let arguments = self.config.argument_hint
            .as_ref()
            .map(|hints| {
                hints.iter().map(|hint| {
                    let name = hint.trim_start_matches('-').to_string();
                    let (short_flag, long_flag) = if hint.starts_with("--") {
                        (None, Some(hint.clone()))
                    } else if hint.starts_with('-') {
                        (Some(hint.clone()), None)
                    } else {
                        (None, None)
                    };

                    SkillArgument {
                        name,
                        description: format!("Argument: {}", hint),
                        required: false,
                        default: None,
                        short_flag,
                        long_flag,
                    }
                }).collect()
            })
            .unwrap_or_default();

        SkillDefinition {
            name: self.config.name.clone(),
            command,
            description: self.config.description.clone().unwrap_or_default(),
            usage: format!("/{} [args]", self.config.name),
            arguments,
            category: "file-based".to_string(),
            user_invocable: self.config.user_invocable.unwrap_or(true),
        }
    }

    fn metadata(&self) -> SkillMetadata {
        SkillMetadata {
            name: self.config.name.clone(),
            source: Some(self.source_path.display().to_string()),
            version: "1.0.0".to_string(),
            author: None,
            required_tools: self.config.allowed_tools.clone().unwrap_or_default(),
            required_permissions: vec![],
            tags: vec!["file-based".to_string()],
            hidden: self.config.user_invocable == Some(false),
        }
    }

    fn system_prompt(&self) -> Option<String> {
        if self.system_prompt.is_empty() {
            None
        } else {
            Some(self.system_prompt.clone())
        }
    }

    fn requires_agent_loop(&self) -> bool {
        self.needs_agent_loop()
    }

    async fn execute(&self, _ctx: &SkillContext<'_>, input: SkillInput) -> Result<SkillOutput> {
        // $ARGUMENTS 치환
        let mut prompt = self.system_prompt.clone();

        // $ARGUMENTS → 전체 인자 (raw_command 사용)
        let raw_args = input.raw_command
            .split_whitespace()
            .skip(1) // 명령어 스킵
            .collect::<Vec<_>>()
            .join(" ");
        prompt = prompt.replace("$ARGUMENTS", &raw_args);

        // $N 또는 $ARGUMENTS[N] → 특정 인자
        for (i, (_, value)) in input.arguments.iter().enumerate() {
            prompt = prompt.replace(&format!("${}", i), value);
            prompt = prompt.replace(&format!("$ARGUMENTS[{}]", i), value);
        }

        // 위치 인자도 치환
        for (i, value) in input.positional_args.iter().enumerate() {
            let idx = input.arguments.len() + i;
            prompt = prompt.replace(&format!("${}", idx), value);
            prompt = prompt.replace(&format!("$ARGUMENTS[{}]", idx), value);
        }

        Ok(SkillOutput::success(prompt))
    }
}

// ============================================================================
// SkillLoader - Skill 로더
// ============================================================================

/// 파일 시스템에서 Skill을 검색하고 로드
pub struct SkillLoader {
    /// 검색 경로
    search_paths: Vec<PathBuf>,
}

impl SkillLoader {
    /// 새 로더 생성 (기본 검색 경로)
    pub fn new(working_dir: &Path) -> Self {
        let mut paths = Vec::new();

        // 1. User-level
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".claude/skills"));
            paths.push(home.join(".forgecode/skills"));
        }

        // 2. Project-level
        paths.push(working_dir.join(".claude/skills"));
        paths.push(working_dir.join(".forgecode/skills"));

        Self { search_paths: paths }
    }

    /// 커스텀 검색 경로로 생성
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self { search_paths: paths }
    }

    /// 검색 경로 추가
    pub fn add_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    /// 모든 Skill 로드
    pub fn load_all(&self) -> Vec<FileBasedSkill> {
        let mut skills = Vec::new();
        let mut loaded_names = std::collections::HashSet::new();

        // 역순으로 검색 (나중 경로가 우선)
        for search_path in self.search_paths.iter().rev() {
            if !search_path.exists() {
                continue;
            }

            debug!("Searching for skills in: {}", search_path.display());

            if let Ok(entries) = std::fs::read_dir(search_path) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    // 디렉토리이고 SKILL.md가 있는 경우
                    if path.is_dir() {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.exists() {
                            match FileBasedSkill::from_file(&skill_file) {
                                Ok(skill) => {
                                    let name = skill.config.name.clone();

                                    // 이미 로드된 이름은 스킵 (우선순위 처리)
                                    if !loaded_names.contains(&name) {
                                        info!("Loaded skill '{}' from {}", name, skill_file.display());
                                        loaded_names.insert(name);
                                        skills.push(skill);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to load skill from {}: {}", skill_file.display(), e);
                                }
                            }
                        }
                    }
                }
            }
        }

        skills
    }

    /// 특정 이름의 Skill 로드
    pub fn load_by_name(&self, name: &str) -> Option<FileBasedSkill> {
        // 역순으로 검색 (나중 경로가 우선)
        for search_path in self.search_paths.iter().rev() {
            let skill_dir = search_path.join(name);
            let skill_file = skill_dir.join("SKILL.md");

            if skill_file.exists() {
                match FileBasedSkill::from_file(&skill_file) {
                    Ok(skill) => return Some(skill),
                    Err(e) => {
                        warn!("Failed to load skill '{}': {}", name, e);
                    }
                }
            }
        }

        None
    }
}

// ============================================================================
// Frontmatter 파서
// ============================================================================

/// YAML frontmatter와 body를 분리
fn parse_frontmatter(content: &str) -> Result<(SkillConfig, String)> {
    let lines: Vec<&str> = content.lines().collect();

    // frontmatter 시작 확인
    if lines.is_empty() || lines[0].trim() != "---" {
        // frontmatter 없음 - 기본 설정 사용
        return Ok((SkillConfig::default(), content.to_string()));
    }

    // frontmatter 끝 찾기
    let mut end_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_idx = Some(i);
            break;
        }
    }

    let end_idx = end_idx.ok_or_else(|| {
        forge_foundation::Error::InvalidInput("Invalid SKILL.md: unclosed frontmatter".into())
    })?;

    // YAML 파싱
    let yaml_content = lines[1..end_idx].join("\n");
    let config: SkillConfig = serde_yaml::from_str(&yaml_content)
        .map_err(|e| forge_foundation::Error::InvalidInput(format!("Invalid YAML frontmatter: {}", e)))?;

    // Body 추출
    let body = lines[(end_idx + 1)..].join("\n").trim().to_string();

    Ok((config, body))
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKILL: &str = r#"---
name: commit
description: Git 커밋 자동화
allowed-tools:
  - Read
  - Bash
  - Grep
context: fork
agent: Explore
argument-hint:
  - -m message
---

커밋을 작성할 때 다음을 수행하세요:

1. `git status`로 변경사항 확인
2. 변경된 파일들 분석
3. 커밋 메시지 작성

$ARGUMENTS 파라미터가 있으면 사용합니다.
"#;

    #[test]
    fn test_parse_frontmatter() {
        let (config, body) = parse_frontmatter(SAMPLE_SKILL).unwrap();

        assert_eq!(config.name, "commit");
        assert_eq!(config.description, Some("Git 커밋 자동화".into()));
        assert_eq!(config.allowed_tools, Some(vec!["Read".into(), "Bash".into(), "Grep".into()]));
        assert_eq!(config.context, Some("fork".into()));
        assert_eq!(config.agent, Some("Explore".into()));
        assert!(body.contains("git status"));
    }

    #[test]
    fn test_file_based_skill() {
        let skill = FileBasedSkill::parse(SAMPLE_SKILL, PathBuf::from("test/SKILL.md")).unwrap();

        assert_eq!(skill.definition().name, "commit");
        assert_eq!(skill.definition().command, "/commit");
        assert!(skill.requires_agent_loop());
        assert!(skill.system_prompt().is_some());
    }

    #[test]
    fn test_no_frontmatter() {
        let content = "Just some instructions without frontmatter.";
        let (config, body) = parse_frontmatter(content).unwrap();

        assert!(config.name.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn test_argument_substitution() {
        use crate::tool::RuntimeContext;
        use forge_foundation::PermissionService;
        use std::sync::Arc;

        let skill = FileBasedSkill::parse(SAMPLE_SKILL, PathBuf::from("test/SKILL.md")).unwrap();

        // SkillInput의 올바른 구조 사용
        let input = SkillInput::new("/commit -m 'test message'")
            .with_arg("m", "'test message'");

        let output = tokio_test::block_on(async {
            let permissions = Arc::new(PermissionService::new());
            let tool_ctx = RuntimeContext::new(
                "test-session",
                PathBuf::from("."),
                permissions,
            );
            let ctx = SkillContext::new(&tool_ctx, "test-session");
            skill.execute(&ctx, input).await.unwrap()
        });

        // SkillOutput은 message 필드 사용
        assert!(output.message.contains("-m 'test message'"));
    }
}
