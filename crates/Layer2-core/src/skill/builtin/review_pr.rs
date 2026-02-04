//! Review PR Skill - Pull Request 리뷰 자동화
//!
//! Claude Code의 /review-pr 스킬과 유사하게 동작:
//! 1. PR 정보 및 diff 가져오기
//! 2. 코드 변경사항 분석
//! 3. 리뷰 코멘트 생성

use crate::skill::{
    Skill, SkillArgument, SkillContext, SkillDefinition, SkillInput, SkillMetadata, SkillOutput,
};
use async_trait::async_trait;
use forge_foundation::Result;

/// Pull Request 리뷰 스킬
pub struct ReviewPrSkill;

impl ReviewPrSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReviewPrSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ReviewPrSkill {
    fn definition(&self) -> SkillDefinition {
        SkillDefinition {
            name: "review-pr".into(),
            command: "/review-pr".into(),
            description: "Review a pull request and provide feedback".into(),
            usage: "/review-pr [PR_NUMBER | PR_URL] [--focus <area>]".into(),
            arguments: vec![
                SkillArgument {
                    name: "pr".into(),
                    description: "PR number or URL (uses current branch if not specified)".into(),
                    required: false,
                    default: None,
                    short_flag: None,
                    long_flag: None,
                },
                SkillArgument {
                    name: "focus".into(),
                    description: "Focus area: security, performance, style, logic".into(),
                    required: false,
                    default: None,
                    short_flag: Some("-f".into()),
                    long_flag: Some("--focus".into()),
                },
                SkillArgument {
                    name: "output".into(),
                    description: "Output format: markdown, json, github".into(),
                    required: false,
                    default: Some("markdown".into()),
                    short_flag: Some("-o".into()),
                    long_flag: Some("--output".into()),
                },
            ],
            category: "git".into(),
            user_invocable: true,
        }
    }

    fn metadata(&self) -> SkillMetadata {
        SkillMetadata {
            name: "review-pr".into(),
            version: "1.0.0".into(),
            author: Some("ForgeCode".into()),
            source: None,
            required_tools: vec!["bash".into(), "read".into()],
            required_permissions: vec!["execute".into()],
            tags: vec!["git".into(), "review".into(), "pr".into()],
            hidden: false,
        }
    }

    fn system_prompt(&self) -> Option<String> {
        Some(
            r#"You are a code reviewer assistant. Your task is to:

1. Fetch the PR diff and related information
2. Analyze the changes thoroughly
3. Provide constructive feedback

Review guidelines:
- Focus on logic errors, security issues, and performance problems
- Suggest improvements with code examples when helpful
- Be constructive and respectful
- Prioritize issues by severity (critical, major, minor, suggestion)
- Consider the context and project conventions

Output format:
## Summary
Brief overview of the PR changes

## Issues Found
### Critical
- ...
### Major
- ...
### Minor
- ...

## Suggestions
- ...

## Positive Aspects
- ..."#
                .into(),
        )
    }

    fn requires_agent_loop(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &SkillContext<'_>, input: SkillInput) -> Result<SkillOutput> {
        let pr_ref = input.positional_args.first().cloned();
        let focus = input.get("focus").or(input.get("f")).cloned();
        let output_format = input.get("output").or(input.get("o"))
            .cloned()
            .unwrap_or_else(|| "markdown".to_string());

        // PR 정보 결정
        let pr_target = if let Some(ref pr) = pr_ref {
            pr.clone()
        } else if let Some(ref git_info) = ctx.git_info {
            format!("current branch: {}", git_info.branch)
        } else {
            "current branch".to_string()
        };

        let prompt = format!(
            "Review the pull request: {}. Focus: {:?}. Output format: {}",
            pr_target,
            focus,
            output_format
        );

        Ok(SkillOutput::success(prompt)
            .with_data(serde_json::json!({
                "requires_agent_loop": true,
                "pr_ref": pr_ref,
                "focus": focus,
                "output_format": output_format,
            }))
            .with_summary(format!("Ready to review PR: {}", pr_target)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_pr_definition() {
        let skill = ReviewPrSkill::new();
        let def = skill.definition();

        assert_eq!(def.command, "/review-pr");
        assert!(skill.requires_agent_loop());
    }
}
