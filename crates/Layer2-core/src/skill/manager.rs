//! Skill Manager - 스킬 통합 관리
//!
//! 스킬의 추가, 저장, 변경을 쉽게 할 수 있는 통합 API를 제공합니다.
//!
//! ## 사용 예시
//!
//! ```ignore
//! let manager = SkillManager::new(&working_dir).await?;
//!
//! // 마켓플레이스에서 검색
//! let skills = manager.search("commit").await;
//!
//! // 마켓플레이스에서 설치
//! manager.install_from_marketplace("commit").await?;
//!
//! // GitHub에서 직접 설치
//! manager.install("github:user/skills/my-skill").await?;
//!
//! // 스킬 교체
//! manager.replace("commit", new_content).await?;
//!
//! // 스킬 제거
//! manager.uninstall("commit").await?;
//! ```

use super::installer::SkillInstaller;
use super::loader::FileBasedSkill;
use super::marketplace::{MarketplaceSkill, SkillMarketplace};
use super::store::{InstalledSkill, SkillStore};
use crate::registry::DynamicSkillRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tracing::{info, warn};

use forge_foundation::Result;

/// 스킬 통합 관리자
///
/// 스킬의 추가, 저장, 변경을 쉽게 할 수 있는 통합 API
#[allow(dead_code)]
pub struct SkillManager {
    /// 스킬 저장소
    store: Arc<SkillStore>,

    /// 스킬 설치기
    installer: SkillInstaller,

    /// 마켓플레이스 클라이언트
    marketplace: SkillMarketplace,

    /// 동적 레지스트리 (선택적)
    registry: Option<Arc<DynamicSkillRegistry>>,

    /// 작업 디렉토리
    working_dir: PathBuf,
}

impl SkillManager {
    /// 새 스킬 관리자 생성
    pub async fn new(working_dir: &Path) -> Result<Self> {
        // 기본 스킬 디렉토리: ~/.forgecode/skills
        let base_dir = dirs::home_dir()
            .map(|h| h.join(".forgecode").join("skills"))
            .unwrap_or_else(|| working_dir.join(".forgecode").join("skills"));

        let store = Arc::new(SkillStore::new(&base_dir));
        store.load().await?;

        let installer = SkillInstaller::new(Arc::clone(&store));

        let marketplace_dir = base_dir
            .parent()
            .map(|p| p.join("marketplace"))
            .unwrap_or_else(|| base_dir.join("..").join("marketplace"));
        let marketplace = SkillMarketplace::new(marketplace_dir);
        marketplace.load_cache().await?;

        Ok(Self {
            store,
            installer,
            marketplace,
            registry: None,
            working_dir: working_dir.to_path_buf(),
        })
    }

    /// 동적 레지스트리 연결
    pub fn with_registry(mut self, registry: Arc<DynamicSkillRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    // ========================================================================
    // 마켓플레이스 - 검색
    // ========================================================================

    /// 마켓플레이스 새로고침
    pub async fn refresh_marketplace(&self) -> Result<()> {
        self.marketplace.refresh().await
    }

    /// 스킬 검색 (마켓플레이스)
    pub async fn search(&self, query: &str) -> Vec<MarketplaceSkill> {
        self.marketplace.search(query).await
    }

    /// 추천 스킬 목록
    pub async fn featured(&self) -> Vec<MarketplaceSkill> {
        self.marketplace.list_featured().await
    }

    /// 카테고리별 스킬 목록
    pub async fn browse(&self, category: &str) -> Vec<MarketplaceSkill> {
        self.marketplace.list_by_category(category).await
    }

    /// 마켓플레이스에서 스킬 찾기
    pub async fn find_in_marketplace(&self, name: &str) -> Option<MarketplaceSkill> {
        self.marketplace.find(name).await
    }

    // ========================================================================
    // 설치
    // ========================================================================

    /// 소스에서 스킬 설치 (자동 감지)
    ///
    /// - `github:user/repo/path` - GitHub에서 설치
    /// - `/path/to/skill` - 로컬 경로에서 설치
    /// - `skill-name` - 마켓플레이스에서 설치
    pub async fn install(&self, source: &str) -> Result<InstalledSkill> {
        // GitHub 또는 로컬 소스
        if source.starts_with("github:") || Path::new(source).exists() {
            return self.installer.install(source).await;
        }

        // 마켓플레이스에서 찾기
        if let Some(skill) = self.find_in_marketplace(source).await {
            return self.installer.install(&skill.source).await;
        }

        Err(forge_foundation::Error::NotFound(format!(
            "Skill '{}' not found in marketplace or as source",
            source
        )))
    }

    /// 마켓플레이스에서 스킬 설치
    pub async fn install_from_marketplace(&self, name: &str) -> Result<InstalledSkill> {
        let skill = self.find_in_marketplace(name).await.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("Skill '{}' not found in marketplace", name))
        })?;

        info!("Installing {} from marketplace...", name);
        self.installer.install(&skill.source).await
    }

    /// GitHub에서 스킬 설치
    pub async fn install_from_github(&self, source: &str) -> Result<InstalledSkill> {
        self.installer.install(source).await
    }

    /// 원시 SKILL.md 내용으로 스킬 생성
    pub async fn create(&self, name: &str, content: &str) -> Result<InstalledSkill> {
        self.installer.install_from_raw(name, content).await
    }

    // ========================================================================
    // 변경 (핵심!)
    // ========================================================================

    /// 스킬 교체 (백업 포함)
    ///
    /// 기존 스킬을 새 내용으로 교체합니다.
    /// 자동으로 백업이 생성되므로 복원이 가능합니다.
    pub async fn replace(&self, name: &str, new_content: &str) -> Result<()> {
        info!("Replacing skill: {}", name);
        self.installer.replace(name, new_content).await?;

        // 레지스트리에 반영
        if let Some(ref registry) = self.registry {
            let skill_file = self.store.skill_dir(name).join("SKILL.md");
            if skill_file.exists() {
                if let Ok(skill) = FileBasedSkill::from_file(&skill_file) {
                    let _ = registry.replace(name, Arc::new(skill), "updated").await;
                }
            }
        }

        Ok(())
    }

    /// 스킬 복원 (최신 백업에서)
    pub async fn restore(&self, name: &str) -> Result<()> {
        self.installer.restore(name).await
    }

    /// 백업 목록 조회
    pub async fn list_backups(&self, name: &str) -> Result<Vec<PathBuf>> {
        self.installer.list_backups(name).await
    }

    // ========================================================================
    // 활성화 / 비활성화
    // ========================================================================

    /// 스킬 활성화
    pub async fn enable(&self, name: &str) -> Result<bool> {
        let result = self.store.set_enabled(name, true).await?;

        // 레지스트리에 등록
        if result {
            if let Some(ref registry) = self.registry {
                let skill_file = self.store.skill_dir(name).join("SKILL.md");
                if skill_file.exists() {
                    if let Ok(skill) = FileBasedSkill::from_file(&skill_file) {
                        let _ = registry.register(Arc::new(skill)).await;
                    }
                }
            }
        }

        Ok(result)
    }

    /// 스킬 비활성화
    pub async fn disable(&self, name: &str) -> Result<bool> {
        let result = self.store.set_enabled(name, false).await?;

        // 레지스트리에서 제거
        if result {
            if let Some(ref registry) = self.registry {
                let _ = registry.unregister(name).await;
            }
        }

        Ok(result)
    }

    // ========================================================================
    // 제거
    // ========================================================================

    /// 스킬 제거
    pub async fn uninstall(&self, name: &str) -> Result<()> {
        self.installer.uninstall(name).await?;

        // 레지스트리에서도 제거
        if let Some(ref registry) = self.registry {
            let _ = registry.unregister(name).await;
        }

        Ok(())
    }

    // ========================================================================
    // 조회
    // ========================================================================

    /// 설치된 스킬 목록
    pub async fn list_installed(&self) -> Vec<InstalledSkill> {
        self.store.list().await
    }

    /// 활성화된 스킬만
    pub async fn list_enabled(&self) -> Vec<InstalledSkill> {
        self.store.list_enabled().await
    }

    /// 스킬 상세 정보
    pub async fn get(&self, name: &str) -> Option<InstalledSkill> {
        self.store.get(name).await
    }

    /// 스킬 내용 읽기
    pub async fn read_content(&self, name: &str) -> Result<String> {
        let skill_file = self.store.skill_dir(name).join("SKILL.md");

        if !skill_file.exists() {
            return Err(forge_foundation::Error::NotFound(format!(
                "Skill '{}' not found",
                name
            )));
        }

        let content = fs::read_to_string(&skill_file).await?;
        Ok(content)
    }

    /// 스킬이 설치되어 있는지 확인
    pub async fn is_installed(&self, name: &str) -> bool {
        self.store.contains(name).await
    }

    // ========================================================================
    // 동기화
    // ========================================================================

    /// 설치된 스킬을 레지스트리에 동기화
    pub async fn sync_to_registry(&self) -> Result<usize> {
        let Some(ref registry) = self.registry else {
            return Ok(0);
        };

        let enabled_skills = self.store.list_enabled().await;
        let mut synced = 0;

        for installed in enabled_skills {
            let skill_file = installed.path.join("SKILL.md");
            if skill_file.exists() {
                match FileBasedSkill::from_file(&skill_file) {
                    Ok(skill) => {
                        if let Err(e) = registry.register(Arc::new(skill)).await {
                            warn!("Failed to sync skill {}: {}", installed.name, e);
                        } else {
                            synced += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to load skill {}: {}", installed.name, e);
                    }
                }
            }
        }

        info!("Synced {} skills to registry", synced);
        Ok(synced)
    }

    // ========================================================================
    // 접근자
    // ========================================================================

    /// 스킬 저장소 접근
    pub fn store(&self) -> &Arc<SkillStore> {
        &self.store
    }

    /// 마켓플레이스 접근
    pub fn marketplace(&self) -> &SkillMarketplace {
        &self.marketplace
    }

    /// 설치기 접근
    pub fn installer(&self) -> &SkillInstaller {
        &self.installer
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_skill_content(name: &str) -> String {
        format!(
            r#"---
name: {}
description: A test skill
---

# Test Skill

This is a test skill prompt.
"#,
            name
        )
    }

    #[tokio::test]
    async fn test_create_and_replace() {
        let temp = TempDir::new().unwrap();

        // 직접 store 생성 (new는 home dir 사용)
        let store = Arc::new(SkillStore::new(temp.path().join("skills")));
        let installer = SkillInstaller::new(Arc::clone(&store));
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        let manager = SkillManager {
            store,
            installer,
            marketplace,
            registry: None,
            working_dir: temp.path().to_path_buf(),
        };

        // 생성
        let content1 = create_test_skill_content("test-skill");
        let installed = manager.create("test-skill", &content1).await.unwrap();
        assert_eq!(installed.name, "test-skill");

        // 내용 읽기
        let read_content = manager.read_content("test-skill").await.unwrap();
        assert!(read_content.contains("test-skill"));

        // 교체
        let content2 = r#"---
name: test-skill
description: Updated skill
---

# Updated prompt
"#;
        manager.replace("test-skill", content2).await.unwrap();

        // 변경 확인
        let updated_content = manager.read_content("test-skill").await.unwrap();
        assert!(updated_content.contains("Updated"));

        // 복원
        manager.restore("test-skill").await.unwrap();
        let restored_content = manager.read_content("test-skill").await.unwrap();
        assert!(restored_content.contains("test-skill"));
        assert!(!restored_content.contains("Updated"));
    }

    #[tokio::test]
    async fn test_enable_disable() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(SkillStore::new(temp.path().join("skills")));
        let installer = SkillInstaller::new(Arc::clone(&store));
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        let manager = SkillManager {
            store,
            installer,
            marketplace,
            registry: None,
            working_dir: temp.path().to_path_buf(),
        };

        // 생성
        let content = create_test_skill_content("toggle-skill");
        manager.create("toggle-skill", &content).await.unwrap();

        // 비활성화
        manager.disable("toggle-skill").await.unwrap();
        let enabled = manager.list_enabled().await;
        assert!(enabled.is_empty());

        // 활성화
        manager.enable("toggle-skill").await.unwrap();
        let enabled = manager.list_enabled().await;
        assert_eq!(enabled.len(), 1);
    }

    #[tokio::test]
    async fn test_uninstall() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(SkillStore::new(temp.path().join("skills")));
        let installer = SkillInstaller::new(Arc::clone(&store));
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        let manager = SkillManager {
            store,
            installer,
            marketplace,
            registry: None,
            working_dir: temp.path().to_path_buf(),
        };

        // 생성
        let content = create_test_skill_content("to-remove");
        manager.create("to-remove", &content).await.unwrap();
        assert!(manager.is_installed("to-remove").await);

        // 제거
        manager.uninstall("to-remove").await.unwrap();
        assert!(!manager.is_installed("to-remove").await);
    }
}
