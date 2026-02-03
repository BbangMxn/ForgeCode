use crate::storage::JsonStore;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 설정 파일명
pub const MCP_FILE: &str = "mcp.json";

/// MCP 서버 타입 (전송 방식)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// 로컬 프로세스 (stdin/stdout)
    Stdio,
    /// HTTP Server-Sent Events
    Sse,
}

impl Default for McpTransport {
    fn default() -> Self {
        Self::Stdio
    }
}

/// 개별 MCP 서버 설정
///
/// Claude Code 호환 형식:
/// ```json
/// {
///   "command": "npx",
///   "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"],
///   "env": { "KEY": "value" }
/// }
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServer {
    /// 전송 타입 (기본: stdio)
    #[serde(rename = "type", default)]
    pub transport: McpTransport,

    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,

    // === stdio 전용 ===
    /// 실행 명령어
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// 명령어 인자
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// 작업 디렉토리
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,

    // === sse 전용 ===
    /// 서버 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    // === 공통 ===
    /// 환경 변수 (${VAR} 형식 지원)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// 연결 타임아웃 (초)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

impl McpServer {
    /// stdio 타입 서버 생성
    pub fn stdio(command: impl Into<String>) -> Self {
        Self {
            transport: McpTransport::Stdio,
            enabled: true,
            command: Some(command.into()),
            args: vec![],
            cwd: None,
            url: None,
            env: HashMap::new(),
            timeout_secs: default_timeout(),
        }
    }

    /// sse 타입 서버 생성
    pub fn sse(url: impl Into<String>) -> Self {
        Self {
            transport: McpTransport::Sse,
            enabled: true,
            command: None,
            args: vec![],
            cwd: None,
            url: Some(url.into()),
            env: HashMap::new(),
            timeout_secs: default_timeout(),
        }
    }

    /// 유효성 검증
    pub fn validate(&self) -> std::result::Result<(), String> {
        match self.transport {
            McpTransport::Stdio => {
                if self.command.is_none() {
                    return Err("stdio server requires 'command'".to_string());
                }
            }
            McpTransport::Sse => {
                if self.url.is_none() {
                    return Err("sse server requires 'url'".to_string());
                }
            }
        }
        Ok(())
    }

    /// 환경변수 확장 (${VAR} 또는 ${VAR:-default})
    pub fn expand_env(&self) -> HashMap<String, String> {
        self.env
            .iter()
            .map(|(k, v)| (k.clone(), expand_env_var(v)))
            .collect()
    }

    // === Builder methods ===

    pub fn args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// MCP 설정 (서버 컬렉션)
///
/// TOML 형식:
/// ```toml
/// [mcp.servers.filesystem]
/// command = "npx"
/// args = ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
/// ```
///
/// JSON 형식 (Claude Code 호환):
/// ```json
/// {
///   "mcpServers": {
///     "filesystem": {
///       "command": "npx",
///       "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct McpConfig {
    /// MCP 서버들 (이름 -> 설정)
    #[serde(default)]
    pub servers: HashMap<String, McpServer>,
}

impl McpConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// 서버 추가
    pub fn add(&mut self, name: impl Into<String>, server: McpServer) {
        self.servers.insert(name.into(), server);
    }

    /// 서버 조회
    pub fn get(&self, name: &str) -> Option<&McpServer> {
        self.servers.get(name)
    }

    /// 서버 가변 조회
    pub fn get_mut(&mut self, name: &str) -> Option<&mut McpServer> {
        self.servers.get_mut(name)
    }

    /// 서버 제거
    pub fn remove(&mut self, name: &str) -> Option<McpServer> {
        self.servers.remove(name)
    }

    /// 서버 존재 여부
    pub fn contains(&self, name: &str) -> bool {
        self.servers.contains_key(name)
    }

    /// 서버 개수
    pub fn len(&self) -> usize {
        self.servers.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    /// 전체 서버 목록
    pub fn iter(&self) -> impl Iterator<Item = (&String, &McpServer)> {
        self.servers.iter()
    }

    /// 활성화된 서버만
    pub fn iter_enabled(&self) -> impl Iterator<Item = (&String, &McpServer)> {
        self.servers.iter().filter(|(_, s)| s.enabled)
    }

    /// 타입별 서버
    pub fn iter_by_transport(
        &self,
        transport: McpTransport,
    ) -> impl Iterator<Item = (&String, &McpServer)> {
        self.servers
            .iter()
            .filter(move |(_, s)| s.transport == transport)
    }

    /// 유효성 검증
    pub fn validate(&self) -> std::result::Result<(), Vec<String>> {
        let errors: Vec<_> = self
            .servers
            .iter()
            .filter_map(|(name, s)| s.validate().err().map(|e| format!("{}: {}", name, e)))
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 다른 설정과 병합 (other가 우선)
    pub fn merge(&mut self, other: McpConfig) {
        for (name, server) in other.servers {
            self.servers.insert(name, server);
        }
    }

    // === Storage 연동 ===

    /// 글로벌 + 프로젝트 MCP 설정 로드
    pub fn load() -> Result<Self> {
        let mut config = McpConfig::new();

        // 1. 글로벌 설정 (~/.forgecode/mcp.json)
        if let Ok(global) = JsonStore::global() {
            if let Some(global_mcp) = global.load_optional::<McpConfigFile>(MCP_FILE)? {
                config.merge(global_mcp.mcp_servers);
            }
        }

        // 2. 프로젝트 설정 (.forgecode/mcp.json)
        if let Ok(project) = JsonStore::current_project() {
            if let Some(project_mcp) = project.load_optional::<McpConfigFile>(MCP_FILE)? {
                config.merge(project_mcp.mcp_servers);
            }
        }

        Ok(config)
    }

    /// 글로벌 MCP 설정 로드
    pub fn load_global() -> Result<Self> {
        let store = JsonStore::global()?;
        let file: McpConfigFile = store.load_or_default(MCP_FILE);
        Ok(file.mcp_servers)
    }

    /// 프로젝트 MCP 설정 로드
    pub fn load_project() -> Result<Self> {
        let store = JsonStore::current_project()?;
        let file: McpConfigFile = store.load_or_default(MCP_FILE);
        Ok(file.mcp_servers)
    }

    /// 글로벌 MCP 설정 저장
    pub fn save_global(&self) -> Result<()> {
        let store = JsonStore::global()?;
        let file = McpConfigFile {
            mcp_servers: self.clone(),
        };
        store.save(MCP_FILE, &file)
    }

    /// 프로젝트 MCP 설정 저장
    pub fn save_project(&self) -> Result<()> {
        let store = JsonStore::current_project()?;
        let file = McpConfigFile {
            mcp_servers: self.clone(),
        };
        store.save(MCP_FILE, &file)
    }
}

/// Claude Code 호환 MCP 설정 파일 구조
///
/// `.mcp.json` 또는 `mcp.json`:
/// ```json
/// {
///   "mcpServers": {
///     "filesystem": {
///       "command": "npx",
///       "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct McpConfigFile {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: McpConfig,
}

/// 환경변수 확장
/// - ${VAR}: 환경변수 값
/// - ${VAR:-default}: 환경변수가 없으면 기본값
fn expand_env_var(value: &str) -> String {
    let mut result = value.to_string();

    // ${VAR:-default} 패턴
    let re_default = regex::Regex::new(r"\$\{([^}:]+):-([^}]*)\}").unwrap();
    result = re_default
        .replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let default_val = &caps[2];
            std::env::var(var_name).unwrap_or_else(|_| default_val.to_string())
        })
        .to_string();

    // ${VAR} 패턴
    let re_simple = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    result = re_simple
        .replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            std::env::var(var_name).unwrap_or_default()
        })
        .to_string();

    result
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_server() {
        let server = McpServer::stdio("npx")
            .arg("-y")
            .arg("@modelcontextprotocol/server-filesystem")
            .arg("/home/user/Desktop")
            .env("NODE_ENV", "production");

        assert_eq!(server.transport, McpTransport::Stdio);
        assert_eq!(server.command, Some("npx".to_string()));
        assert_eq!(
            server.args,
            vec![
                "-y",
                "@modelcontextprotocol/server-filesystem",
                "/home/user/Desktop"
            ]
        );
        assert!(server.validate().is_ok());
    }

    #[test]
    fn test_sse_server() {
        let server = McpServer::sse("http://localhost:3000/sse");

        assert_eq!(server.transport, McpTransport::Sse);
        assert_eq!(server.url, Some("http://localhost:3000/sse".to_string()));
        assert!(server.validate().is_ok());
    }

    #[test]
    fn test_mcp_config() {
        let mut config = McpConfig::new();
        config.add(
            "filesystem",
            McpServer::stdio("npx")
                .arg("-y")
                .arg("@modelcontextprotocol/server-filesystem"),
        );
        config.add("remote", McpServer::sse("http://localhost:3000/sse"));

        assert_eq!(config.len(), 2);
        assert!(config.contains("filesystem"));
        assert!(config.get("filesystem").is_some());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_env_expansion() {
        std::env::set_var("TEST_VAR", "test_value");

        let expanded = expand_env_var("prefix_${TEST_VAR}_suffix");
        assert_eq!(expanded, "prefix_test_value_suffix");

        let with_default = expand_env_var("${NONEXISTENT:-default_val}");
        assert_eq!(with_default, "default_val");

        std::env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_validation_error() {
        let invalid_stdio = McpServer {
            transport: McpTransport::Stdio,
            enabled: true,
            command: None, // 필수인데 없음
            args: vec![],
            cwd: None,
            url: None,
            env: HashMap::new(),
            timeout_secs: 30,
        };

        assert!(invalid_stdio.validate().is_err());
    }
}
