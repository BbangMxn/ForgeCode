//! Project Configuration - Claude Code 스타일 프로젝트 설정
//!
//! 디렉토리 구조:
//! ```text
//! project/
//! ├── .forgecode/
//! │   ├── config.json      # 프로젝트별 설정
//! │   ├── commands/        # 커스텀 슬래시 명령어
//! │   └── agents/          # 커스텀 에이전트
//! ├── FORGECODE.md         # 프로젝트 지침 (CLAUDE.md 대응)
//! └── HANDOFF.md           # 세션 간 핸드오프 문서
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Result, Context};

/// 프로젝트 설정
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    /// 프로젝트 이름
    pub name: Option<String>,
    
    /// 기본 모델
    pub model: Option<String>,
    
    /// 기본 프로바이더
    pub provider: Option<String>,
    
    /// 자동 승인 패턴 (파일 경로)
    #[serde(default)]
    pub auto_approve: Vec<String>,
    
    /// 무시할 파일 패턴
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    
    /// 환경 변수
    #[serde(default)]
    pub env: HashMap<String, String>,
    
    /// 커스텀 도구 설정
    #[serde(default)]
    pub tools: ToolConfig,
    
    /// MCP 서버 설정
    #[serde(default)]
    pub mcp: McpConfig,
}

/// 도구 설정
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolConfig {
    /// bash 도구 설정
    pub bash: Option<BashConfig>,
    
    /// 허용된 도구 목록 (비어있으면 모두 허용)
    #[serde(default)]
    pub allowed: Vec<String>,
    
    /// 차단된 도구 목록
    #[serde(default)]
    pub blocked: Vec<String>,
}

/// Bash 도구 설정
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BashConfig {
    /// 기본 쉘 (powershell, cmd, bash, zsh)
    pub shell: Option<String>,
    
    /// 작업 디렉토리
    pub working_dir: Option<String>,
    
    /// 타임아웃 (초)
    pub timeout_secs: Option<u64>,
}

/// MCP 서버 설정
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    /// MCP 서버 목록
    #[serde(default)]
    pub servers: HashMap<String, McpServer>,
}

/// MCP 서버 정의
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// 서버 URL
    pub url: String,
    
    /// 인증 토큰
    pub token: Option<String>,
    
    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

/// 프로젝트 매니저
pub struct ProjectManager {
    /// 프로젝트 루트 경로
    root: PathBuf,
    
    /// 설정
    config: ProjectConfig,
    
    /// FORGECODE.md 내용
    guidelines: Option<String>,
    
    /// HANDOFF.md 내용
    handoff: Option<String>,
}

impl ProjectManager {
    /// 현재 디렉토리에서 프로젝트 감지
    pub fn detect() -> Result<Option<Self>> {
        let cwd = std::env::current_dir()?;
        Self::from_path(&cwd)
    }

    /// 특정 경로에서 프로젝트 로드
    pub fn from_path(path: &Path) -> Result<Option<Self>> {
        // 상위 디렉토리로 올라가며 .forgecode 또는 FORGECODE.md 찾기
        let mut current = path.to_path_buf();
        
        loop {
            let forgecode_dir = current.join(".forgecode");
            let forgecode_md = current.join("FORGECODE.md");
            
            if forgecode_dir.exists() || forgecode_md.exists() {
                return Ok(Some(Self::load(&current)?));
            }
            
            if !current.pop() {
                break;
            }
        }
        
        Ok(None)
    }

    /// 프로젝트 로드
    fn load(root: &Path) -> Result<Self> {
        let config_path = root.join(".forgecode/config.json");
        
        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .context("Failed to read .forgecode/config.json")?;
            serde_json::from_str(&content)
                .context("Failed to parse .forgecode/config.json")?
        } else {
            ProjectConfig::default()
        };

        let guidelines = Self::read_optional(root, "FORGECODE.md")?;
        let handoff = Self::read_optional(root, "HANDOFF.md")?;

        Ok(Self {
            root: root.to_path_buf(),
            config,
            guidelines,
            handoff,
        })
    }

    fn read_optional(root: &Path, filename: &str) -> Result<Option<String>> {
        let path = root.join(filename);
        if path.exists() {
            Ok(Some(fs::read_to_string(&path)?))
        } else {
            Ok(None)
        }
    }

    /// 프로젝트 루트 경로
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 설정 참조
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// FORGECODE.md 내용
    pub fn guidelines(&self) -> Option<&str> {
        self.guidelines.as_deref()
    }

    /// HANDOFF.md 내용
    pub fn handoff(&self) -> Option<&str> {
        self.handoff.as_deref()
    }

    /// 시스템 프롬프트에 추가할 컨텍스트 생성
    pub fn context_for_prompt(&self) -> String {
        let mut context = String::new();

        if let Some(guidelines) = &self.guidelines {
            context.push_str("## Project Guidelines (FORGECODE.md)\n\n");
            context.push_str(guidelines);
            context.push_str("\n\n");
        }

        if let Some(handoff) = &self.handoff {
            context.push_str("## Handoff Notes (HANDOFF.md)\n\n");
            context.push_str(handoff);
            context.push_str("\n\n");
        }

        context
    }

    /// HANDOFF.md 업데이트
    pub fn update_handoff(&mut self, content: &str) -> Result<()> {
        let path = self.root.join("HANDOFF.md");
        fs::write(&path, content)?;
        self.handoff = Some(content.to_string());
        Ok(())
    }

    /// 자동 승인 패턴 확인
    pub fn should_auto_approve(&self, path: &str) -> bool {
        for pattern in &self.config.auto_approve {
            if glob_match(pattern, path) {
                return true;
            }
        }
        false
    }

    /// 무시 패턴 확인
    pub fn should_ignore(&self, path: &str) -> bool {
        for pattern in &self.config.ignore_patterns {
            if glob_match(pattern, path) {
                return true;
            }
        }
        false
    }

    /// 커스텀 명령어 로드
    pub fn load_commands(&self) -> Result<HashMap<String, CustomCommand>> {
        let commands_dir = self.root.join(".forgecode/commands");
        let mut commands = HashMap::new();

        if commands_dir.exists() {
            for entry in fs::read_dir(&commands_dir)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        let content = fs::read_to_string(&path)?;
                        let command = CustomCommand::parse(&content)?;
                        commands.insert(name.to_string(), command);
                    }
                }
            }
        }

        Ok(commands)
    }

    /// 프로젝트 초기화
    pub fn init(path: &Path) -> Result<Self> {
        let forgecode_dir = path.join(".forgecode");
        let commands_dir = forgecode_dir.join("commands");
        let agents_dir = forgecode_dir.join("agents");

        fs::create_dir_all(&commands_dir)?;
        fs::create_dir_all(&agents_dir)?;

        // 기본 설정 파일 생성
        let default_config = ProjectConfig {
            name: path.file_name().and_then(|s| s.to_str()).map(String::from),
            ignore_patterns: vec![
                "node_modules/**".to_string(),
                "target/**".to_string(),
                ".git/**".to_string(),
                "*.lock".to_string(),
            ],
            ..Default::default()
        };

        let config_path = forgecode_dir.join("config.json");
        let config_json = serde_json::to_string_pretty(&default_config)?;
        fs::write(&config_path, config_json)?;

        // FORGECODE.md 템플릿 생성
        let forgecode_md = path.join("FORGECODE.md");
        if !forgecode_md.exists() {
            fs::write(&forgecode_md, FORGECODE_TEMPLATE)?;
        }

        Self::load(path)
    }
}

/// 커스텀 명령어
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// 명령어 설명
    pub description: String,
    /// 프롬프트 템플릿
    pub prompt: String,
    /// 필요한 인자
    pub args: Vec<String>,
}

impl CustomCommand {
    fn parse(content: &str) -> Result<Self> {
        let lines: Vec<&str> = content.lines().collect();
        let mut description = String::new();
        let mut prompt = String::new();
        let mut args = Vec::new();
        let mut in_prompt = false;

        for line in lines {
            if line.starts_with("# ") {
                description = line[2..].to_string();
            } else if line.starts_with("args:") {
                let args_str = line[5..].trim();
                args = args_str.split(',').map(|s| s.trim().to_string()).collect();
            } else if line.starts_with("---") {
                in_prompt = true;
            } else if in_prompt {
                prompt.push_str(line);
                prompt.push('\n');
            }
        }

        Ok(Self { description, prompt, args })
    }
}

/// 간단한 glob 매칭
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1].trim_start_matches('/');
            return path.starts_with(prefix) && path.ends_with(suffix);
        }
    }
    
    if pattern.starts_with("*.") {
        let ext = &pattern[2..];
        return path.ends_with(&format!(".{}", ext));
    }
    
    pattern == path
}

/// FORGECODE.md 템플릿
const FORGECODE_TEMPLATE: &str = r#"# Project Guidelines

## Overview
Describe your project here.

## Code Style
- Use consistent formatting
- Add comments for complex logic
- Follow language-specific conventions

## Architecture
Describe the project structure and key components.

## Testing
- Run tests before committing
- Add tests for new features

## Notes
Additional notes for the AI assistant.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "src/lib.rs"));
        assert!(glob_match("target/**", "target/debug/main"));
        assert!(glob_match("node_modules/**", "node_modules/foo/bar.js"));
        assert!(!glob_match("*.rs", "main.py"));
    }

    #[test]
    fn test_custom_command_parse() {
        let content = r#"# Generate Tests
args: file

---
Generate comprehensive tests for the file: $file
Include edge cases and error handling.
"#;
        let cmd = CustomCommand::parse(content).unwrap();
        assert_eq!(cmd.description, "Generate Tests");
        assert_eq!(cmd.args, vec!["file"]);
        assert!(cmd.prompt.contains("Generate comprehensive tests"));
    }
}
