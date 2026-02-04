//! Skill Marketplace - 커뮤니티 스킬 레지스트리
//!
//! GitHub 기반 Skills 마켓플레이스에서 스킬을 검색하고 설치합니다.
//! Claude Code 커뮤니티 Skills와 호환됩니다.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};

use forge_foundation::Result;

// ============================================================================
// MarketplaceSkill - 마켓플레이스 스킬 정보
// ============================================================================

/// 마켓플레이스에 등록된 스킬 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSkill {
    /// 스킬 이름 (슬래시 명령어)
    pub name: String,

    /// 표시 이름
    #[serde(default)]
    pub display_name: String,

    /// 설명
    pub description: String,

    /// 카테고리
    #[serde(default)]
    pub category: String,

    /// 작성자
    pub author: String,

    /// 버전
    #[serde(default = "default_version")]
    pub version: String,

    /// GitHub 소스 (github:user/repo/path@tag)
    pub source: String,

    /// 태그
    #[serde(default)]
    pub tags: Vec<String>,

    /// 다운로드 수
    #[serde(default)]
    pub downloads: u64,

    /// 별점 (1-5)
    #[serde(default)]
    pub rating: f32,

    /// 추천 스킬 여부
    #[serde(default)]
    pub featured: bool,

    /// 최종 업데이트 시간
    #[serde(default)]
    pub updated_at: String,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl MarketplaceSkill {
    /// 새 스킬 생성
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            display_name: name.clone(),
            name,
            description: String::new(),
            category: "general".to_string(),
            author: String::new(),
            version: default_version(),
            source: source.into(),
            tags: Vec::new(),
            downloads: 0,
            rating: 0.0,
            featured: false,
            updated_at: String::new(),
        }
    }
}

// ============================================================================
// MarketplaceRegistry - 마켓플레이스 레지스트리
// ============================================================================

/// 마켓플레이스 레지스트리 파일 구조
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceRegistry {
    /// 레지스트리 버전
    #[serde(default = "default_registry_version")]
    pub version: String,

    /// 레지스트리 이름
    #[serde(default)]
    pub name: String,

    /// 레지스트리 URL
    #[serde(default)]
    pub url: String,

    /// 등록된 스킬 목록
    #[serde(default)]
    pub skills: Vec<MarketplaceSkill>,

    /// 카테고리 목록
    #[serde(default)]
    pub categories: Vec<SkillCategory>,
}

fn default_registry_version() -> String {
    "1.0".to_string()
}

impl Default for MarketplaceRegistry {
    fn default() -> Self {
        Self {
            version: default_registry_version(),
            name: "ForgeCode Skills".to_string(),
            url: String::new(),
            skills: Vec::new(),
            categories: default_categories(),
        }
    }
}

/// 스킬 카테고리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCategory {
    pub id: String,
    pub name: String,
    pub description: String,
}

fn default_categories() -> Vec<SkillCategory> {
    vec![
        SkillCategory {
            id: "git".to_string(),
            name: "Git & Version Control".to_string(),
            description: "Git 관련 스킬 (커밋, PR, 브랜치 등)".to_string(),
        },
        SkillCategory {
            id: "code-review".to_string(),
            name: "Code Review".to_string(),
            description: "코드 리뷰 및 분석".to_string(),
        },
        SkillCategory {
            id: "refactor".to_string(),
            name: "Refactoring".to_string(),
            description: "코드 리팩토링".to_string(),
        },
        SkillCategory {
            id: "testing".to_string(),
            name: "Testing".to_string(),
            description: "테스트 관련".to_string(),
        },
        SkillCategory {
            id: "docs".to_string(),
            name: "Documentation".to_string(),
            description: "문서화".to_string(),
        },
        SkillCategory {
            id: "general".to_string(),
            name: "General".to_string(),
            description: "일반 스킬".to_string(),
        },
    ]
}

// ============================================================================
// SkillMarketplace - 마켓플레이스 클라이언트
// ============================================================================

/// 스킬 마켓플레이스 클라이언트
pub struct SkillMarketplace {
    /// HTTP 클라이언트
    client: Client,

    /// 레지스트리 URL 목록
    registries: Vec<String>,

    /// 캐시 디렉토리
    cache_dir: PathBuf,

    /// 캐시된 레지스트리
    cache: tokio::sync::RwLock<HashMap<String, MarketplaceRegistry>>,
}

impl SkillMarketplace {
    /// 새 마켓플레이스 클라이언트 생성
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            client: Client::new(),
            registries: default_registries(),
            cache_dir: cache_dir.into(),
            cache: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 사용자 홈 디렉토리 기반으로 생성
    pub fn user_marketplace() -> Option<Self> {
        dirs::home_dir().map(|home| {
            Self::new(home.join(".forgecode").join("marketplace"))
        })
    }

    /// 레지스트리 추가
    pub fn add_registry(&mut self, url: impl Into<String>) {
        self.registries.push(url.into());
    }

    // ========================================================================
    // 레지스트리 로드
    // ========================================================================

    /// 모든 레지스트리 새로고침
    pub async fn refresh(&self) -> Result<()> {
        info!("Refreshing skill marketplace registries...");

        for url in &self.registries {
            match self.fetch_registry(url).await {
                Ok(registry) => {
                    info!("Loaded {} skills from {}", registry.skills.len(), url);
                    let mut cache = self.cache.write().await;
                    cache.insert(url.clone(), registry);
                }
                Err(e) => {
                    warn!("Failed to fetch registry {}: {}", url, e);
                }
            }
        }

        // 캐시 저장
        self.save_cache().await?;

        Ok(())
    }

    /// 레지스트리 가져오기
    async fn fetch_registry(&self, url: &str) -> Result<MarketplaceRegistry> {
        debug!("Fetching registry from {}", url);

        let response = self.client
            .get(url)
            .header("User-Agent", "ForgeCode")
            .send()
            .await
            .map_err(|e| forge_foundation::Error::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(forge_foundation::Error::Http(format!(
                "Failed to fetch registry: HTTP {}",
                response.status()
            )));
        }

        let registry: MarketplaceRegistry = response.json().await
            .map_err(|e| forge_foundation::Error::Http(e.to_string()))?;

        Ok(registry)
    }

    /// 캐시 로드
    pub async fn load_cache(&self) -> Result<()> {
        let cache_file = self.cache_dir.join("registries.json");

        if !cache_file.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&cache_file).await?;
        let registries: HashMap<String, MarketplaceRegistry> = serde_json::from_str(&content)?;

        *self.cache.write().await = registries;

        debug!("Loaded marketplace cache");
        Ok(())
    }

    /// 캐시 저장
    async fn save_cache(&self) -> Result<()> {
        if !self.cache_dir.exists() {
            fs::create_dir_all(&self.cache_dir).await?;
        }

        let cache_file = self.cache_dir.join("registries.json");
        let cache = self.cache.read().await;
        let content = serde_json::to_string_pretty(&*cache)?;

        fs::write(&cache_file, content).await?;

        debug!("Saved marketplace cache");
        Ok(())
    }

    // ========================================================================
    // 검색
    // ========================================================================

    /// 모든 스킬 목록
    pub async fn list_all(&self) -> Vec<MarketplaceSkill> {
        let cache = self.cache.read().await;
        cache.values()
            .flat_map(|r| r.skills.clone())
            .collect()
    }

    /// 카테고리별 스킬 목록
    pub async fn list_by_category(&self, category: &str) -> Vec<MarketplaceSkill> {
        self.list_all().await
            .into_iter()
            .filter(|s| s.category == category)
            .collect()
    }

    /// 추천 스킬 목록
    pub async fn list_featured(&self) -> Vec<MarketplaceSkill> {
        self.list_all().await
            .into_iter()
            .filter(|s| s.featured)
            .collect()
    }

    /// 스킬 검색
    pub async fn search(&self, query: &str) -> Vec<MarketplaceSkill> {
        let query = query.to_lowercase();

        self.list_all().await
            .into_iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query)
                    || s.display_name.to_lowercase().contains(&query)
                    || s.description.to_lowercase().contains(&query)
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&query))
            })
            .collect()
    }

    /// 이름으로 스킬 찾기
    pub async fn find(&self, name: &str) -> Option<MarketplaceSkill> {
        let clean_name = name.trim_start_matches('/');

        self.list_all().await
            .into_iter()
            .find(|s| s.name.trim_start_matches('/') == clean_name)
    }

    // ========================================================================
    // 카테고리
    // ========================================================================

    /// 모든 카테고리 목록
    pub async fn categories(&self) -> Vec<SkillCategory> {
        let cache = self.cache.read().await;

        // 모든 레지스트리에서 카테고리 수집 (중복 제거)
        let mut seen = std::collections::HashSet::new();
        cache.values()
            .flat_map(|r| r.categories.clone())
            .filter(|c| seen.insert(c.id.clone()))
            .collect()
    }

    // ========================================================================
    // 통계
    // ========================================================================

    /// 마켓플레이스 통계
    pub async fn stats(&self) -> MarketplaceStats {
        let skills = self.list_all().await;

        MarketplaceStats {
            total_skills: skills.len(),
            featured_count: skills.iter().filter(|s| s.featured).count(),
            total_downloads: skills.iter().map(|s| s.downloads).sum(),
            categories: self.categories().await.len(),
            registries: self.registries.len(),
        }
    }
}

/// 마켓플레이스 통계
#[derive(Debug, Clone)]
pub struct MarketplaceStats {
    pub total_skills: usize,
    pub featured_count: usize,
    pub total_downloads: u64,
    pub categories: usize,
    pub registries: usize,
}

// ============================================================================
// 기본 레지스트리
// ============================================================================

fn default_registries() -> Vec<String> {
    vec![
        // ForgeCode 공식 레지스트리
        "https://raw.githubusercontent.com/anthropics/claude-code/main/skills/registry.json".to_string(),
        // 커뮤니티 레지스트리 (예시)
        // "https://raw.githubusercontent.com/community/awesome-claude-skills/main/registry.json".to_string(),
    ]
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registry() -> MarketplaceRegistry {
        MarketplaceRegistry {
            version: "1.0".to_string(),
            name: "Test Registry".to_string(),
            url: "https://example.com/registry.json".to_string(),
            skills: vec![
                MarketplaceSkill {
                    name: "commit".to_string(),
                    display_name: "Smart Commit".to_string(),
                    description: "Intelligent git commit with auto-generated messages".to_string(),
                    category: "git".to_string(),
                    author: "forge".to_string(),
                    version: "1.0.0".to_string(),
                    source: "github:forge/skills/commit@v1.0.0".to_string(),
                    tags: vec!["git".to_string(), "commit".to_string()],
                    downloads: 1000,
                    rating: 4.5,
                    featured: true,
                    updated_at: "2024-01-01".to_string(),
                },
                MarketplaceSkill {
                    name: "review-pr".to_string(),
                    display_name: "PR Review".to_string(),
                    description: "Automated PR review".to_string(),
                    category: "code-review".to_string(),
                    author: "forge".to_string(),
                    version: "1.0.0".to_string(),
                    source: "github:forge/skills/review-pr@v1.0.0".to_string(),
                    tags: vec!["pr".to_string(), "review".to_string()],
                    downloads: 500,
                    rating: 4.0,
                    featured: false,
                    updated_at: "2024-01-01".to_string(),
                },
            ],
            categories: default_categories(),
        }
    }

    #[tokio::test]
    async fn test_marketplace_search() {
        let temp = TempDir::new().unwrap();
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        // 수동으로 캐시 추가
        {
            let mut cache = marketplace.cache.write().await;
            cache.insert("test".to_string(), create_test_registry());
        }

        // 검색
        let results = marketplace.search("commit").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "commit");

        // git 태그로 검색
        let results = marketplace.search("git").await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_marketplace_list_featured() {
        let temp = TempDir::new().unwrap();
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        {
            let mut cache = marketplace.cache.write().await;
            cache.insert("test".to_string(), create_test_registry());
        }

        let featured = marketplace.list_featured().await;
        assert_eq!(featured.len(), 1);
        assert_eq!(featured[0].name, "commit");
    }

    #[tokio::test]
    async fn test_marketplace_list_by_category() {
        let temp = TempDir::new().unwrap();
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        {
            let mut cache = marketplace.cache.write().await;
            cache.insert("test".to_string(), create_test_registry());
        }

        let git_skills = marketplace.list_by_category("git").await;
        assert_eq!(git_skills.len(), 1);

        let review_skills = marketplace.list_by_category("code-review").await;
        assert_eq!(review_skills.len(), 1);
    }

    #[tokio::test]
    async fn test_marketplace_find() {
        let temp = TempDir::new().unwrap();
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        {
            let mut cache = marketplace.cache.write().await;
            cache.insert("test".to_string(), create_test_registry());
        }

        let skill = marketplace.find("commit").await;
        assert!(skill.is_some());
        assert_eq!(skill.unwrap().source, "github:forge/skills/commit@v1.0.0");

        // 슬래시 포함도 동작
        let skill = marketplace.find("/commit").await;
        assert!(skill.is_some());
    }

    #[tokio::test]
    async fn test_marketplace_stats() {
        let temp = TempDir::new().unwrap();
        let marketplace = SkillMarketplace::new(temp.path().join("marketplace"));

        {
            let mut cache = marketplace.cache.write().await;
            cache.insert("test".to_string(), create_test_registry());
        }

        let stats = marketplace.stats().await;
        assert_eq!(stats.total_skills, 2);
        assert_eq!(stats.featured_count, 1);
        assert_eq!(stats.total_downloads, 1500);
    }

    #[test]
    fn test_registry_serialization() {
        let registry = create_test_registry();
        let json = serde_json::to_string_pretty(&registry).unwrap();

        let parsed: MarketplaceRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.skills.len(), 2);
        assert_eq!(parsed.name, "Test Registry");
    }
}
