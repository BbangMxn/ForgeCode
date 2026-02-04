//! Plugin Installer - 플러그인 다운로드 및 설치
//!
//! GitHub 등 소스에서 플러그인을 다운로드하고 설치합니다.

use super::store::{InstalledPlugin, PluginStore};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, info};

use forge_foundation::Result;

// ============================================================================
// PluginSource - 플러그인 소스
// ============================================================================

/// 플러그인 설치 소스
#[derive(Debug, Clone)]
pub enum PluginSource {
    /// GitHub 저장소 (owner/repo 또는 owner/repo@tag)
    GitHub {
        owner: String,
        repo: String,
        tag: Option<String>,
        path: Option<String>,
    },
    /// 로컬 경로
    Local(PathBuf),
    /// HTTP URL
    Url(String),
}

impl PluginSource {
    /// GitHub URL 파싱 (예: "github:owner/repo@tag", "github:owner/repo")
    pub fn parse_github(source: &str) -> Option<Self> {
        let source = source.strip_prefix("github:")?;

        // @로 tag 분리
        let (repo_path, tag) = if let Some(idx) = source.find('@') {
            let (path, tag) = source.split_at(idx);
            (path, Some(tag[1..].to_string()))
        } else {
            (source, None)
        };

        // owner/repo/path 분리
        let parts: Vec<&str> = repo_path.split('/').collect();
        if parts.len() < 2 {
            return None;
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let path = if parts.len() > 2 {
            Some(parts[2..].join("/"))
        } else {
            None
        };

        Some(PluginSource::GitHub {
            owner,
            repo,
            tag,
            path,
        })
    }

    /// 소스 문자열에서 파싱
    pub fn parse(source: &str) -> Option<Self> {
        if source.starts_with("github:") {
            Self::parse_github(source)
        } else if source.starts_with("http://") || source.starts_with("https://") {
            Some(PluginSource::Url(source.to_string()))
        } else {
            // 로컬 경로로 간주
            let path = PathBuf::from(source);
            if path.exists() {
                Some(PluginSource::Local(path))
            } else {
                None
            }
        }
    }

    /// 소스 문자열로 변환
    pub fn to_string(&self) -> String {
        match self {
            PluginSource::GitHub {
                owner,
                repo,
                tag,
                path,
            } => {
                let mut s = format!("github:{}/{}", owner, repo);
                if let Some(p) = path {
                    s.push('/');
                    s.push_str(p);
                }
                if let Some(t) = tag {
                    s.push('@');
                    s.push_str(t);
                }
                s
            }
            PluginSource::Local(path) => path.to_string_lossy().to_string(),
            PluginSource::Url(url) => url.clone(),
        }
    }
}

// ============================================================================
// PluginInstaller - 플러그인 설치기
// ============================================================================

/// 플러그인 설치기
pub struct PluginInstaller {
    /// 플러그인 저장소
    store: Arc<PluginStore>,

    /// HTTP 클라이언트
    client: Client,
}

impl PluginInstaller {
    /// 새 설치기 생성
    pub fn new(store: Arc<PluginStore>) -> Self {
        Self {
            store,
            client: Client::new(),
        }
    }

    // ========================================================================
    // 설치
    // ========================================================================

    /// 소스에서 플러그인 설치
    pub async fn install(&self, source: &str) -> Result<InstalledPlugin> {
        let plugin_source = PluginSource::parse(source).ok_or_else(|| {
            forge_foundation::Error::InvalidInput(format!("Invalid plugin source: {}", source))
        })?;

        match plugin_source {
            PluginSource::GitHub {
                owner,
                repo,
                tag,
                path,
            } => {
                self.install_from_github(&owner, &repo, tag.as_deref(), path.as_deref())
                    .await
            }
            PluginSource::Local(path) => self.install_from_path(&path).await,
            PluginSource::Url(url) => self.install_from_url(&url).await,
        }
    }

    /// GitHub에서 플러그인 설치
    pub async fn install_from_github(
        &self,
        owner: &str,
        repo: &str,
        tag: Option<&str>,
        subpath: Option<&str>,
    ) -> Result<InstalledPlugin> {
        let tag = tag.unwrap_or("main");
        info!("Installing plugin from github:{}/{}@{}", owner, repo, tag);

        // GitHub API로 tarball URL 생성
        let archive_url = format!(
            "https://github.com/{}/{}/archive/refs/{}.tar.gz",
            owner,
            repo,
            if tag == "main" || tag == "master" {
                format!("heads/{}", tag)
            } else {
                format!("tags/{}", tag)
            }
        );

        // 임시 디렉토리에 다운로드
        let temp_dir = std::env::temp_dir().join(format!("forge_plugin_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).await?;

        // 다운로드
        let archive_path = temp_dir.join("archive.tar.gz");
        self.download_file(&archive_url, &archive_path).await?;

        // 압축 해제
        self.extract_tarball(&archive_path, &temp_dir).await?;

        // 실제 플러그인 디렉토리 찾기 (archive 내부의 repo-tag 디렉토리)
        let extracted_dir = self.find_extracted_dir(&temp_dir).await?;

        // subpath가 있으면 그 안의 plugin.json을 찾음
        let source_dir = if let Some(subpath) = subpath {
            extracted_dir.join(subpath)
        } else {
            extracted_dir
        };

        // plugin.json 파싱
        let manifest_path = source_dir.join("plugin.json");
        if !manifest_path.exists() {
            return Err(forge_foundation::Error::NotFound(
                "plugin.json not found in repository".into(),
            ));
        }

        let manifest_content = fs::read_to_string(&manifest_path).await?;
        let manifest: serde_json::Value = serde_json::from_str(&manifest_content)?;

        let plugin_id = manifest["id"]
            .as_str()
            .ok_or_else(|| {
                forge_foundation::Error::InvalidInput("plugin.json missing 'id' field".into())
            })?
            .to_string();

        let plugin_version = manifest["version"].as_str().unwrap_or("1.0.0").to_string();
        let plugin_name = manifest["name"].as_str().unwrap_or(&plugin_id).to_string();
        let description = manifest["description"].as_str().map(String::from);
        let author = manifest["author"].as_str().map(String::from);

        // 플러그인 디렉토리로 복사
        let target_dir = self.store.create_plugin_dir(&plugin_id).await?;
        self.copy_dir_recursive(&source_dir, &target_dir).await?;

        // 임시 디렉토리 정리
        let _ = fs::remove_dir_all(&temp_dir).await;

        // 설치 기록
        let source_str = format!("github:{}/{}@{}", owner, repo, tag);
        let mut installed =
            InstalledPlugin::new(&plugin_id, &plugin_version, &source_str, &target_dir)
                .with_name(&plugin_name);

        if let Some(desc) = description {
            installed = installed.with_description(desc);
        }
        if let Some(auth) = author {
            installed = installed.with_author(auth);
        }

        self.store.record_install(installed.clone()).await?;

        info!("Installed plugin {} v{}", plugin_id, plugin_version);
        Ok(installed)
    }

    /// 로컬 경로에서 플러그인 설치
    pub async fn install_from_path(&self, source_path: &Path) -> Result<InstalledPlugin> {
        info!("Installing plugin from {:?}", source_path);

        // plugin.json 확인
        let manifest_path = source_path.join("plugin.json");
        if !manifest_path.exists() {
            return Err(forge_foundation::Error::NotFound(
                "plugin.json not found".into(),
            ));
        }

        let manifest_content = fs::read_to_string(&manifest_path).await?;
        let manifest: serde_json::Value = serde_json::from_str(&manifest_content)?;

        let plugin_id = manifest["id"]
            .as_str()
            .ok_or_else(|| {
                forge_foundation::Error::InvalidInput("plugin.json missing 'id' field".into())
            })?
            .to_string();

        let plugin_version = manifest["version"].as_str().unwrap_or("1.0.0").to_string();
        let plugin_name = manifest["name"].as_str().unwrap_or(&plugin_id).to_string();
        let description = manifest["description"].as_str().map(String::from);
        let author = manifest["author"].as_str().map(String::from);

        // 플러그인 디렉토리로 복사
        let target_dir = self.store.create_plugin_dir(&plugin_id).await?;
        self.copy_dir_recursive(source_path, &target_dir).await?;

        // 설치 기록
        let source_str = source_path.to_string_lossy().to_string();
        let mut installed =
            InstalledPlugin::new(&plugin_id, &plugin_version, &source_str, &target_dir)
                .with_name(&plugin_name);

        if let Some(desc) = description {
            installed = installed.with_description(desc);
        }
        if let Some(auth) = author {
            installed = installed.with_author(auth);
        }

        self.store.record_install(installed.clone()).await?;

        info!("Installed plugin {} v{}", plugin_id, plugin_version);
        Ok(installed)
    }

    /// URL에서 플러그인 설치 (tarball 가정)
    pub async fn install_from_url(&self, url: &str) -> Result<InstalledPlugin> {
        info!("Installing plugin from {}", url);

        // 임시 디렉토리에 다운로드
        let temp_dir = std::env::temp_dir().join(format!("forge_plugin_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).await?;

        let archive_path = temp_dir.join("archive.tar.gz");
        self.download_file(url, &archive_path).await?;

        // 압축 해제
        self.extract_tarball(&archive_path, &temp_dir).await?;

        // 압축 해제된 디렉토리 찾기
        let extracted_dir = self.find_extracted_dir(&temp_dir).await?;

        // 로컬 설치 진행
        let result = self.install_from_path(&extracted_dir).await;

        // 임시 디렉토리 정리
        let _ = fs::remove_dir_all(&temp_dir).await;

        result
    }

    // ========================================================================
    // 제거
    // ========================================================================

    /// 플러그인 제거
    pub async fn uninstall(&self, id: &str) -> Result<()> {
        info!("Uninstalling plugin: {}", id);

        // 디렉토리 삭제
        self.store.remove_plugin_dir(id).await?;

        // 기록에서 제거
        self.store.record_uninstall(id).await?;

        info!("Uninstalled plugin: {}", id);
        Ok(())
    }

    // ========================================================================
    // 업데이트 체크
    // ========================================================================

    /// 플러그인 업데이트 확인
    pub async fn check_update(&self, id: &str) -> Result<Option<String>> {
        let plugin = self.store.get(id).await.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("Plugin {} not installed", id))
        })?;

        // GitHub 소스인 경우만 체크 가능
        if let Some(source) = PluginSource::parse(&plugin.source) {
            if let PluginSource::GitHub { owner, repo, .. } = source {
                // 최신 태그 조회
                let url = format!("https://api.github.com/repos/{}/{}/tags", owner, repo);
                let response = self
                    .client
                    .get(&url)
                    .header("User-Agent", "ForgeCode")
                    .send()
                    .await;

                if let Ok(resp) = response {
                    if let Ok(tags) = resp.json::<Vec<serde_json::Value>>().await {
                        if let Some(latest) = tags.first() {
                            if let Some(tag_name) = latest["name"].as_str() {
                                if tag_name != plugin.version {
                                    return Ok(Some(tag_name.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    // ========================================================================
    // 유틸리티
    // ========================================================================

    /// 파일 다운로드
    async fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        debug!("Downloading {} to {:?}", url, dest);

        let response = self
            .client
            .get(url)
            .header("User-Agent", "ForgeCode")
            .send()
            .await
            .map_err(|e| forge_foundation::Error::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(forge_foundation::Error::Http(format!(
                "Failed to download: HTTP {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| forge_foundation::Error::Http(e.to_string()))?;

        fs::write(dest, bytes).await?;
        Ok(())
    }

    /// tarball 압축 해제
    async fn extract_tarball(&self, archive: &Path, dest: &Path) -> Result<()> {
        debug!("Extracting {:?} to {:?}", archive, dest);

        // tar -xzf 명령 사용 (크로스 플랫폼을 위해 OS별 처리 필요)
        #[cfg(windows)]
        {
            // Windows: tar 명령 사용 (Windows 10 1803+)
            let output = tokio::process::Command::new("tar")
                .args([
                    "-xzf",
                    &archive.to_string_lossy(),
                    "-C",
                    &dest.to_string_lossy(),
                ])
                .output()
                .await?;

            if !output.status.success() {
                return Err(forge_foundation::Error::Internal(format!(
                    "Failed to extract archive: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }

        #[cfg(not(windows))]
        {
            let output = tokio::process::Command::new("tar")
                .args([
                    "-xzf",
                    &archive.to_string_lossy(),
                    "-C",
                    &dest.to_string_lossy(),
                ])
                .output()
                .await?;

            if !output.status.success() {
                return Err(forge_foundation::Error::Internal(format!(
                    "Failed to extract archive: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }

        Ok(())
    }

    /// 압축 해제 후 실제 디렉토리 찾기
    async fn find_extracted_dir(&self, temp_dir: &Path) -> Result<PathBuf> {
        let mut entries = fs::read_dir(temp_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() && !path.file_name().map_or(false, |n| n == "archive.tar.gz") {
                return Ok(path);
            }
        }

        Err(forge_foundation::Error::NotFound(
            "No directory found after extraction".into(),
        ))
    }

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

    #[test]
    fn test_parse_github_source() {
        let source = PluginSource::parse_github("github:owner/repo").unwrap();
        if let PluginSource::GitHub {
            owner,
            repo,
            tag,
            path,
        } = source
        {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert!(tag.is_none());
            assert!(path.is_none());
        } else {
            panic!("Expected GitHub source, got {:?}", source);
        }

        let source = PluginSource::parse_github("github:owner/repo@v1.0.0").unwrap();
        if let PluginSource::GitHub {
            owner, repo, tag, ..
        } = source
        {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert_eq!(tag, Some("v1.0.0".to_string()));
        }

        let source =
            PluginSource::parse_github("github:owner/repo/plugins/myplugin@v1.0.0").unwrap();
        if let PluginSource::GitHub { path, .. } = source {
            assert_eq!(path, Some("plugins/myplugin".to_string()));
        }
    }

    #[test]
    fn test_parse_source() {
        // GitHub
        let source = PluginSource::parse("github:owner/repo");
        assert!(matches!(source, Some(PluginSource::GitHub { .. })));

        // URL
        let source = PluginSource::parse("https://example.com/plugin.tar.gz");
        assert!(matches!(source, Some(PluginSource::Url(_))));
    }

    #[test]
    fn test_source_to_string() {
        let source = PluginSource::GitHub {
            owner: "owner".into(),
            repo: "repo".into(),
            tag: Some("v1.0.0".into()),
            path: None,
        };
        assert_eq!(source.to_string(), "github:owner/repo@v1.0.0");
    }

    #[tokio::test]
    async fn test_install_from_path() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(PluginStore::new(temp.path().join("plugins")));

        // 테스트 플러그인 생성
        let plugin_dir = temp.path().join("test-plugin");
        fs::create_dir_all(&plugin_dir).await.unwrap();
        fs::write(
            plugin_dir.join("plugin.json"),
            r#"{"id": "test.plugin", "name": "Test Plugin", "version": "1.0.0"}"#,
        )
        .await
        .unwrap();

        let installer = PluginInstaller::new(store.clone());
        let result = installer.install_from_path(&plugin_dir).await;

        assert!(result.is_ok());
        let installed = result.unwrap();
        assert_eq!(installed.id, "test.plugin");
        assert!(store.contains("test.plugin").await);
    }

    #[tokio::test]
    async fn test_uninstall() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(PluginStore::new(temp.path().join("plugins")));

        // 테스트 플러그인 생성 및 설치
        let plugin_dir = temp.path().join("test-plugin");
        fs::create_dir_all(&plugin_dir).await.unwrap();
        fs::write(
            plugin_dir.join("plugin.json"),
            r#"{"id": "test.plugin", "name": "Test Plugin", "version": "1.0.0"}"#,
        )
        .await
        .unwrap();

        let installer = PluginInstaller::new(store.clone());
        installer.install_from_path(&plugin_dir).await.unwrap();

        // 제거
        let result = installer.uninstall("test.plugin").await;
        assert!(result.is_ok());
        assert!(!store.contains("test.plugin").await);
    }
}
