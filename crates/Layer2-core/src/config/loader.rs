//! Configuration Loader
//!
//! ForgeCode 전용 설정 로더 (`.forgecode` 폴더만 지원)
//!
//! ## 검색 우선순위
//!
//! 1. User-level: `~/.forgecode/settings.json`
//! 2. Project-level: `.forgecode/settings.json`
//! 3. Local (gitignored): `.forgecode/settings.local.json`
//!
//! 각 레벨의 설정이 이전 레벨을 오버라이드합니다.

use super::types::ForgeConfig;
use forge_foundation::Result;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// 설정 폴더 이름 (통일)
pub const CONFIG_DIR_NAME: &str = ".forgecode";

// ============================================================================
// ConfigLoader - 설정 로더
// ============================================================================

/// 설정 로더
pub struct ConfigLoader {
    /// 검색 경로
    search_paths: Vec<ConfigPath>,
}

/// 설정 파일 경로 정보
#[derive(Debug, Clone)]
struct ConfigPath {
    /// 경로
    path: PathBuf,
    /// 우선순위 (높을수록 우선)
    priority: u8,
    /// 설명
    description: &'static str,
}

impl ConfigLoader {
    /// 새 로더 생성 (기본 검색 경로)
    pub fn new(working_dir: &Path) -> Self {
        let mut paths = Vec::new();

        // 1. User-level (가장 낮은 우선순위)
        if let Some(home) = dirs::home_dir() {
            // ForgeCode 전용
            paths.push(ConfigPath {
                path: home.join(CONFIG_DIR_NAME).join("settings.json"),
                priority: 10,
                description: "User settings",
            });
        }

        // 2. Project-level
        paths.push(ConfigPath {
            path: working_dir.join(CONFIG_DIR_NAME).join("settings.json"),
            priority: 20,
            description: "Project settings",
        });

        // 3. Local (gitignored, 가장 높은 우선순위)
        paths.push(ConfigPath {
            path: working_dir.join(CONFIG_DIR_NAME).join("settings.local.json"),
            priority: 30,
            description: "Local settings",
        });

        // 우선순위 순으로 정렬
        paths.sort_by_key(|p| p.priority);

        Self { search_paths: paths }
    }

    /// 커스텀 검색 경로로 생성
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        let search_paths = paths
            .into_iter()
            .enumerate()
            .map(|(i, path)| ConfigPath {
                path,
                priority: i as u8,
                description: "Custom",
            })
            .collect();

        Self { search_paths }
    }

    /// 검색 경로 추가
    pub fn add_path(&mut self, path: PathBuf, priority: u8) {
        self.search_paths.push(ConfigPath {
            path,
            priority,
            description: "Added",
        });
        self.search_paths.sort_by_key(|p| p.priority);
    }

    /// 모든 경로에서 설정 로드하여 병합
    pub fn load_all(&self) -> Result<ForgeConfig> {
        let mut merged = ForgeConfig::new();

        for config_path in &self.search_paths {
            if config_path.path.exists() {
                match load_config_from_file(&config_path.path) {
                    Ok(config) => {
                        info!(
                            "Loaded {} from: {}",
                            config_path.description,
                            config_path.path.display()
                        );
                        merged = merge_configs(merged, config);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to load settings from {}: {}",
                            config_path.path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(merged)
    }

    /// 특정 경로에서만 로드
    pub fn load_from(&self, path: &Path) -> Result<ForgeConfig> {
        if path.exists() {
            load_config_from_file(path)
        } else {
            Ok(ForgeConfig::new())
        }
    }

    /// 존재하는 설정 파일 목록
    pub fn existing_files(&self) -> Vec<PathBuf> {
        self.search_paths
            .iter()
            .filter(|p| p.path.exists())
            .map(|p| p.path.clone())
            .collect()
    }
}

// ============================================================================
// 유틸리티 함수
// ============================================================================

/// 파일에서 설정 로드
pub fn load_config_from_file(path: &Path) -> Result<ForgeConfig> {
    let content = std::fs::read_to_string(path)?;

    // JSON5 또는 JSONC 파일일 수 있음 (주석 제거)
    let content = strip_json_comments(&content);

    let config: ForgeConfig = serde_json::from_str(&content).map_err(|e| {
        forge_foundation::Error::InvalidInput(format!(
            "Invalid settings.json at {}: {}",
            path.display(),
            e
        ))
    })?;

    debug!(
        "Loaded config from {}: {} MCP servers, model: {:?}",
        path.display(),
        config.mcp_servers.len(),
        config.model
    );

    Ok(config)
}

/// 두 설정 병합 (later가 earlier를 오버라이드)
pub fn merge_configs(earlier: ForgeConfig, later: ForgeConfig) -> ForgeConfig {
    ForgeConfig {
        // API 키: later 우선
        api_key: later.api_key.or(earlier.api_key),

        // Providers: 병합
        providers: {
            let mut merged = earlier.providers;
            merged.extend(later.providers);
            merged
        },

        // 모델: later 우선
        model: later.model.or(earlier.model),

        // Models: 병합
        models: {
            let mut merged = earlier.models;
            merged.extend(later.models);
            merged
        },

        // MCP 서버: 병합
        mcp_servers: {
            let mut merged = earlier.mcp_servers;
            merged.extend(later.mcp_servers);
            merged
        },

        // 권한: 병합 (패턴들 추가)
        permissions: merge_permissions(earlier.permissions, later.permissions),

        // Shell: later 우선 (부분 오버라이드)
        shell: merge_shell(earlier.shell, later.shell),

        // 테마: later 우선
        theme: later.theme,

        // 일반 설정: later 우선
        auto_context: later.auto_context,
        streaming: later.streaming,
        save_history: later.save_history,
        max_context_tokens: later.max_context_tokens.or(earlier.max_context_tokens),
        max_response_tokens: later.max_response_tokens.or(earlier.max_response_tokens),

        // Git 설정: later 우선
        git: later.git,

        // Security 설정: later 우선
        security: later.security,

        // Extra: 병합
        extra: {
            let mut merged = earlier.extra;
            merged.extend(later.extra);
            merged
        },
    }
}

/// 권한 설정 병합
fn merge_permissions(
    earlier: super::types::PermissionConfig,
    later: super::types::PermissionConfig,
) -> super::types::PermissionConfig {
    super::types::PermissionConfig {
        auto_approve: {
            let mut merged = earlier.auto_approve;
            merged.extend(later.auto_approve);
            merged
        },
        always_deny: {
            let mut merged = earlier.always_deny;
            merged.extend(later.always_deny);
            merged
        },
        allowed_directories: {
            let mut merged = earlier.allowed_directories;
            merged.extend(later.allowed_directories);
            merged
        },
        denied_directories: {
            let mut merged = earlier.denied_directories;
            merged.extend(later.denied_directories);
            merged
        },
        confirm_dangerous: later.confirm_dangerous,
    }
}

/// Shell 설정 병합
fn merge_shell(
    earlier: super::types::ShellConfigSection,
    later: super::types::ShellConfigSection,
) -> super::types::ShellConfigSection {
    super::types::ShellConfigSection {
        shell: later.shell.or(earlier.shell),
        shell_args: if later.shell_args.is_empty() {
            earlier.shell_args
        } else {
            later.shell_args
        },
        timeout: later.timeout,
        working_directory: later.working_directory.or(earlier.working_directory),
        env: {
            let mut merged = earlier.env;
            merged.extend(later.env);
            merged
        },
    }
}

/// JSON 주석 제거 (// 및 /* */)
pub fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(c) = chars.next() {
        if escape_next {
            output.push(c);
            escape_next = false;
            continue;
        }

        if c == '\\' && in_string {
            output.push(c);
            escape_next = true;
            continue;
        }

        if c == '"' && !escape_next {
            in_string = !in_string;
            output.push(c);
            continue;
        }

        if !in_string && c == '/' {
            if let Some(&next) = chars.peek() {
                if next == '/' {
                    // 라인 주석 스킵
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\n' {
                            output.push(c);
                            break;
                        }
                    }
                    continue;
                } else if next == '*' {
                    // 블록 주석 스킵
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '*' {
                            if let Some(&'/') = chars.peek() {
                                chars.next();
                                break;
                            }
                        }
                    }
                    continue;
                }
            }
        }

        output.push(c);
    }

    output
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_config_loader_new() {
        let loader = ConfigLoader::new(Path::new("."));
        assert!(!loader.search_paths.is_empty());
    }

    #[test]
    fn test_load_config_from_file() {
        let dir = tempdir().unwrap();
        let config_file = dir.path().join("settings.json");

        let content = r#"{
            "apiKey": "test-key",
            "model": "claude-opus-4-5-20251101",
            "streaming": false
        }"#;

        fs::write(&config_file, content).unwrap();

        let config = load_config_from_file(&config_file).unwrap();
        assert_eq!(config.api_key, Some("test-key".to_string()));
        assert_eq!(config.model, Some("claude-opus-4-5-20251101".to_string()));
        assert!(!config.streaming);
    }

    #[test]
    fn test_merge_configs() {
        let earlier = ForgeConfig {
            api_key: Some("earlier-key".to_string()),
            model: Some("earlier-model".to_string()),
            ..Default::default()
        };

        let later = ForgeConfig {
            model: Some("later-model".to_string()),
            streaming: false,
            ..Default::default()
        };

        let merged = merge_configs(earlier, later);

        // later의 api_key가 None이므로 earlier 유지
        assert_eq!(merged.api_key, Some("earlier-key".to_string()));
        // later의 model이 우선
        assert_eq!(merged.model, Some("later-model".to_string()));
        // later의 streaming 적용
        assert!(!merged.streaming);
    }

    #[test]
    fn test_strip_json_comments() {
        let input = r#"{
            // This is a comment
            "key": "value", /* inline comment */
            "other": 123
        }"#;

        let output = strip_json_comments(input);
        assert!(!output.contains("comment"));
        assert!(output.contains("\"key\""));
    }

    #[test]
    fn test_loader_load_all() {
        let dir = tempdir().unwrap();
        let forgecode_dir = dir.path().join(CONFIG_DIR_NAME);
        fs::create_dir_all(&forgecode_dir).unwrap();

        // 기본 설정
        let settings_file = forgecode_dir.join("settings.json");
        fs::write(&settings_file, r#"{"model": "base-model"}"#).unwrap();

        // 로컬 오버라이드
        let local_file = forgecode_dir.join("settings.local.json");
        fs::write(&local_file, r#"{"model": "local-model", "streaming": false}"#).unwrap();

        let loader = ConfigLoader::new(dir.path());
        let config = loader.load_all().unwrap();

        // 로컬 설정이 우선
        assert_eq!(config.model, Some("local-model".to_string()));
        assert!(!config.streaming);
    }

    #[test]
    fn test_existing_files() {
        let dir = tempdir().unwrap();
        let forgecode_dir = dir.path().join(CONFIG_DIR_NAME);
        fs::create_dir_all(&forgecode_dir).unwrap();

        let settings_file = forgecode_dir.join("settings.json");
        fs::write(&settings_file, "{}").unwrap();

        let loader = ConfigLoader::new(dir.path());
        let files = loader.existing_files();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], settings_file);
    }

    #[test]
    fn test_mcp_servers_merge() {
        let earlier = ForgeConfig {
            mcp_servers: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "server1".to_string(),
                    super::super::types::McpServerConfig {
                        command: Some("cmd1".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };

        let later = ForgeConfig {
            mcp_servers: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "server2".to_string(),
                    super::super::types::McpServerConfig {
                        command: Some("cmd2".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };

        let merged = merge_configs(earlier, later);
        assert!(merged.mcp_servers.contains_key("server1"));
        assert!(merged.mcp_servers.contains_key("server2"));
    }
}
