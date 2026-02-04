//! Explain Skill - 코드 설명 생성
//!
//! 코드 파일이나 함수에 대한 상세한 설명을 생성

use crate::skill::{
    Skill, SkillArgument, SkillContext, SkillDefinition, SkillInput, SkillMetadata, SkillOutput,
};
use async_trait::async_trait;
use forge_foundation::Result;

/// 코드 설명 스킬
pub struct ExplainSkill;

impl ExplainSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExplainSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ExplainSkill {
    fn definition(&self) -> SkillDefinition {
        SkillDefinition {
            name: "explain".into(),
            command: "/explain".into(),
            description: "Explain code in detail".into(),
            usage: "/explain [FILE | FUNCTION | CONCEPT] [--depth <level>]".into(),
            arguments: vec![
                SkillArgument {
                    name: "target".into(),
                    description: "File path, function name, or concept to explain".into(),
                    required: false,
                    default: None,
                    short_flag: None,
                    long_flag: None,
                },
                SkillArgument {
                    name: "depth".into(),
                    description: "Explanation depth: brief, normal, detailed".into(),
                    required: false,
                    default: Some("normal".into()),
                    short_flag: Some("-d".into()),
                    long_flag: Some("--depth".into()),
                },
                SkillArgument {
                    name: "audience".into(),
                    description: "Target audience: beginner, intermediate, expert".into(),
                    required: false,
                    default: Some("intermediate".into()),
                    short_flag: Some("-a".into()),
                    long_flag: Some("--audience".into()),
                },
            ],
            category: "code".into(),
            user_invocable: true,
        }
    }

    fn metadata(&self) -> SkillMetadata {
        SkillMetadata {
            name: "explain".into(),
            version: "1.0.0".into(),
            author: Some("ForgeCode".into()),
            source: None,
            required_tools: vec!["read".into(), "grep".into()],
            required_permissions: vec![],
            tags: vec!["explain".into(), "documentation".into()],
            hidden: false,
        }
    }

    fn system_prompt(&self) -> Option<String> {
        Some(
            r#"You are a code explanation assistant. Your task is to:

1. Read and understand the target code
2. Explain it clearly and thoroughly
3. Provide relevant examples if helpful

Explanation guidelines:
- Start with a high-level overview
- Break down complex parts step by step
- Use analogies for difficult concepts
- Highlight important patterns and design decisions
- Mention potential gotchas or edge cases
- Adjust language complexity to the audience level

For files: Explain the overall purpose, structure, and key components
For functions: Explain parameters, return values, algorithm, and usage
For concepts: Explain the theory, implementation, and practical applications"#
                .into(),
        )
    }

    fn requires_agent_loop(&self) -> bool {
        true
    }

    async fn execute(&self, _ctx: &SkillContext<'_>, input: SkillInput) -> Result<SkillOutput> {
        let target = input.positional_args.first().cloned();
        let depth = input.get("depth").or(input.get("d"))
            .cloned()
            .unwrap_or_else(|| "normal".to_string());
        let audience = input.get("audience").or(input.get("a"))
            .cloned()
            .unwrap_or_else(|| "intermediate".to_string());

        let target_desc = target.as_deref().unwrap_or("recent context");

        let prompt = format!(
            "Explain: {}. Depth: {}. Audience: {}",
            target_desc, depth, audience
        );

        Ok(SkillOutput::success(prompt)
            .with_data(serde_json::json!({
                "requires_agent_loop": true,
                "target": target,
                "depth": depth,
                "audience": audience,
            }))
            .with_summary(format!("Ready to explain: {}", target_desc)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_definition() {
        let skill = ExplainSkill::new();
        let def = skill.definition();

        assert_eq!(def.command, "/explain");
        assert_eq!(def.category, "code");
    }
}
