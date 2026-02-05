//! Skill Loader - 파일 기반 Skill 로딩
//!
//! SKILL.md 파일을 파싱하여 Skill로 변환합니다.
//!
//! ## 호환성
//! - **Claude Code**: `.claude/skills/` (공식 표준)
//! - **OpenCode**: `.opencode/skills/` (Go 기반 CLI)
//! - **ForgeCode**: `.forgecode/skills/` (확장 기능)
//!
//! ## 지원 파일
//! - `SKILL.md` - 메인 스킬 정의 (필수)
//! - `FORMS.md` - 입력 폼 정의 (선택)
//! - `REFERENCE.md` - 참조 문서 (선택)
//! - `scripts/` - 실행 스크립트 (선택)
//!
//! ## Frontmatter 필드 (완전 호환)
//! ```yaml
//! name: skill-name              # 필수 (64자 이하)
//! description: 설명             # 필수 (1024자 이하)
//! allowed-tools: [Read, Bash]   # 허용 도구
//! user-invocable: true          # 사용자 호출 가능
//! context: fork                 # subagent 실행
//! agent: Explore                # subagent 타입
//! model: claude-sonnet-4        # 모델 오버라이드
//! argument-hint: [-m message]   # 인자 힌트
//! # 확장 필드 (ForgeCode)
//! category: git                 # 카테고리
//! difficulty: beginner          # 난이도
//! prerequisites: [skill-1]      # 선행 스킬
//! estimated-time: "5 minutes"   # 예상 시간
//! ```

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
///
/// Claude Code / OpenCode / ForgeCode 완전 호환
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillConfig {
    // ========================================
    // 필수 필드 (Claude Code 표준)
    // ========================================

    /// Skill 이름 (슬래시 명령어로 사용, 64자 이하)
    pub name: String,

    /// 설명 (1024자 이하, XML 태그 금지)
    pub description: Option<String>,

    // ========================================
    // Claude Code 표준 필드
    // ========================================

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

    /// Subagent 타입 ("Explore", "Plan", "general-purpose", 커스텀)
    pub agent: Option<String>,

    /// 모델 오버라이드
    pub model: Option<String>,

    /// 인자 힌트 (자동완성용)
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<Vec<String>>,

    // ========================================
    // ForgeCode 확장 필드
    // ========================================

    /// 카테고리 (git, code, explain, testing, cloud-native 등)
    pub category: Option<String>,

    /// 난이도 (beginner, intermediate, advanced, expert)
    pub difficulty: Option<String>,

    /// 선행 스킬 목록
    pub prerequisites: Option<Vec<String>>,

    /// 관련 스킬 목록
    #[serde(rename = "related-skills")]
    pub related_skills: Option<Vec<String>>,

    /// 예상 소요 시간
    #[serde(rename = "estimated-time")]
    pub estimated_time: Option<String>,

    /// 마지막 업데이트 날짜
    #[serde(rename = "last-updated")]
    pub last_updated: Option<String>,

    /// 작성자
    pub author: Option<String>,

    /// 버전
    pub version: Option<String>,

    /// 태그 목록
    pub tags: Option<Vec<String>>,

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
///
/// ## 지원 파일 구조
/// ```text
/// skill-name/
/// ├── SKILL.md      # 메인 정의 (필수)
/// ├── FORMS.md      # 입력 폼 정의 (선택)
/// ├── REFERENCE.md  # 참조 문서 (선택)
/// └── scripts/      # 실행 스크립트 (선택)
///     ├── run.py
///     └── helper.sh
/// ```
pub struct FileBasedSkill {
    /// 설정 (frontmatter)
    config: SkillConfig,

    /// 시스템 프롬프트 (Markdown body)
    system_prompt: String,

    /// 소스 파일 경로
    source_path: PathBuf,

    /// FORMS.md 내용 (선택)
    forms_content: Option<String>,

    /// REFERENCE.md 내용 (선택)
    reference_content: Option<String>,

    /// scripts/ 디렉토리 존재 여부
    has_scripts: bool,
}

impl FileBasedSkill {
    /// SKILL.md 파일에서 Skill 생성 (전체 디렉토리 로드)
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let skill_dir = path.parent().unwrap_or(Path::new("."));

        // YAML frontmatter 추출
        let (config, body) = parse_frontmatter(&content)?;

        // 추가 파일 로드 (FORMS.md, REFERENCE.md)
        let forms_path = skill_dir.join("FORMS.md");
        let forms_content = if forms_path.exists() {
            std::fs::read_to_string(&forms_path).ok()
        } else {
            None
        };

        let reference_path = skill_dir.join("REFERENCE.md");
        let reference_content = if reference_path.exists() {
            std::fs::read_to_string(&reference_path).ok()
        } else {
            None
        };

        // scripts/ 디렉토리 확인
        let scripts_dir = skill_dir.join("scripts");
        let has_scripts = scripts_dir.is_dir();

        if has_scripts {
            debug!("Skill '{}' has scripts directory", config.name);
        }

        Ok(Self {
            config,
            system_prompt: body,
            source_path: path.to_path_buf(),
            forms_content,
            reference_content,
            has_scripts,
        })
    }

    /// 문자열에서 Skill 파싱 (단순 파싱, 추가 파일 없음)
    pub fn parse(content: &str, source_path: PathBuf) -> Result<Self> {
        let (config, body) = parse_frontmatter(content)?;

        Ok(Self {
            config,
            system_prompt: body,
            source_path,
            forms_content: None,
            reference_content: None,
            has_scripts: false,
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

    /// 스킬 디렉토리 경로 가져오기
    pub fn skill_dir(&self) -> &Path {
        self.source_path.parent().unwrap_or(Path::new("."))
    }

    /// Agent Loop이 필요한지 (context: fork)
    fn needs_agent_loop(&self) -> bool {
        self.config.context.as_deref() == Some("fork")
    }

    /// FORMS.md 내용 가져오기
    pub fn forms(&self) -> Option<&str> {
        self.forms_content.as_deref()
    }

    /// REFERENCE.md 내용 가져오기
    pub fn reference(&self) -> Option<&str> {
        self.reference_content.as_deref()
    }

    /// scripts/ 디렉토리에서 스크립트 목록 가져오기
    pub fn list_scripts(&self) -> Vec<PathBuf> {
        if !self.has_scripts {
            return vec![];
        }

        let scripts_dir = self.skill_dir().join("scripts");
        std::fs::read_dir(&scripts_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().is_file())
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 특정 스크립트 경로 가져오기
    pub fn script_path(&self, name: &str) -> Option<PathBuf> {
        if !self.has_scripts {
            return None;
        }

        let scripts_dir = self.skill_dir().join("scripts");
        let script_path = scripts_dir.join(name);

        if script_path.exists() {
            Some(script_path)
        } else {
            None
        }
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

        // 카테고리: 설정에서 가져오거나 기본값 사용
        let category = self.config.category.clone()
            .unwrap_or_else(|| "file-based".to_string());

        SkillDefinition {
            name: self.config.name.clone(),
            command,
            description: self.config.description.clone().unwrap_or_default(),
            usage: format!("/{} [args]", self.config.name),
            arguments,
            category,
            user_invocable: self.config.user_invocable.unwrap_or(true),
        }
    }

    fn metadata(&self) -> SkillMetadata {
        // 태그 생성: 파일 기반 + 카테고리 + 사용자 태그
        let mut tags = vec!["file-based".to_string()];
        if let Some(cat) = &self.config.category {
            tags.push(cat.clone());
        }
        if let Some(diff) = &self.config.difficulty {
            tags.push(format!("difficulty:{}", diff));
        }
        if let Some(user_tags) = &self.config.tags {
            tags.extend(user_tags.clone());
        }

        SkillMetadata {
            name: self.config.name.clone(),
            source: Some(self.source_path.display().to_string()),
            version: self.config.version.clone().unwrap_or_else(|| "1.0.0".to_string()),
            author: self.config.author.clone(),
            required_tools: self.config.allowed_tools.clone().unwrap_or_default(),
            required_permissions: vec![],
            tags,
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
    ///
    /// ## 검색 경로 (우선순위 역순, 나중이 높음)
    /// 1. `~/.claude/skills/` - Claude Code 사용자 레벨
    /// 2. `~/.opencode/skills/` - OpenCode 사용자 레벨
    /// 3. `~/.forgecode/skills/` - ForgeCode 사용자 레벨
    /// 4. `.claude/skills/` - Claude Code 프로젝트 레벨
    /// 5. `.opencode/skills/` - OpenCode 프로젝트 레벨
    /// 6. `.forgecode/skills/` - ForgeCode 프로젝트 레벨
    pub fn new(working_dir: &Path) -> Self {
        let mut paths = Vec::new();

        // 1. User-level (사용자 홈 디렉토리)
        if let Some(home) = dirs::home_dir() {
            // Claude Code (Anthropic 공식)
            paths.push(home.join(".claude/skills"));
            // OpenCode (Go 기반 CLI)
            paths.push(home.join(".opencode/skills"));
            // Codex CLI (OpenAI)
            paths.push(home.join(".codex/skills"));
            // ForgeCode
            paths.push(home.join(".forgecode/skills"));
        }

        // 2. Project-level (작업 디렉토리)
        // Claude Code
        paths.push(working_dir.join(".claude/skills"));
        // OpenCode
        paths.push(working_dir.join(".opencode/skills"));
        // Codex CLI
        paths.push(working_dir.join(".codex/skills"));
        // ForgeCode (최고 우선순위)
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

    /// 확장 필드 테스트 (ForgeCode 고유 기능)
    #[test]
    fn test_extended_fields() {
        let extended_skill = r#"---
name: deploy
description: 배포 자동화
allowed-tools:
  - Bash
  - Read
category: devops
difficulty: advanced
author: ForgeCode Team
version: "2.0.0"
tags:
  - deployment
  - ci-cd
prerequisites:
  - build
  - test
related-skills:
  - rollback
  - monitor
estimated-time: "15 minutes"
last-updated: "2025-02-05"
---

배포 스크립트 실행
"#;

        let (config, _body) = parse_frontmatter(extended_skill).unwrap();

        // 필수 필드
        assert_eq!(config.name, "deploy");
        assert_eq!(config.description, Some("배포 자동화".into()));

        // 확장 필드
        assert_eq!(config.category, Some("devops".into()));
        assert_eq!(config.difficulty, Some("advanced".into()));
        assert_eq!(config.author, Some("ForgeCode Team".into()));
        assert_eq!(config.version, Some("2.0.0".into()));
        assert_eq!(config.tags, Some(vec!["deployment".into(), "ci-cd".into()]));
        assert_eq!(config.prerequisites, Some(vec!["build".into(), "test".into()]));
        assert_eq!(config.related_skills, Some(vec!["rollback".into(), "monitor".into()]));
        assert_eq!(config.estimated_time, Some("15 minutes".into()));
        assert_eq!(config.last_updated, Some("2025-02-05".into()));
    }

    /// Claude Code 최소 형식 호환성 테스트
    #[test]
    fn test_claude_code_minimal() {
        // Claude Code 최소 형식 (name + description만)
        let minimal = r#"---
name: simple-skill
description: A simple skill that does one thing
---

Instructions for the skill...
"#;

        let skill = FileBasedSkill::parse(minimal, PathBuf::from("simple/SKILL.md")).unwrap();

        assert_eq!(skill.definition().name, "simple-skill");
        assert_eq!(skill.definition().command, "/simple-skill");
        assert_eq!(skill.definition().description, "A simple skill that does one thing");
        assert!(!skill.requires_agent_loop());  // context: fork 없으므로 false
    }

    /// OpenCode 형식 호환성 테스트
    #[test]
    fn test_opencode_format() {
        // OpenCode도 동일 형식 사용
        let opencode = r#"---
name: trello-manager
description: Manage Trello boards and cards
allowed-tools:
  - Bash
  - Read
---

Use the trello_api.py script to interact with Trello.

Available commands:
- list boards
- create card
- move card
"#;

        let skill = FileBasedSkill::parse(opencode, PathBuf::from("trello-manager/SKILL.md")).unwrap();

        assert_eq!(skill.definition().name, "trello-manager");
        assert!(skill.system_prompt().unwrap().contains("trello_api.py"));
    }

    /// 검색 경로 테스트
    #[test]
    fn test_search_paths() {
        let loader = SkillLoader::new(Path::new("/project"));

        // Claude Code, OpenCode, Codex, ForgeCode 경로가 모두 포함되어야 함
        let paths: Vec<String> = loader.search_paths.iter()
            .map(|p| p.display().to_string())
            .collect();

        // 프로젝트 레벨 경로 확인
        assert!(paths.iter().any(|p| p.contains(".claude")));
        assert!(paths.iter().any(|p| p.contains(".opencode")));
        assert!(paths.iter().any(|p| p.contains(".codex")));
        assert!(paths.iter().any(|p| p.contains(".forgecode")));
    }
}
