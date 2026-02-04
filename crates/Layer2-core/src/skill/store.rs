//! Skill Store - 설치된 스킬 정보 관리
//!
//! installed_skills.json을 통해 설치된 스킬 목록을 관리합니다.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

use forge_foundation::Result;

// ============================================================================
// InstalledSkill - 설치된 스킬 정보
// ============================================================================

/// 설치된 스킬 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    /// 스킬 이름 (슬래시 명령어)
    pub name: String,

    /// 버전
    pub version: String,

    /// 설치 시간
    pub installed_at: DateTime<Utc>,

    /// 설치 소스 (예: "github:user/skills/my-skill")
    pub source: String,

    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 설치 경로
    pub path: PathBuf,

    /// 설명
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 작성자
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

fn default_true() -> bool {
    true
}

impl InstalledSkill {
    /// 새 InstalledSkill 생성
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        source: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            installed_at: Utc::now(),
            source: source.into(),
            enabled: true,
            path: path.into(),
            description: None,
            author: None,
        }
    }

    /// 설명 설정
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 작성자 설정
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }
}

// ============================================================================
// InstalledSkillsFile - installed_skills.json 구조
// ============================================================================

/// installed_skills.json 파일 구조
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkillsFile {
    /// 파일 버전
    #[serde(default = "default_version")]
    pub version: String,

    /// 설치된 스킬 목록
    #[serde(default)]
    pub skills: Vec<InstalledSkill>,
}

fn default_version() -> String {
    "1.0".to_string()
}

impl Default for InstalledSkillsFile {
    fn default() -> Self {
        Self {
            version: default_version(),
            skills: Vec::new(),
        }
    }
}

// ============================================================================
// SkillStore - 스킬 저장소
// ============================================================================

/// 스킬 저장소 - installed_skills.json 관리
pub struct SkillStore {
    /// 기본 디렉토리 (~/.forgecode/skills)
    base_dir: PathBuf,

    /// installed_skills.json 캐시
    cache: tokio::sync::RwLock<InstalledSkillsFile>,
}

impl SkillStore {
    /// 새 저장소 생성
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache: tokio::sync::RwLock::new(InstalledSkillsFile::default()),
        }
    }

    /// 사용자 홈 디렉토리 기반으로 생성
    pub fn user_store() -> Option<Self> {
        dirs::home_dir().map(|home| Self::new(home.join(".forgecode").join("skills")))
    }

    /// 기본 디렉토리 경로
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// installed_skills.json 경로
    fn installed_file(&self) -> PathBuf {
        self.base_dir.join("installed_skills.json")
    }

    /// 스킬 디렉토리 경로
    pub fn skill_dir(&self, name: &str) -> PathBuf {
        // 슬래시 제거
        let clean_name = name.trim_start_matches('/');
        self.base_dir.join(clean_name)
    }

    // ========================================================================
    // 로드 / 저장
    // ========================================================================

    /// installed_skills.json 로드
    pub async fn load(&self) -> Result<()> {
        let path = self.installed_file();

        if !path.exists() {
            debug!("installed_skills.json not found at {:?}, using empty", path);
            return Ok(());
        }

        let content = fs::read_to_string(&path).await?;
        let file: InstalledSkillsFile = serde_json::from_str(&content)?;

        *self.cache.write().await = file;

        info!(
            "Loaded {} installed skills",
            self.cache.read().await.skills.len()
        );
        Ok(())
    }

    /// installed_skills.json 저장
    pub async fn save(&self) -> Result<()> {
        // 디렉토리 생성
        if !self.base_dir.exists() {
            fs::create_dir_all(&self.base_dir).await?;
        }

        let path = self.installed_file();
        let cache = self.cache.read().await;
        let content = serde_json::to_string_pretty(&*cache)?;

        fs::write(&path, content).await?;

        debug!("Saved installed_skills.json with {} skills", cache.skills.len());
        Ok(())
    }

    // ========================================================================
    // 스킬 관리
    // ========================================================================

    /// 설치된 스킬 목록
    pub async fn list(&self) -> Vec<InstalledSkill> {
        self.cache.read().await.skills.clone()
    }

    /// 활성화된 스킬만
    pub async fn list_enabled(&self) -> Vec<InstalledSkill> {
        self.cache
            .read()
            .await
            .skills
            .iter()
            .filter(|s| s.enabled)
            .cloned()
            .collect()
    }

    /// 이름으로 스킬 조회
    pub async fn get(&self, name: &str) -> Option<InstalledSkill> {
        let clean_name = name.trim_start_matches('/');
        self.cache
            .read()
            .await
            .skills
            .iter()
            .find(|s| s.name.trim_start_matches('/') == clean_name)
            .cloned()
    }

    /// 스킬 존재 여부
    pub async fn contains(&self, name: &str) -> bool {
        let clean_name = name.trim_start_matches('/');
        self.cache
            .read()
            .await
            .skills
            .iter()
            .any(|s| s.name.trim_start_matches('/') == clean_name)
    }

    /// 스킬 설치 기록
    pub async fn record_install(&self, skill: InstalledSkill) -> Result<()> {
        {
            let mut cache = self.cache.write().await;
            let clean_name = skill.name.trim_start_matches('/');

            // 이미 존재하면 업데이트
            if let Some(existing) = cache
                .skills
                .iter_mut()
                .find(|s| s.name.trim_start_matches('/') == clean_name)
            {
                info!("Updating skill: {} -> v{}", skill.name, skill.version);
                *existing = skill;
            } else {
                info!("Installing skill: {} v{}", skill.name, skill.version);
                cache.skills.push(skill);
            }
        }

        self.save().await
    }

    /// 스킬 제거 기록
    pub async fn record_uninstall(&self, name: &str) -> Result<Option<InstalledSkill>> {
        let clean_name = name.trim_start_matches('/');

        let removed = {
            let mut cache = self.cache.write().await;
            let index = cache
                .skills
                .iter()
                .position(|s| s.name.trim_start_matches('/') == clean_name);

            if let Some(idx) = index {
                let removed = cache.skills.remove(idx);
                info!("Uninstalled skill: {}", name);
                Some(removed)
            } else {
                warn!("Skill not found for uninstall: {}", name);
                None
            }
        };

        if removed.is_some() {
            self.save().await?;
        }

        Ok(removed)
    }

    /// 스킬 활성화 상태 변경
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<bool> {
        let clean_name = name.trim_start_matches('/');

        let updated = {
            let mut cache = self.cache.write().await;

            if let Some(skill) = cache
                .skills
                .iter_mut()
                .find(|s| s.name.trim_start_matches('/') == clean_name)
            {
                skill.enabled = enabled;
                info!(
                    "Skill {} {}",
                    name,
                    if enabled { "enabled" } else { "disabled" }
                );
                true
            } else {
                false
            }
        };

        if updated {
            self.save().await?;
        }

        Ok(updated)
    }

    /// 스킬 수
    pub async fn len(&self) -> usize {
        self.cache.read().await.skills.len()
    }

    /// 비어있는지 확인
    pub async fn is_empty(&self) -> bool {
        self.cache.read().await.skills.is_empty()
    }

    // ========================================================================
    // 유틸리티
    // ========================================================================

    /// 스킬 디렉토리 존재 여부 확인
    pub async fn skill_dir_exists(&self, name: &str) -> bool {
        self.skill_dir(name).exists()
    }

    /// 스킬 디렉토리 생성
    pub async fn create_skill_dir(&self, name: &str) -> Result<PathBuf> {
        let dir = self.skill_dir(name);
        fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    /// 스킬 디렉토리 삭제
    pub async fn remove_skill_dir(&self, name: &str) -> Result<()> {
        let dir = self.skill_dir(name);
        if dir.exists() {
            fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }

    /// 이름별 스킬 맵 생성
    pub async fn as_map(&self) -> HashMap<String, InstalledSkill> {
        self.cache
            .read()
            .await
            .skills
            .iter()
            .map(|s| (s.name.clone(), s.clone()))
            .collect()
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_store() -> (SkillStore, TempDir) {
        let temp = TempDir::new().unwrap();
        let store = SkillStore::new(temp.path().join("skills"));
        (store, temp)
    }

    #[tokio::test]
    async fn test_empty_store() {
        let (store, _temp) = test_store().await;
        assert!(store.is_empty().await);
        assert!(store.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_record_install() {
        let (store, _temp) = test_store().await;

        let skill = InstalledSkill::new(
            "my-skill",
            "1.0.0",
            "github:user/skills/my-skill",
            store.skill_dir("my-skill"),
        );

        store.record_install(skill.clone()).await.unwrap();

        assert!(!store.is_empty().await);
        assert!(store.contains("my-skill").await);
        assert!(store.contains("/my-skill").await); // 슬래시 포함도 작동

        let retrieved = store.get("my-skill").await.unwrap();
        assert_eq!(retrieved.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_record_uninstall() {
        let (store, _temp) = test_store().await;

        let skill = InstalledSkill::new("my-skill", "1.0.0", "local", store.skill_dir("my-skill"));

        store.record_install(skill).await.unwrap();
        assert!(store.contains("my-skill").await);

        let removed = store.record_uninstall("my-skill").await.unwrap();
        assert!(removed.is_some());
        assert!(!store.contains("my-skill").await);
    }

    #[tokio::test]
    async fn test_set_enabled() {
        let (store, _temp) = test_store().await;

        let skill = InstalledSkill::new("my-skill", "1.0.0", "local", store.skill_dir("my-skill"));

        store.record_install(skill).await.unwrap();

        // 기본값은 enabled
        assert_eq!(store.list_enabled().await.len(), 1);

        // 비활성화
        store.set_enabled("my-skill", false).await.unwrap();
        assert!(store.list_enabled().await.is_empty());

        // 다시 활성화
        store.set_enabled("my-skill", true).await.unwrap();
        assert_eq!(store.list_enabled().await.len(), 1);
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let (store, _temp) = test_store().await;

        let skill = InstalledSkill::new(
            "my-skill",
            "1.0.0",
            "github:user/skills/my-skill",
            store.skill_dir("my-skill"),
        )
        .with_description("A test skill")
        .with_author("Test Author");

        store.record_install(skill).await.unwrap();

        // 새 store 인스턴스로 로드
        let store2 = SkillStore::new(store.base_dir());
        store2.load().await.unwrap();

        let retrieved = store2.get("my-skill").await.unwrap();
        assert_eq!(retrieved.description, Some("A test skill".to_string()));
        assert_eq!(retrieved.author, Some("Test Author".to_string()));
    }

    #[tokio::test]
    async fn test_update_existing() {
        let (store, _temp) = test_store().await;

        let skill1 = InstalledSkill::new("my-skill", "1.0.0", "local", store.skill_dir("my-skill"));

        store.record_install(skill1).await.unwrap();

        // 업데이트
        let skill2 = InstalledSkill::new("my-skill", "2.0.0", "local", store.skill_dir("my-skill"));

        store.record_install(skill2).await.unwrap();

        // 하나만 있어야 함
        assert_eq!(store.len().await, 1);

        // 버전이 업데이트 됨
        let retrieved = store.get("my-skill").await.unwrap();
        assert_eq!(retrieved.version, "2.0.0");
    }
}
