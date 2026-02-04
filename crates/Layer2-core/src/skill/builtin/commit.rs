//! Commit Skill - Git 커밋 자동화
//!
//! Claude Code의 /commit 스킬과 유사하게 동작:
//! 1. git status로 변경사항 확인
//! 2. git diff로 변경 내용 분석
//! 3. 의미있는 커밋 메시지 생성
//! 4. 커밋 실행

use crate::skill::{
    Skill, SkillArgument, SkillContext, SkillDefinition, SkillInput, SkillMetadata, SkillOutput,
};
use async_trait::async_trait;
use forge_foundation::Result;

/// Git 커밋 자동화 스킬
pub struct CommitSkill;

impl CommitSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CommitSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for CommitSkill {
    fn definition(&self) -> SkillDefinition {
        SkillDefinition {
            name: "commit".into(),
            command: "/commit".into(),
            description: "Analyze changes and create a meaningful git commit".into(),
            usage: "/commit [-m <message>] [--all] [--amend]".into(),
            arguments: vec![
                SkillArgument {
                    name: "message".into(),
                    description: "Optional commit message (auto-generated if not provided)".into(),
                    required: false,
                    default: None,
                    short_flag: Some("-m".into()),
                    long_flag: Some("--message".into()),
                },
                SkillArgument {
                    name: "all".into(),
                    description: "Stage all changes before committing".into(),
                    required: false,
                    default: Some("false".into()),
                    short_flag: Some("-a".into()),
                    long_flag: Some("--all".into()),
                },
                SkillArgument {
                    name: "amend".into(),
                    description: "Amend the previous commit".into(),
                    required: false,
                    default: Some("false".into()),
                    short_flag: None,
                    long_flag: Some("--amend".into()),
                },
            ],
            category: "git".into(),
            user_invocable: true,
        }
    }

    fn metadata(&self) -> SkillMetadata {
        SkillMetadata {
            name: "commit".into(),
            version: "1.0.0".into(),
            author: Some("ForgeCode".into()),
            source: None,
            required_tools: vec!["bash".into()],
            required_permissions: vec!["execute".into()],
            tags: vec!["git".into(), "vcs".into()],
            hidden: false,
        }
    }

    fn system_prompt(&self) -> Option<String> {
        Some(
            r#"You are a Git commit assistant. Your task is to:

1. Analyze the staged changes (or all changes if --all flag is used)
2. Generate a meaningful commit message following conventional commit format
3. Create the commit

Guidelines for commit messages:
- Use conventional commit format: type(scope): description
- Types: feat, fix, docs, style, refactor, test, chore
- Keep the first line under 72 characters
- Add a body with more details if needed
- Focus on the "why" rather than the "what"

Do NOT push to remote unless explicitly asked."#
                .into(),
        )
    }

    fn requires_agent_loop(&self) -> bool {
        // Commit skill은 에이전트 루프를 통해 diff 분석 후 커밋
        true
    }

    async fn execute(&self, ctx: &SkillContext<'_>, input: SkillInput) -> Result<SkillOutput> {
        // 이 스킬은 에이전트 루프가 필요하므로, 여기서는 기본 정보만 제공
        // 실제 실행은 Layer3-agent에서 에이전트 루프를 통해 처리

        let message = input.get("m").or(input.get("message")).cloned();
        let stage_all = input.has_flag("a") || input.has_flag("all");
        let amend = input.has_flag("amend");

        // Git 정보 수집을 위한 프롬프트 생성
        let prompt = if let Some(ref msg) = message {
            format!(
                "Create a git commit with the message: '{}'. Stage all: {}. Amend: {}",
                msg, stage_all, amend
            )
        } else {
            format!(
                "Analyze the current changes and create a meaningful git commit. Stage all: {}. Amend: {}",
                stage_all, amend
            )
        };

        // 에이전트 루프에서 사용할 데이터 반환
        Ok(SkillOutput::success(prompt)
            .with_data(serde_json::json!({
                "requires_agent_loop": true,
                "stage_all": stage_all,
                "amend": amend,
                "message": message,
                "git_info": ctx.git_info.as_ref().map(|g| serde_json::json!({
                    "branch": g.branch,
                    "changed_files": g.changed_files,
                    "staged_files": g.staged_files,
                })),
            }))
            .with_summary("Ready to create commit"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_skill_definition() {
        let skill = CommitSkill::new();
        let def = skill.definition();

        assert_eq!(def.command, "/commit");
        assert_eq!(def.category, "git");
        assert!(def.user_invocable);
    }

    #[test]
    fn test_commit_skill_requires_agent() {
        let skill = CommitSkill::new();
        assert!(skill.requires_agent_loop());
    }

    #[test]
    fn test_parse_commit_input() {
        let skill = CommitSkill::new();
        let input = skill.parse_input("/commit -m 'fix bug' --all");

        assert_eq!(input.get("m"), Some(&"'fix".to_string())); // Simple split
        assert!(input.has_flag("all"));
    }
}
