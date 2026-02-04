//! Skill Installer - 스킬 설치 및 관리
//!
//! 로컬 파일이나 GitHub에서 스킬을 설치하고 관리합니다.
//! 주요 기능: 스킬 교체, 백업, 복원

use super::loader::FileBasedSkill;
use super::store::{InstalledSkill, SkillStore};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tracing::{info, warn};

use forge_foundation::Result;

// ============================================================================
// SkillSource - 스킬 소스
// ============================================================================

/// 스킬 설치 소스
#[derive(Debug, Clone)]
pub enum SkillSource {
    /// GitHub 저장소 (owner/repo/path@tag)
    GitHub {
        owner: String,
        repo: String,
        path: String,
        tag: Option<String>,
    },
    /// 로컬 경로
    Local(PathBuf),
    /// 원시 SKILL.md 내용
    Raw { name: String, content: String },
}

impl SkillSource {
    /// GitHub URL 파싱 (예: "github:user/repo/skills/my-skill@v1.0")
    pub fn parse_github(source: &str) -> Option<Self> {
        let source = source.strip_prefix("github:")?;

        // @로 tag 분리
        let (repo_path, tag) = if let Some(idx) = source.rfind('@') {
            let (path, tag) = source.split_at(idx);
            (path, Some(tag[1..].to_string()))
        } else {
            (source, None)
        };

        // owner/repo/path 분리
        let parts: Vec<&str> = repo_path.split('/').collect();
        if parts.len() < 3 {
            return None;
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let path = parts[2..].join("/");

        Some(SkillSource::GitHub {
            owner,
            repo,
            path,
            tag,
        })
    }

    /// 소스 문자열에서 파싱
    pub fn parse(source: &str) -> Option<Self> {
        if source.starts_with("github:") {
            Self::parse_github(source)
        } else {
            // 로컬 경로로 간주
            let path = PathBuf::from(source);
            if path.exists() {
                Some(SkillSource::Local(path))
            } else {
                None
            }
        }
    }

    /// 소스 문자열로 변환
    pub fn to_source_string(&self) -> String {
        match self {
            SkillSource::GitHub {
                owner,
                repo,
                path,
                tag,
            } => {
                let mut s = format!("github:{}/{}/{}", owner, repo, path);
                if let Some(t) = tag {
                    s.push('@');
                    s.push_str(t);
                }
                s
            }
            SkillSource::Local(path) => path.to_string_lossy().to_string(),
            SkillSource::Raw { name, .. } => format!("raw:{}", name),
        }
    }
}

// ============================================================================
// SkillInstaller - 스킬 설치기
// ============================================================================

/// 스킬 설치 및 관리
pub struct SkillInstaller {
    /// 스킬 저장소
    store: Arc<SkillStore>,

    /// HTTP 클라이언트
    client: Client,

    /// 백업 디렉토리
    backup_dir: PathBuf,
}

impl SkillInstaller {
    /// 새 설치기 생성
    pub fn new(store: Arc<SkillStore>) -> Self {
        let backup_dir = store.base_dir().join(".backups");
        Self {
            store,
            client: Client::new(),
            backup_dir,
        }
    }

    // ========================================================================
    // 설치
    // ========================================================================

    /// 소스에서 스킬 설치
    pub async fn install(&self, source: &str) -> Result<InstalledSkill> {
        let skill_source = SkillSource::parse(source).ok_or_else(|| {
            forge_foundation::Error::InvalidInput(format!("Invalid skill source: {}", source))
        })?;

        match skill_source {
            SkillSource::GitHub {
                owner,
                repo,
                path,
                tag,
            } => {
                self.install_from_github(&owner, &repo, &path, tag.as_deref())
                    .await
            }
            SkillSource::Local(path) => self.install_from_path(&path).await,
            SkillSource::Raw { name, content } => self.install_from_raw(&name, &content).await,
        }
    }

    /// GitHub에서 스킬 설치
    pub async fn install_from_github(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        tag: Option<&str>,
    ) -> Result<InstalledSkill> {
        let tag = tag.unwrap_or("main");
        info!(
            "Installing skill from github:{}/{}/{}@{}",
            owner, repo, path, tag
        );

        // GitHub raw URL로 SKILL.md 다운로드
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}/SKILL.md",
            owner, repo, tag, path
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "ForgeCode")
            .send()
            .await
            .map_err(|e| forge_foundation::Error::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(forge_foundation::Error::Http(format!(
                "Failed to download SKILL.md: HTTP {}",
                response.status()
            )));
        }

        let content = response
            .text()
            .await
            .map_err(|e| forge_foundation::Error::Http(e.to_string()))?;

        // 스킬 이름 추출 (path의 마지막 부분)
        let skill_name = path.rsplit('/').next().unwrap_or(path);

        // 스킬 디렉토리 생성 및 파일 저장
        let skill_dir = self.store.create_skill_dir(skill_name).await?;
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, &content).await?;

        // 스킬 로드하여 메타데이터 추출
        let skill = FileBasedSkill::from_file(&skill_file)?;
        let config = skill.config();

        // 설치 기록
        let source_str = format!("github:{}/{}/{}@{}", owner, repo, path, tag);
        let mut installed = InstalledSkill::new(
            &config.name,
            "1.0.0", // SKILL.md에 버전이 없으면 기본값
            &source_str,
            &skill_dir,
        );

        if let Some(desc) = &config.description {
            installed = installed.with_description(desc);
        }

        self.store.record_install(installed.clone()).await?;

        info!("Installed skill: {}", config.name);
        Ok(installed)
    }

    /// 로컬 경로에서 스킬 설치
    pub async fn install_from_path(&self, source_path: &Path) -> Result<InstalledSkill> {
        info!("Installing skill from {:?}", source_path);

        // SKILL.md 찾기
        let skill_file = if source_path.is_dir() {
            source_path.join("SKILL.md")
        } else {
            source_path.to_path_buf()
        };

        if !skill_file.exists() {
            return Err(forge_foundation::Error::NotFound(
                "SKILL.md not found".into(),
            ));
        }

        // 스킬 로드
        let skill = FileBasedSkill::from_file(&skill_file)?;
        let config = skill.config();

        // 스킬 디렉토리 생성 및 복사
        let skill_dir = self.store.create_skill_dir(&config.name).await?;
        let dest_file = skill_dir.join("SKILL.md");

        // 전체 디렉토리 복사 또는 단일 파일 복사
        if source_path.is_dir() {
            self.copy_dir_recursive(source_path, &skill_dir).await?;
        } else {
            fs::copy(&skill_file, &dest_file).await?;
        }

        // 설치 기록
        let source_str = source_path.to_string_lossy().to_string();
        let mut installed = InstalledSkill::new(&config.name, "1.0.0", &source_str, &skill_dir);

        if let Some(desc) = &config.description {
            installed = installed.with_description(desc);
        }

        self.store.record_install(installed.clone()).await?;

        info!("Installed skill: {}", config.name);
        Ok(installed)
    }

    /// 원시 SKILL.md 내용으로 스킬 설치 (인라인 생성)
    pub async fn install_from_raw(&self, name: &str, content: &str) -> Result<InstalledSkill> {
        info!("Installing skill from raw content: {}", name);

        // 스킬 디렉토리 생성
        let skill_dir = self.store.create_skill_dir(name).await?;
        let skill_file = skill_dir.join("SKILL.md");

        // 파일 저장
        fs::write(&skill_file, content).await?;

        // 스킬 로드하여 검증
        let skill = FileBasedSkill::from_file(&skill_file)?;
        let config = skill.config();

        // 설치 기록
        let mut installed =
            InstalledSkill::new(&config.name, "1.0.0", format!("raw:{}", name), &skill_dir);

        if let Some(desc) = &config.description {
            installed = installed.with_description(desc);
        }

        self.store.record_install(installed.clone()).await?;

        info!("Installed skill: {}", config.name);
        Ok(installed)
    }

    // ========================================================================
    // 교체 (핵심 기능!)
    // ========================================================================

    /// 스킬 교체 (백업 후 새 스킬로 교체)
    pub async fn replace(&self, name: &str, new_content: &str) -> Result<()> {
        info!("Replacing skill: {}", name);

        // 기존 스킬 백업
        self.backup(name).await?;

        // 스킬 파일 교체
        let skill_dir = self.store.skill_dir(name);
        let skill_file = skill_dir.join("SKILL.md");

        if !skill_dir.exists() {
            return Err(forge_foundation::Error::NotFound(format!(
                "Skill {} not found",
                name
            )));
        }

        fs::write(&skill_file, new_content).await?;

        // 검증
        if let Err(e) = FileBasedSkill::from_file(&skill_file) {
            warn!("Invalid SKILL.md, restoring backup: {}", e);
            self.restore(name).await?;
            return Err(e.into());
        }

        info!("Skill {} replaced successfully", name);
        Ok(())
    }

    // ========================================================================
    // 백업 / 복원
    // ========================================================================

    /// 스킬 백업
    pub async fn backup(&self, name: &str) -> Result<PathBuf> {
        let skill_dir = self.store.skill_dir(name);
        let skill_file = skill_dir.join("SKILL.md");

        if !skill_file.exists() {
            return Err(forge_foundation::Error::NotFound(format!(
                "Skill {} not found",
                name
            )));
        }

        // 백업 디렉토리 생성
        if !self.backup_dir.exists() {
            fs::create_dir_all(&self.backup_dir).await?;
        }

        // 타임스탬프로 백업
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("{}_{}.md", name, timestamp);
        let backup_path = self.backup_dir.join(&backup_name);

        fs::copy(&skill_file, &backup_path).await?;

        info!("Backed up skill {} to {:?}", name, backup_path);
        Ok(backup_path)
    }

    /// 스킬 복원 (최신 백업에서)
    pub async fn restore(&self, name: &str) -> Result<()> {
        let backup_pattern = format!("{}_", name);

        // 가장 최신 백업 찾기
        let mut latest_backup: Option<(PathBuf, String)> = None;

        if self.backup_dir.exists() {
            let mut entries = fs::read_dir(&self.backup_dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.starts_with(&backup_pattern) {
                    if latest_backup
                        .as_ref()
                        .map_or(true, |(_, old)| &file_name > old)
                    {
                        latest_backup = Some((entry.path(), file_name));
                    }
                }
            }
        }

        let (backup_path, _) = latest_backup.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("No backup found for skill {}", name))
        })?;

        // 복원
        let skill_file = self.store.skill_dir(name).join("SKILL.md");
        fs::copy(&backup_path, &skill_file).await?;

        info!("Restored skill {} from {:?}", name, backup_path);
        Ok(())
    }

    /// 백업 목록 조회
    pub async fn list_backups(&self, name: &str) -> Result<Vec<PathBuf>> {
        let backup_pattern = format!("{}_", name);
        let mut backups = Vec::new();

        if self.backup_dir.exists() {
            let mut entries = fs::read_dir(&self.backup_dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.starts_with(&backup_pattern) {
                    backups.push(entry.path());
                }
            }
        }

        backups.sort();
        backups.reverse(); // 최신순
        Ok(backups)
    }

    // ========================================================================
    // 제거
    // ========================================================================

    /// 스킬 제거
    pub async fn uninstall(&self, name: &str) -> Result<()> {
        info!("Uninstalling skill: {}", name);

        // 백업 생성
        let _ = self.backup(name).await;

        // 디렉토리 삭제
        self.store.remove_skill_dir(name).await?;

        // 기록에서 제거
        self.store.record_uninstall(name).await?;

        info!("Uninstalled skill: {}", name);
        Ok(())
    }

    // ========================================================================
    // 유틸리티
    // ========================================================================

    /// 디렉토리 재귀 복사
    async fn copy_dir_recursive(&self, src: &Path, dest: &Path) -> Result<()> {
        if !dest.exists() {
            fs::create_dir_all(dest).await?;
        }

        let mut entries = fs::read_dir(src).await?;

        while let Some(entry) = entries.next_entry().await? {
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if src_path.is_dir() {
                Box::pin(self.copy_dir_recursive(&src_path, &dest_path)).await?;
            } else {
                fs::copy(&src_path, &dest_path).await?;
            }
        }

        Ok(())
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

    #[test]
    fn test_parse_github_source() {
        let source = SkillSource::parse_github("github:user/repo/skills/my-skill").unwrap();
        if let SkillSource::GitHub {
            owner,
            repo,
            path,
            tag,
        } = source
        {
            assert_eq!(owner, "user");
            assert_eq!(repo, "repo");
            assert_eq!(path, "skills/my-skill");
            assert!(tag.is_none());
        } else {
            panic!("Expected GitHub source, got {:?}", source);
        }

        let source = SkillSource::parse_github("github:user/repo/skills/my-skill@v1.0.0").unwrap();
        if let SkillSource::GitHub { tag, .. } = source {
            assert_eq!(tag, Some("v1.0.0".to_string()));
        }
    }

    #[tokio::test]
    async fn test_install_from_raw() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(SkillStore::new(temp.path().join("skills")));
        let installer = SkillInstaller::new(store.clone());

        let content = create_test_skill_content("test-skill");
        let result = installer.install_from_raw("test-skill", &content).await;

        assert!(result.is_ok());
        let installed = result.unwrap();
        assert_eq!(installed.name, "test-skill");
        assert!(store.contains("test-skill").await);
    }

    #[tokio::test]
    async fn test_replace_skill() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(SkillStore::new(temp.path().join("skills")));
        let installer = SkillInstaller::new(store.clone());

        // 먼저 설치
        let content1 = create_test_skill_content("replaceable");
        installer
            .install_from_raw("replaceable", &content1)
            .await
            .unwrap();

        // 교체
        let content2 = r#"---
name: replaceable
description: Updated skill
---

# Updated Skill

New prompt content.
"#;
        let result = installer.replace("replaceable", content2).await;
        assert!(result.is_ok());

        // 백업 확인
        let backups = installer.list_backups("replaceable").await.unwrap();
        assert_eq!(backups.len(), 1);
    }

    #[tokio::test]
    async fn test_backup_and_restore() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(SkillStore::new(temp.path().join("skills")));
        let installer = SkillInstaller::new(store.clone());

        // 설치
        let original_content = create_test_skill_content("backup-test");
        installer
            .install_from_raw("backup-test", &original_content)
            .await
            .unwrap();

        // 백업
        let backup_path = installer.backup("backup-test").await.unwrap();
        assert!(backup_path.exists());

        // 파일 수정
        let skill_file = store.skill_dir("backup-test").join("SKILL.md");
        fs::write(&skill_file, "corrupted content").await.unwrap();

        // 복원
        installer.restore("backup-test").await.unwrap();

        // 복원된 내용 확인
        let restored_content = fs::read_to_string(&skill_file).await.unwrap();
        assert_eq!(restored_content, original_content);
    }

    #[tokio::test]
    async fn test_uninstall() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(SkillStore::new(temp.path().join("skills")));
        let installer = SkillInstaller::new(store.clone());

        // 설치
        let content = create_test_skill_content("to-remove");
        installer
            .install_from_raw("to-remove", &content)
            .await
            .unwrap();
        assert!(store.contains("to-remove").await);

        // 제거
        installer.uninstall("to-remove").await.unwrap();
        assert!(!store.contains("to-remove").await);

        // 백업은 남아있음
        let backups = installer.list_backups("to-remove").await.unwrap();
        assert_eq!(backups.len(), 1);
    }
}
