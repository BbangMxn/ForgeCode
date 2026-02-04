//! Hook Loader - Hook 설정 로더
//!
//! hooks.json 파일을 로드하고 파싱합니다.
//! Claude Code와 동일한 검색 경로를 지원합니다.

use super::types::HookConfig;
use forge_foundation::Result;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ============================================================================
// HookLoader - Hook 로더
// ============================================================================

/// Hook 설정 로더
pub struct HookLoader {
    /// 검색 경로
    search_paths: Vec<PathBuf>,
}

impl HookLoader {
    /// 새 로더 생성 (기본 검색 경로)
    pub fn new(working_dir: &Path) -> Self {
        let mut paths = Vec::new();

        // 1. User-level
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".claude")); // Claude Code 호환
            paths.push(home.join(".forgecode"));
        }

        // 2. Project-level
        paths.push(working_dir.join(".claude"));
        paths.push(working_dir.join(".forgecode"));

        Self {
            search_paths: paths,
        }
    }

    /// 커스텀 검색 경로로 생성
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            search_paths: paths,
        }
    }

    /// 검색 경로 추가
    pub fn add_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    /// 모든 경로에서 hooks.json 로드하여 병합
    pub fn load_all(&self) -> Result<HookConfig> {
        let mut merged = HookConfig::new();

        for search_path in &self.search_paths {
            let hooks_file = search_path.join("hooks.json");

            if hooks_file.exists() {
                match load_hooks_from_file(&hooks_file) {
                    Ok(config) => {
                        info!("Loaded hooks from: {}", hooks_file.display());
                        merged.merge(config);
                    }
                    Err(e) => {
                        warn!("Failed to load hooks from {}: {}", hooks_file.display(), e);
                    }
                }
            }
        }

        Ok(merged)
    }

    /// 특정 경로에서만 로드
    pub fn load_from(&self, path: &Path) -> Result<HookConfig> {
        let hooks_file = path.join("hooks.json");
        if hooks_file.exists() {
            load_hooks_from_file(&hooks_file)
        } else {
            Ok(HookConfig::new())
        }
    }
}

// ============================================================================
// 유틸리티 함수
// ============================================================================

/// 파일에서 Hook 설정 로드
pub fn load_hooks_from_file(path: &Path) -> Result<HookConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: HookConfig = serde_json::from_str(&content).map_err(|e| {
        forge_foundation::Error::InvalidInput(format!(
            "Invalid hooks.json at {}: {}",
            path.display(),
            e
        ))
    })?;

    debug!(
        "Loaded {} hook matchers from {}",
        config.total_matchers(),
        path.display()
    );

    Ok(config)
}

/// 디렉토리에서 Hook 설정 로드 (hooks.json 찾기)
pub fn load_hooks_from_dir(dir: &Path) -> Result<HookConfig> {
    let hooks_file = dir.join("hooks.json");
    if hooks_file.exists() {
        load_hooks_from_file(&hooks_file)
    } else {
        Ok(HookConfig::new())
    }
}

/// hooks.json 파일 경로 찾기 (우선순위대로)
#[allow(dead_code)]
pub fn find_hooks_files(working_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    // User-level (낮은 우선순위)
    if let Some(home) = dirs::home_dir() {
        let user_claude = home.join(".claude/hooks.json");
        if user_claude.exists() {
            files.push(user_claude);
        }

        let user_forge = home.join(".forgecode/hooks.json");
        if user_forge.exists() {
            files.push(user_forge);
        }
    }

    // Project-level (높은 우선순위)
    let project_claude = working_dir.join(".claude/hooks.json");
    if project_claude.exists() {
        files.push(project_claude);
    }

    let project_forge = working_dir.join(".forgecode/hooks.json");
    if project_forge.exists() {
        files.push(project_forge);
    }

    files
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
    fn test_hook_loader_new() {
        let loader = HookLoader::new(Path::new("."));
        assert!(!loader.search_paths.is_empty());
    }

    #[test]
    fn test_load_hooks_from_file() {
        let dir = tempdir().unwrap();
        let hooks_file = dir.path().join("hooks.json");

        let content = r#"{
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "notify",
                    "message": "Bash tool called"
                }]
            }]
        }"#;

        fs::write(&hooks_file, content).unwrap();

        let config = load_hooks_from_file(&hooks_file).unwrap();
        assert_eq!(config.pre_tool_use.len(), 1);
        assert_eq!(config.pre_tool_use[0].matcher, "Bash");
    }

    #[test]
    fn test_load_hooks_from_dir() {
        let dir = tempdir().unwrap();
        let hooks_file = dir.path().join("hooks.json");

        let content = r#"{"PostToolUse": []}"#;
        fs::write(&hooks_file, content).unwrap();

        let config = load_hooks_from_dir(dir.path()).unwrap();
        assert!(config.is_empty()); // 빈 배열
    }

    #[test]
    fn test_load_nonexistent() {
        let config = load_hooks_from_dir(Path::new("/nonexistent/path")).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn test_loader_load_all() {
        let dir = tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let hooks_file = claude_dir.join("hooks.json");
        let content = r#"{
            "SessionStart": [{
                "matcher": "*",
                "hooks": [{
                    "type": "notify",
                    "message": "Session started"
                }]
            }]
        }"#;
        fs::write(&hooks_file, content).unwrap();

        let loader = HookLoader::new(dir.path());
        let config = loader.load_all().unwrap();
        assert_eq!(config.session_start.len(), 1);
    }

    #[test]
    fn test_complex_hooks_config() {
        let json = r#"{
            "PreToolUse": [
                {
                    "matcher": "Write",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "echo 'Writing file'",
                            "timeout": 5,
                            "blocking": true
                        }
                    ]
                },
                {
                    "matcher": "*",
                    "hooks": [
                        {
                            "type": "notify",
                            "message": "Tool called",
                            "level": "info"
                        }
                    ]
                }
            ],
            "PostToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        {
                            "type": "prompt",
                            "prompt": "Analyze the bash command output"
                        }
                    ]
                }
            ]
        }"#;

        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.pre_tool_use.len(), 2);
        assert_eq!(config.post_tool_use.len(), 1);
        assert_eq!(config.total_matchers(), 3);
    }
}
