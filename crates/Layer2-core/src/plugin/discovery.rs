//! Plugin Discovery - 플러그인 발견 및 로드
//!
//! 파일 시스템에서 플러그인과 스킬을 발견합니다.
//! Claude Code 호환 디렉토리 구조를 지원합니다.

use super::manifest::PluginManifest;
use crate::skill::{FileBasedSkill, SkillLoader};
use crate::config::strip_json_comments;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

use forge_foundation::Result;

// ============================================================================
// DiscoveredPlugin - 발견된 플러그인
// ============================================================================

/// 발견된 플러그인 정보
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// 플러그인 매니페스트
    pub manifest: PluginManifest,

    /// 플러그인 디렉토리 경로
    pub path: PathBuf,

    /// 발견된 위치 (user, project, local)
    pub scope: PluginScope,
}

/// 플러그인 발견 범위
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginScope {
    /// 사용자 레벨 (~/.forgecode/plugins)
    User,
    /// 프로젝트 레벨 (.forgecode/plugins)
    Project,
    /// 로컬 (gitignored, .forgecode.local/plugins)
    Local,
}

impl PluginScope {
    /// 우선순위 (높을수록 우선)
    pub fn priority(&self) -> u8 {
        match self {
            PluginScope::Local => 3,
            PluginScope::Project => 2,
            PluginScope::User => 1,
        }
    }
}

// ============================================================================
// PluginDiscovery - 플러그인 발견 시스템
// ============================================================================

/// 플러그인 발견 시스템
pub struct PluginDiscovery {
    /// 검색 경로들 (우선순위 순)
    search_paths: Vec<(PathBuf, PluginScope)>,

    /// 스킬 로더
    skill_loader: SkillLoader,
}

impl PluginDiscovery {
    /// 새 발견 시스템 생성
    pub fn new(working_dir: &Path) -> Self {
        let mut search_paths = Vec::new();

        // 1. 로컬 (가장 높은 우선순위) - gitignored
        let local_dir = working_dir.join(".forgecode.local");
        search_paths.push((local_dir.join("plugins"), PluginScope::Local));
        search_paths.push((local_dir.join("skills"), PluginScope::Local));

        // 2. 프로젝트 레벨
        let project_forgecode = working_dir.join(".forgecode");
        let project_claude = working_dir.join(".claude");
        search_paths.push((project_forgecode.join("plugins"), PluginScope::Project));
        search_paths.push((project_forgecode.join("skills"), PluginScope::Project));
        search_paths.push((project_claude.join("plugins"), PluginScope::Project));
        search_paths.push((project_claude.join("skills"), PluginScope::Project));

        // 3. 사용자 레벨 (가장 낮은 우선순위)
        if let Some(home) = dirs::home_dir() {
            let user_forgecode = home.join(".forgecode");
            let user_claude = home.join(".claude");
            search_paths.push((user_forgecode.join("plugins"), PluginScope::User));
            search_paths.push((user_forgecode.join("skills"), PluginScope::User));
            search_paths.push((user_claude.join("plugins"), PluginScope::User));
            search_paths.push((user_claude.join("skills"), PluginScope::User));
        }

        Self {
            search_paths,
            skill_loader: SkillLoader::new(working_dir),
        }
    }

    /// 검색 경로 추가
    pub fn add_search_path(&mut self, path: impl Into<PathBuf>, scope: PluginScope) {
        self.search_paths.push((path.into(), scope));
    }

    // ========================================================================
    // 플러그인 발견
    // ========================================================================

    /// 모든 플러그인 발견
    pub async fn discover_plugins(&self) -> Vec<DiscoveredPlugin> {
        let mut plugins = Vec::new();

        for (path, scope) in &self.search_paths {
            if !path.exists() || path.to_string_lossy().contains("skills") {
                continue;
            }

            match self.scan_plugin_directory(path, *scope).await {
                Ok(found) => plugins.extend(found),
                Err(e) => {
                    warn!("Failed to scan plugin directory {:?}: {}", path, e);
                }
            }
        }

        // 우선순위로 정렬 (높은 우선순위 먼저)
        plugins.sort_by(|a, b| b.scope.priority().cmp(&a.scope.priority()));

        info!("Discovered {} plugins", plugins.len());
        plugins
    }

    /// 플러그인 디렉토리 스캔
    async fn scan_plugin_directory(&self, dir: &Path, scope: PluginScope) -> Result<Vec<DiscoveredPlugin>> {
        let mut plugins = Vec::new();

        let mut entries = fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            // plugin.json 찾기
            let manifest_path = path.join("plugin.json");
            if manifest_path.exists() {
                match self.parse_plugin_manifest(&manifest_path).await {
                    Ok(manifest) => {
                        debug!("Found plugin: {} at {:?}", manifest.id, path);
                        plugins.push(DiscoveredPlugin {
                            manifest,
                            path,
                            scope,
                        });
                    }
                    Err(e) => {
                        warn!("Failed to parse plugin manifest {:?}: {}", manifest_path, e);
                    }
                }
            }
        }

        Ok(plugins)
    }

    /// plugin.json 파싱
    async fn parse_plugin_manifest(&self, path: &Path) -> Result<PluginManifest> {
        let content = fs::read_to_string(path).await?;

        // JSON 코멘트 제거
        let content = strip_json_comments(&content);

        let json: PluginJsonFile = serde_json::from_str(&content)?;

        Ok(json.into_manifest())
    }

    // ========================================================================
    // 스킬 발견
    // ========================================================================

    /// 모든 스킬 발견
    pub async fn discover_skills(&self) -> Vec<FileBasedSkill> {
        let mut skills = Vec::new();

        for (path, _scope) in &self.search_paths {
            if !path.exists() || !path.to_string_lossy().contains("skills") {
                continue;
            }

            match self.scan_skill_directory(path).await {
                Ok(found) => skills.extend(found),
                Err(e) => {
                    warn!("Failed to scan skill directory {:?}: {}", path, e);
                }
            }
        }

        // SkillLoader로 모든 스킬 로드 (동기 API)
        let loader_skills = self.skill_loader.load_all();
        skills.extend(loader_skills);

        info!("Discovered {} skills", skills.len());
        skills
    }

    /// 스킬 디렉토리 스캔
    async fn scan_skill_directory(&self, dir: &Path) -> Result<Vec<FileBasedSkill>> {
        let mut skills = Vec::new();

        let mut entries = fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // 디렉토리 내 SKILL.md 확인
            if path.is_dir() {
                let skill_file = path.join("SKILL.md");
                if skill_file.exists() {
                    match FileBasedSkill::from_file(&skill_file) {
                        Ok(skill) => {
                            debug!("Found skill: {} at {:?}", skill.config().name, path);
                            skills.push(skill);
                        }
                        Err(e) => {
                            warn!("Failed to load skill {:?}: {}", skill_file, e);
                        }
                    }
                }
            }
            // 단일 SKILL.md 파일
            else if path.extension().map_or(false, |e| e == "md") {
                if path.file_name().map_or(false, |n| n.to_string_lossy().ends_with("SKILL.md")) {
                    match FileBasedSkill::from_file(&path) {
                        Ok(skill) => {
                            debug!("Found skill: {} at {:?}", skill.config().name, path);
                            skills.push(skill);
                        }
                        Err(e) => {
                            warn!("Failed to load skill {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        Ok(skills)
    }

    // ========================================================================
    // 유틸리티
    // ========================================================================

    /// 특정 ID의 플러그인 찾기
    pub async fn find_plugin(&self, id: &str) -> Option<DiscoveredPlugin> {
        let plugins = self.discover_plugins().await;
        plugins.into_iter().find(|p| p.manifest.id == id)
    }

    /// 특정 이름의 스킬 찾기
    pub async fn find_skill(&self, name: &str) -> Option<FileBasedSkill> {
        let skills = self.discover_skills().await;
        skills.into_iter().find(|s| s.config().name == name)
    }
}

// ============================================================================
// PluginJsonFile - plugin.json 파일 구조
// ============================================================================

/// plugin.json 파일 구조 (Claude Code 호환)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginJsonFile {
    /// 플러그인 ID
    pub id: String,

    /// 플러그인 이름
    pub name: String,

    /// 버전
    pub version: String,

    /// 설명
    #[serde(default)]
    pub description: String,

    /// 작성자
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// 라이센스
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// 메인 파일
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,

    /// 플러그인 타입
    #[serde(default = "default_plugin_type")]
    pub r#type: String,

    /// 제공하는 기능
    #[serde(default)]
    pub provides: PluginProvides,

    /// 의존성
    #[serde(default)]
    pub dependencies: std::collections::HashMap<String, String>,

    /// 필요한 권한
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn default_plugin_type() -> String {
    "script".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PluginProvides {
    #[serde(default)]
    pub tools: Vec<String>,

    #[serde(default)]
    pub skills: Vec<String>,
}

impl PluginJsonFile {
    /// PluginManifest로 변환
    fn into_manifest(self) -> PluginManifest {
        use super::manifest::{PluginProvides as ManifestProvides, PluginDependency, PluginVersion};

        // 버전 파싱
        let version = PluginVersion::parse(&self.version).unwrap_or_default();

        let mut manifest = PluginManifest::new(&self.id, &self.name)
            .with_version(version)
            .with_description(&self.description);

        if let Some(author) = self.author {
            manifest = manifest.with_author(&author);
        }

        // provides 변환
        let mut provides = ManifestProvides::new();
        for tool in self.provides.tools {
            provides = provides.with_tool(&tool);
        }
        for skill in self.provides.skills {
            provides = provides.with_skill(&skill);
        }
        manifest = manifest.with_provides(provides);

        // dependencies 변환
        for (name, version_str) in self.dependencies {
            let dep_version = PluginVersion::parse(&version_str).unwrap_or_default();
            manifest.dependencies.push(PluginDependency::new(&name, dep_version));
        }

        // permissions를 메타데이터에 저장
        for (i, perm) in self.permissions.into_iter().enumerate() {
            manifest = manifest.with_metadata(format!("permission_{}", i), perm);
        }

        manifest
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_plugin(dir: &Path, id: &str) {
        let plugin_dir = dir.join("plugins").join(id);
        fs::create_dir_all(&plugin_dir).await.unwrap();

        let manifest = format!(r#"{{
            "id": "{}",
            "name": "Test Plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "provides": {{
                "tools": ["test-tool"],
                "skills": ["test-skill"]
            }}
        }}"#, id);

        fs::write(plugin_dir.join("plugin.json"), manifest).await.unwrap();
    }

    async fn create_test_skill(dir: &Path, name: &str) {
        let skill_dir = dir.join("skills").join(name);
        fs::create_dir_all(&skill_dir).await.unwrap();

        let skill_content = format!(r#"---
name: {}
version: 1.0.0
description: A test skill
---

# Test Skill

This is a test skill.
"#, name);

        fs::write(skill_dir.join("SKILL.md"), skill_content).await.unwrap();
    }

    #[tokio::test]
    async fn test_discover_plugins() {
        let temp = TempDir::new().unwrap();
        let working_dir = temp.path();
        let project_dir = working_dir.join(".forgecode");

        create_test_plugin(&project_dir, "test.plugin").await;

        let discovery = PluginDiscovery::new(working_dir);
        let plugins = discovery.discover_plugins().await;

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].manifest.id, "test.plugin");
        assert_eq!(plugins[0].scope, PluginScope::Project);
    }

    #[tokio::test]
    async fn test_discover_skills() {
        let temp = TempDir::new().unwrap();
        let working_dir = temp.path();
        let project_dir = working_dir.join(".forgecode");

        create_test_skill(&project_dir, "test-skill").await;

        let discovery = PluginDiscovery::new(working_dir);
        let skills = discovery.discover_skills().await;

        // 최소한 생성한 스킬이 발견되어야 함
        assert!(skills.iter().any(|s| s.config().name == "test-skill"));
    }

    #[tokio::test]
    async fn test_scope_priority() {
        let temp = TempDir::new().unwrap();
        let working_dir = temp.path();

        // 같은 ID의 플러그인을 다른 scope에 생성
        let project_dir = working_dir.join(".forgecode");
        let local_dir = working_dir.join(".forgecode.local");

        create_test_plugin(&project_dir, "dup.plugin").await;
        create_test_plugin(&local_dir, "dup.plugin").await;

        let discovery = PluginDiscovery::new(working_dir);
        let plugins = discovery.discover_plugins().await;

        // Local이 먼저 와야 함 (높은 우선순위)
        let dup_plugins: Vec<_> = plugins.iter().filter(|p| p.manifest.id == "dup.plugin").collect();
        assert_eq!(dup_plugins.len(), 2);
        assert_eq!(dup_plugins[0].scope, PluginScope::Local);
    }

    #[tokio::test]
    async fn test_find_plugin() {
        let temp = TempDir::new().unwrap();
        let working_dir = temp.path();
        let project_dir = working_dir.join(".forgecode");

        create_test_plugin(&project_dir, "findme.plugin").await;

        let discovery = PluginDiscovery::new(working_dir);
        let found = discovery.find_plugin("findme.plugin").await;

        assert!(found.is_some());
        assert_eq!(found.unwrap().manifest.id, "findme.plugin");
    }

    #[test]
    fn test_plugin_json_parsing() {
        let json = r#"{
            "id": "test.plugin",
            "name": "Test Plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": "Test Author",
            "type": "script",
            "provides": {
                "tools": ["tool1", "tool2"],
                "skills": ["skill1"]
            },
            "dependencies": {
                "forge.core": ">=0.1.0"
            },
            "permissions": ["execute:*"]
        }"#;

        let file: PluginJsonFile = serde_json::from_str(json).unwrap();
        let manifest = file.into_manifest();

        assert_eq!(manifest.id, "test.plugin");
        assert_eq!(manifest.name, "Test Plugin");
        assert_eq!(manifest.version.to_string(), "1.0.0");
        assert_eq!(manifest.provides.tools, vec!["tool1", "tool2"]);
        assert_eq!(manifest.provides.skills, vec!["skill1"]);
        assert_eq!(manifest.dependencies.len(), 1);
        // permissions는 metadata로 저장됨
        assert!(manifest.metadata.contains_key("permission_0"));
    }
}
