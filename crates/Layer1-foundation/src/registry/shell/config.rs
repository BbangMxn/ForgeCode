//! Shell Configuration - 쉘 설정
//!
//! bash, powershell, cmd 등 다양한 쉘에 대한 설정을 관리합니다.

use crate::storage::JsonStore;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 설정 파일명
pub const SHELL_FILE: &str = "shell.json";

// ============================================================================
// Shell Type
// ============================================================================

/// 쉘 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellType {
    /// Bash (Linux 기본)
    Bash,
    /// Zsh (macOS 기본)
    Zsh,
    /// Fish
    Fish,
    /// PowerShell (Windows 기본)
    #[serde(alias = "pwsh")]
    PowerShell,
    /// Cmd (Windows 레거시)
    Cmd,
    /// Nushell
    #[serde(alias = "nu")]
    Nushell,
}

impl ShellType {
    /// 현재 OS의 기본 쉘
    pub fn default_for_os() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::PowerShell
        }
        #[cfg(target_os = "macos")]
        {
            Self::Zsh
        }
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            Self::Bash
        }
    }

    /// 쉘 실행 파일 이름 (기본값)
    pub fn default_executable(&self) -> &'static str {
        match self {
            ShellType::Bash => "bash",
            ShellType::Zsh => "zsh",
            ShellType::Fish => "fish",
            ShellType::PowerShell => {
                #[cfg(target_os = "windows")]
                {
                    "powershell.exe"
                }
                #[cfg(not(target_os = "windows"))]
                {
                    "pwsh"
                }
            }
            ShellType::Cmd => "cmd.exe",
            ShellType::Nushell => "nu",
        }
    }

    /// 명령어 실행 인자 (기본값)
    pub fn default_exec_args(&self) -> Vec<&'static str> {
        match self {
            ShellType::Bash | ShellType::Zsh | ShellType::Fish | ShellType::Nushell => {
                vec!["-c"]
            }
            ShellType::PowerShell => vec!["-NoProfile", "-NonInteractive", "-Command"],
            ShellType::Cmd => vec!["/C"],
        }
    }

    /// 문자열에서 파싱
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            "powershell" | "pwsh" => Some(Self::PowerShell),
            "cmd" | "cmd.exe" => Some(Self::Cmd),
            "nu" | "nushell" => Some(Self::Nushell),
            _ => None,
        }
    }

    /// 모든 쉘 타입
    pub fn all() -> Vec<Self> {
        vec![
            Self::Bash,
            Self::Zsh,
            Self::Fish,
            Self::PowerShell,
            Self::Cmd,
            Self::Nushell,
        ]
    }
}

impl Default for ShellType {
    fn default() -> Self {
        Self::default_for_os()
    }
}

impl std::fmt::Display for ShellType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellType::Bash => write!(f, "bash"),
            ShellType::Zsh => write!(f, "zsh"),
            ShellType::Fish => write!(f, "fish"),
            ShellType::PowerShell => write!(f, "powershell"),
            ShellType::Cmd => write!(f, "cmd"),
            ShellType::Nushell => write!(f, "nu"),
        }
    }
}

// ============================================================================
// Individual Shell Config
// ============================================================================

/// 개별 쉘 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSettings {
    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 실행 파일 경로 (기본값 사용 시 None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,

    /// 명령어 실행 인자 (기본값 사용 시 None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,

    /// 추가 환경 변수
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// 명령어 타임아웃 (초)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// 작업 디렉토리 (None이면 현재 디렉토리)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,

    /// 초기화 명령어 (쉘 시작 시 실행)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_command: Option<String>,
}

impl ShellSettings {
    pub fn new() -> Self {
        Self {
            enabled: true,
            executable: None,
            args: None,
            env: HashMap::new(),
            timeout_secs: default_timeout(),
            working_dir: None,
            init_command: None,
        }
    }

    /// 실행 파일 경로 가져오기 (설정 또는 기본값)
    pub fn get_executable(&self, shell_type: ShellType) -> String {
        self.executable
            .clone()
            .unwrap_or_else(|| shell_type.default_executable().to_string())
    }

    /// 실행 인자 가져오기 (설정 또는 기본값)
    pub fn get_args(&self, shell_type: ShellType) -> Vec<String> {
        self.args.clone().unwrap_or_else(|| {
            shell_type
                .default_exec_args()
                .into_iter()
                .map(String::from)
                .collect()
        })
    }

    // Builder methods
    pub fn executable(mut self, exe: impl Into<String>) -> Self {
        self.executable = Some(exe.into());
        self
    }

    pub fn args(mut self, args: Vec<String>) -> Self {
        self.args = Some(args);
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

    pub fn working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn init_command(mut self, cmd: impl Into<String>) -> Self {
        self.init_command = Some(cmd.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

impl Default for ShellSettings {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Shell Config (전체 설정)
// ============================================================================

/// 쉘 설정 관리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// 기본 쉘 타입
    #[serde(default)]
    pub default: ShellType,

    /// 쉘별 설정
    #[serde(default)]
    pub shells: HashMap<String, ShellSettings>,

    /// 글로벌 환경 변수 (모든 쉘에 적용)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub global_env: HashMap<String, String>,

    /// 글로벌 타임아웃 (개별 설정이 없을 때 사용)
    #[serde(default = "default_timeout")]
    pub global_timeout_secs: u64,
}

impl ShellConfig {
    pub fn new() -> Self {
        Self {
            default: ShellType::default_for_os(),
            shells: HashMap::new(),
            global_env: HashMap::new(),
            global_timeout_secs: default_timeout(),
        }
    }

    // ========================================================================
    // Load / Save
    // ========================================================================

    /// 글로벌 + 프로젝트 병합 로드
    pub fn load() -> Result<Self> {
        let mut config = Self::new();

        // 1. 글로벌 설정
        if let Ok(global) = JsonStore::global() {
            if let Some(global_config) = global.load_optional::<ShellConfig>(SHELL_FILE)? {
                config.merge(global_config);
            }
        }

        // 2. 프로젝트 설정
        if let Ok(project) = JsonStore::current_project() {
            if let Some(project_config) = project.load_optional::<ShellConfig>(SHELL_FILE)? {
                config.merge(project_config);
            }
        }

        Ok(config)
    }

    /// 글로벌 설정만 로드
    pub fn load_global() -> Result<Self> {
        let store = JsonStore::global()?;
        Ok(store.load_or_default(SHELL_FILE))
    }

    /// 프로젝트 설정만 로드
    pub fn load_project() -> Result<Self> {
        let store = JsonStore::current_project()?;
        Ok(store.load_or_default(SHELL_FILE))
    }

    /// 글로벌 설정 저장
    pub fn save_global(&self) -> Result<()> {
        let store = JsonStore::global()?;
        store.save(SHELL_FILE, self)
    }

    /// 프로젝트 설정 저장
    pub fn save_project(&self) -> Result<()> {
        let store = JsonStore::current_project()?;
        store.save(SHELL_FILE, self)
    }

    // ========================================================================
    // CRUD
    // ========================================================================

    /// 쉘 설정 가져오기
    pub fn get(&self, shell_type: ShellType) -> ShellSettings {
        self.shells
            .get(&shell_type.to_string())
            .cloned()
            .unwrap_or_default()
    }

    /// 쉘 설정 가져오기 (문자열 키)
    pub fn get_by_name(&self, name: &str) -> Option<&ShellSettings> {
        self.shells.get(name)
    }

    /// 쉘 설정 추가/수정
    pub fn set(&mut self, shell_type: ShellType, settings: ShellSettings) {
        self.shells.insert(shell_type.to_string(), settings);
    }

    /// 기본 쉘 설정
    pub fn set_default(&mut self, shell_type: ShellType) {
        self.default = shell_type;
    }

    /// 기본 쉘 가져오기
    pub fn get_default(&self) -> (ShellType, ShellSettings) {
        (self.default, self.get(self.default))
    }

    /// 쉘 활성화 여부
    pub fn is_enabled(&self, shell_type: ShellType) -> bool {
        self.get(shell_type).enabled
    }

    /// 활성화된 쉘 목록
    pub fn enabled_shells(&self) -> Vec<ShellType> {
        ShellType::all()
            .into_iter()
            .filter(|t| self.is_enabled(*t))
            .collect()
    }

    // ========================================================================
    // Effective Values
    // ========================================================================

    /// 실제 사용할 실행 파일 경로
    pub fn effective_executable(&self, shell_type: ShellType) -> String {
        self.get(shell_type).get_executable(shell_type)
    }

    /// 실제 사용할 실행 인자
    pub fn effective_args(&self, shell_type: ShellType) -> Vec<String> {
        self.get(shell_type).get_args(shell_type)
    }

    /// 실제 사용할 환경 변수 (글로벌 + 쉘별 병합)
    pub fn effective_env(&self, shell_type: ShellType) -> HashMap<String, String> {
        let mut env = self.global_env.clone();
        env.extend(self.get(shell_type).env.clone());
        env
    }

    /// 실제 사용할 타임아웃
    pub fn effective_timeout(&self, shell_type: ShellType) -> u64 {
        let settings = self.get(shell_type);
        if settings.timeout_secs != default_timeout() {
            settings.timeout_secs
        } else {
            self.global_timeout_secs
        }
    }

    // ========================================================================
    // Merge
    // ========================================================================

    /// 다른 설정과 병합 (other가 우선)
    pub fn merge(&mut self, other: ShellConfig) {
        self.default = other.default;
        for (name, settings) in other.shells {
            self.shells.insert(name, settings);
        }
        self.global_env.extend(other.global_env);
        if other.global_timeout_secs != default_timeout() {
            self.global_timeout_secs = other.global_timeout_secs;
        }
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    120 // 2분
}

// ============================================================================
// ShellConfig Trait 구현 (core/traits.rs의 trait 구현)
// ============================================================================

/// ShellSettings를 ShellConfig trait으로 사용하기 위한 래퍼
pub struct ShellRunner {
    pub shell_type: ShellType,
    pub settings: ShellSettings,
    pub global_env: HashMap<String, String>,
}

impl ShellRunner {
    pub fn new(shell_type: ShellType, settings: ShellSettings) -> Self {
        Self {
            shell_type,
            settings,
            global_env: HashMap::new(),
        }
    }

    pub fn with_global_env(mut self, env: HashMap<String, String>) -> Self {
        self.global_env = env;
        self
    }

    /// 명령어 실행을 위한 전체 커맨드 생성
    pub fn build_command(&self, command: &str) -> (String, Vec<String>) {
        let executable = self.settings.get_executable(self.shell_type);
        let mut args = self.settings.get_args(self.shell_type);
        args.push(command.to_string());
        (executable, args)
    }

    /// 환경 변수 (글로벌 + 쉘별)
    pub fn env(&self) -> HashMap<String, String> {
        let mut env = self.global_env.clone();
        env.extend(self.settings.env.clone());
        env
    }

    /// 타임아웃
    pub fn timeout(&self) -> u64 {
        self.settings.timeout_secs
    }

    /// 작업 디렉토리
    pub fn working_dir(&self) -> Option<&std::path::Path> {
        self.settings.working_dir.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_type_default() {
        let shell = ShellType::default_for_os();
        #[cfg(target_os = "windows")]
        assert_eq!(shell, ShellType::PowerShell);
        #[cfg(target_os = "macos")]
        assert_eq!(shell, ShellType::Zsh);
    }

    #[test]
    fn test_shell_settings_builder() {
        let settings = ShellSettings::new()
            .executable("/usr/local/bin/bash")
            .args(vec!["-c".to_string()])
            .env("LANG", "en_US.UTF-8")
            .timeout(60);

        assert_eq!(
            settings.executable,
            Some("/usr/local/bin/bash".to_string())
        );
        assert_eq!(settings.timeout_secs, 60);
        assert!(settings.env.contains_key("LANG"));
    }

    #[test]
    fn test_shell_config() {
        let mut config = ShellConfig::new();

        config.set(
            ShellType::Bash,
            ShellSettings::new()
                .executable("/bin/bash")
                .env("TERM", "xterm-256color"),
        );

        assert_eq!(
            config.effective_executable(ShellType::Bash),
            "/bin/bash".to_string()
        );
        assert!(config
            .effective_env(ShellType::Bash)
            .contains_key("TERM"));
    }

    #[test]
    fn test_shell_runner() {
        let settings = ShellSettings::new();
        let runner = ShellRunner::new(ShellType::Bash, settings);

        let (exe, args) = runner.build_command("echo hello");
        assert_eq!(exe, "bash");
        assert_eq!(args, vec!["-c", "echo hello"]);
    }
}
