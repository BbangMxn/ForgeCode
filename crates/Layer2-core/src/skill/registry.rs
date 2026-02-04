//! Skill Registry - 스킬 관리 및 조회

use super::traits::{Skill, SkillDefinition};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// 스킬 레지스트리 - 모든 스킬을 관리
pub struct SkillRegistry {
    /// 명령어로 인덱싱된 스킬 (예: "/commit" -> CommitSkill)
    skills_by_command: HashMap<String, Arc<dyn Skill>>,

    /// 이름으로 인덱싱된 스킬 (예: "commit" -> CommitSkill)
    skills_by_name: HashMap<String, Arc<dyn Skill>>,

    /// 카테고리별 스킬 목록
    skills_by_category: HashMap<String, Vec<String>>,
}

impl SkillRegistry {
    /// 새 레지스트리 생성
    pub fn new() -> Self {
        Self {
            skills_by_command: HashMap::new(),
            skills_by_name: HashMap::new(),
            skills_by_category: HashMap::new(),
        }
    }

    /// 빌트인 스킬로 초기화된 레지스트리 생성
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register_builtins();
        registry
    }

    /// 빌트인 스킬 등록
    pub fn register_builtins(&mut self) {
        use super::builtin::*;

        self.register(Arc::new(CommitSkill::new()));
        self.register(Arc::new(ReviewPrSkill::new()));
        self.register(Arc::new(ExplainSkill::new()));

        info!("Registered {} built-in skills", self.skills_by_name.len());
    }

    /// 스킬 등록
    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        let def = skill.definition();

        debug!("Registering skill: {} ({})", def.name, def.command);

        // 명령어로 등록
        self.skills_by_command.insert(def.command.clone(), Arc::clone(&skill));

        // 이름으로 등록
        self.skills_by_name.insert(def.name.clone(), Arc::clone(&skill));

        // 카테고리에 추가
        self.skills_by_category
            .entry(def.category.clone())
            .or_default()
            .push(def.name.clone());
    }

    /// 스킬 등록 해제
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn Skill>> {
        if let Some(skill) = self.skills_by_name.remove(name) {
            let def = skill.definition();
            self.skills_by_command.remove(&def.command);

            // 카테고리에서 제거
            if let Some(skills) = self.skills_by_category.get_mut(&def.category) {
                skills.retain(|n| n != name);
            }

            debug!("Unregistered skill: {}", name);
            Some(skill)
        } else {
            None
        }
    }

    /// 명령어로 스킬 조회 (예: "/commit")
    pub fn get_by_command(&self, command: &str) -> Option<Arc<dyn Skill>> {
        // 명령어 정규화 (앞에 /가 없으면 추가)
        let normalized = if command.starts_with('/') {
            command.to_string()
        } else {
            format!("/{}", command)
        };

        self.skills_by_command.get(&normalized).cloned()
    }

    /// 이름으로 스킬 조회 (예: "commit")
    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.skills_by_name.get(name).cloned()
    }

    /// 명령어 문자열에서 스킬 찾기
    ///
    /// 입력 문자열이 스킬 명령어로 시작하면 해당 스킬 반환
    pub fn find_for_input(&self, input: &str) -> Option<Arc<dyn Skill>> {
        let trimmed = input.trim();

        // /로 시작하는지 확인
        if !trimmed.starts_with('/') {
            return None;
        }

        // 첫 단어 추출 (명령어)
        let command = trimmed.split_whitespace().next()?;

        self.get_by_command(command)
    }

    /// 입력이 스킬 명령어인지 확인
    pub fn is_skill_command(&self, input: &str) -> bool {
        self.find_for_input(input).is_some()
    }

    /// 모든 스킬 정의 반환
    pub fn definitions(&self) -> Vec<SkillDefinition> {
        self.skills_by_name
            .values()
            .map(|s| s.definition())
            .collect()
    }

    /// 사용자 호출 가능한 스킬 정의만 반환
    pub fn user_invocable_definitions(&self) -> Vec<SkillDefinition> {
        self.skills_by_name
            .values()
            .map(|s| s.definition())
            .filter(|d| d.user_invocable)
            .collect()
    }

    /// 카테고리별 스킬 목록
    pub fn by_category(&self, category: &str) -> Vec<Arc<dyn Skill>> {
        self.skills_by_category
            .get(category)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.skills_by_name.get(name).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 모든 카테고리 목록
    pub fn categories(&self) -> Vec<&String> {
        self.skills_by_category.keys().collect()
    }

    /// 스킬 수
    pub fn len(&self) -> usize {
        self.skills_by_name.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.skills_by_name.is_empty()
    }

    /// 도움말 생성
    pub fn generate_help(&self) -> String {
        let mut help = String::from("# Available Skills\n\n");

        for category in self.categories() {
            help.push_str(&format!("## {}\n\n", category));

            for skill in self.by_category(category) {
                let def = skill.definition();
                help.push_str(&format!("### {}\n", def.command));
                help.push_str(&format!("{}\n\n", def.description));
                help.push_str(&format!("**Usage:** `{}`\n\n", def.usage));

                if !def.arguments.is_empty() {
                    help.push_str("**Arguments:**\n");
                    for arg in &def.arguments {
                        let flags = match (&arg.short_flag, &arg.long_flag) {
                            (Some(s), Some(l)) => format!("{}, {}", s, l),
                            (Some(s), None) => s.clone(),
                            (None, Some(l)) => l.clone(),
                            (None, None) => arg.name.clone(),
                        };
                        let required = if arg.required { " (required)" } else { "" };
                        help.push_str(&format!("- `{}`: {}{}\n", flags, arg.description, required));
                    }
                    help.push('\n');
                }
            }
        }

        help
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::builtin::CommitSkill;

    #[test]
    fn test_skill_registration() {
        let mut registry = SkillRegistry::new();
        registry.register(Arc::new(CommitSkill::new()));

        assert_eq!(registry.len(), 1);
        assert!(registry.get_by_command("/commit").is_some());
        assert!(registry.get_by_name("commit").is_some());
    }

    #[test]
    fn test_find_for_input() {
        let mut registry = SkillRegistry::new();
        registry.register(Arc::new(CommitSkill::new()));

        assert!(registry.find_for_input("/commit -m 'test'").is_some());
        assert!(registry.find_for_input("just a message").is_none());
        assert!(registry.is_skill_command("/commit"));
        assert!(!registry.is_skill_command("hello"));
    }

    #[test]
    fn test_unregister() {
        let mut registry = SkillRegistry::new();
        registry.register(Arc::new(CommitSkill::new()));

        assert!(registry.unregister("commit").is_some());
        assert!(registry.is_empty());
    }
}
