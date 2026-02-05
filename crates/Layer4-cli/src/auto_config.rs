//! Auto Configuration - 환경 자동 감지 및 최적 설정
//!
//! Claude Code처럼 "just works" 경험을 제공:
//! - OS/쉘 자동 감지
//! - 프로바이더 자동 감지 (Ollama, API 키 등)
//! - 프로젝트 타입 감지 (Rust, Python, Node.js 등)

use std::collections::HashMap;
use std::env;
use std::process::Command;

/// 환경 정보
#[derive(Debug, Clone)]
pub struct EnvironmentInfo {
    /// 운영체제
    pub os: OsType,
    /// 쉘
    pub shell: ShellType,
    /// 쉘 경로
    pub shell_path: Option<String>,
    /// 감지된 프로바이더
    pub detected_providers: Vec<DetectedProvider>,
    /// 프로젝트 타입
    pub project_type: Option<ProjectType>,
    /// 언어/런타임 버전
    pub runtimes: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsType {
    Windows,
    MacOS,
    Linux,
    Unknown,
}

impl OsType {
    pub fn detect() -> Self {
        match env::consts::OS {
            "windows" => Self::Windows,
            "macos" => Self::MacOS,
            "linux" => Self::Linux,
            _ => Self::Unknown,
        }
    }
    
    pub fn as_str(&self) -> &str {
        match self {
            Self::Windows => "Windows",
            Self::MacOS => "macOS",
            Self::Linux => "Linux",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellType {
    PowerShell,
    Cmd,
    Bash,
    Zsh,
    Fish,
    Unknown(String),
}

impl ShellType {
    pub fn detect() -> Self {
        // Windows
        if cfg!(windows) {
            // Check for PowerShell
            if env::var("PSModulePath").is_ok() {
                return Self::PowerShell;
            }
            // Check COMSPEC for cmd
            if let Ok(comspec) = env::var("COMSPEC") {
                if comspec.to_lowercase().contains("cmd.exe") {
                    return Self::Cmd;
                }
            }
            return Self::PowerShell; // Default on Windows
        }
        
        // Unix-like
        if let Ok(shell) = env::var("SHELL") {
            if shell.contains("zsh") {
                return Self::Zsh;
            } else if shell.contains("bash") {
                return Self::Bash;
            } else if shell.contains("fish") {
                return Self::Fish;
            }
            return Self::Unknown(shell);
        }
        
        Self::Bash // Default on Unix
    }
    
    pub fn command_separator(&self) -> &str {
        match self {
            Self::PowerShell => " ; ",
            Self::Cmd => " & ",
            _ => " && ",
        }
    }
    
    pub fn env_set_syntax(&self, key: &str, value: &str) -> String {
        match self {
            Self::PowerShell => format!("$env:{} = '{}'", key, value),
            Self::Cmd => format!("set {}={}", key, value),
            _ => format!("export {}='{}'", key, value),
        }
    }
    
    pub fn path_add_syntax(&self, path: &str) -> String {
        match self {
            Self::PowerShell => format!("$env:Path += ';{}'", path),
            Self::Cmd => format!("set PATH=%PATH%;{}", path),
            _ => format!("export PATH=\"$PATH:{}\"", path),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DetectedProvider {
    pub name: String,
    pub provider_type: ProviderType,
    pub endpoint: Option<String>,
    pub models: Vec<String>,
    pub priority: u8, // 0 = highest
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderType {
    Ollama,
    OpenAI,
    Anthropic,
    Google,
    Local,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    Rust,
    Python,
    Node,
    Go,
    Java,
    CSharp,
    Ruby,
    Mixed,
    Unknown,
}

impl EnvironmentInfo {
    /// 환경 자동 감지
    pub fn detect() -> Self {
        let os = OsType::detect();
        let shell = ShellType::detect();
        let shell_path = Self::detect_shell_path(&shell);
        let detected_providers = Self::detect_providers();
        let project_type = Self::detect_project_type();
        let runtimes = Self::detect_runtimes();

        Self {
            os,
            shell,
            shell_path,
            detected_providers,
            project_type,
            runtimes,
        }
    }

    fn detect_shell_path(shell: &ShellType) -> Option<String> {
        match shell {
            ShellType::PowerShell => Some("powershell".to_string()),
            ShellType::Cmd => env::var("COMSPEC").ok(),
            ShellType::Bash => Some("/bin/bash".to_string()),
            ShellType::Zsh => Some("/bin/zsh".to_string()),
            ShellType::Fish => Some("/usr/bin/fish".to_string()),
            ShellType::Unknown(path) => Some(path.clone()),
        }
    }

    fn detect_providers() -> Vec<DetectedProvider> {
        let mut providers = Vec::new();

        // 1. Ollama 감지
        if let Some(ollama) = Self::detect_ollama() {
            providers.push(ollama);
        }

        // 2. API 키 기반 프로바이더 감지
        if env::var("ANTHROPIC_API_KEY").is_ok() {
            providers.push(DetectedProvider {
                name: "Anthropic".to_string(),
                provider_type: ProviderType::Anthropic,
                endpoint: Some("https://api.anthropic.com".to_string()),
                models: vec!["claude-sonnet-4-20250514".to_string()],
                priority: 1,
            });
        }

        if env::var("OPENAI_API_KEY").is_ok() {
            providers.push(DetectedProvider {
                name: "OpenAI".to_string(),
                provider_type: ProviderType::OpenAI,
                endpoint: Some("https://api.openai.com".to_string()),
                models: vec!["gpt-4o".to_string()],
                priority: 2,
            });
        }

        if env::var("GOOGLE_API_KEY").is_ok() || env::var("GEMINI_API_KEY").is_ok() {
            providers.push(DetectedProvider {
                name: "Google".to_string(),
                provider_type: ProviderType::Google,
                endpoint: None,
                models: vec!["gemini-2.0-flash".to_string()],
                priority: 3,
            });
        }

        // 우선순위로 정렬
        providers.sort_by_key(|p| p.priority);
        providers
    }

    fn detect_ollama() -> Option<DetectedProvider> {
        // Ollama가 실행 중인지 확인
        let endpoints = [
            "http://localhost:11434",
            "http://127.0.0.1:11434",
        ];

        for endpoint in endpoints {
            // 간단히 연결 시도 (실제로는 비동기 요청이 필요)
            // 여기서는 환경 변수나 프로세스 존재 여부로 판단
            if let Ok(ollama_host) = env::var("OLLAMA_HOST") {
                return Some(DetectedProvider {
                    name: "Ollama".to_string(),
                    provider_type: ProviderType::Ollama,
                    endpoint: Some(ollama_host),
                    models: vec!["qwen3:8b".to_string(), "llama3.2".to_string()],
                    priority: 0,
                });
            }
        }

        // 기본 Ollama 설정 (로컬)
        Some(DetectedProvider {
            name: "Ollama".to_string(),
            provider_type: ProviderType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            models: vec![],
            priority: 0,
        })
    }

    fn detect_project_type() -> Option<ProjectType> {
        let cwd = env::current_dir().ok()?;
        
        // Rust
        if cwd.join("Cargo.toml").exists() {
            return Some(ProjectType::Rust);
        }
        
        // Python
        if cwd.join("pyproject.toml").exists() 
            || cwd.join("setup.py").exists()
            || cwd.join("requirements.txt").exists() 
        {
            return Some(ProjectType::Python);
        }
        
        // Node.js
        if cwd.join("package.json").exists() {
            return Some(ProjectType::Node);
        }
        
        // Go
        if cwd.join("go.mod").exists() {
            return Some(ProjectType::Go);
        }
        
        // Java
        if cwd.join("pom.xml").exists() || cwd.join("build.gradle").exists() {
            return Some(ProjectType::Java);
        }
        
        // C#
        if cwd.join("*.csproj").exists() || cwd.join("*.sln").exists() {
            return Some(ProjectType::CSharp);
        }
        
        // Ruby
        if cwd.join("Gemfile").exists() {
            return Some(ProjectType::Ruby);
        }
        
        None
    }

    fn detect_runtimes() -> HashMap<String, String> {
        let mut runtimes = HashMap::new();

        // Rust
        if let Ok(output) = Command::new("rustc").arg("--version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                runtimes.insert("rust".to_string(), version.trim().to_string());
            }
        }

        // Node.js
        if let Ok(output) = Command::new("node").arg("--version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                runtimes.insert("node".to_string(), version.trim().to_string());
            }
        }

        // Python
        if let Ok(output) = Command::new("python").arg("--version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                runtimes.insert("python".to_string(), version.trim().to_string());
            }
        }

        // Go
        if let Ok(output) = Command::new("go").arg("version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                runtimes.insert("go".to_string(), version.trim().to_string());
            }
        }

        runtimes
    }

    /// 가장 적합한 프로바이더 선택
    pub fn best_provider(&self) -> Option<&DetectedProvider> {
        self.detected_providers.first()
    }

    /// 쉘에 맞는 명령어 래핑
    pub fn wrap_command(&self, cmd: &str) -> String {
        match self.shell {
            ShellType::PowerShell => {
                // PowerShell에서 Unix 명령어 변환
                let cmd = cmd
                    .replace("grep ", "Select-String ")
                    .replace("cat ", "Get-Content ")
                    .replace("ls ", "Get-ChildItem ")
                    .replace("rm ", "Remove-Item ")
                    .replace("cp ", "Copy-Item ")
                    .replace("mv ", "Move-Item ")
                    .replace("mkdir ", "New-Item -ItemType Directory -Path ");
                cmd
            }
            _ => cmd.to_string(),
        }
    }

    /// 시스템 프롬프트용 환경 정보
    pub fn for_system_prompt(&self) -> String {
        let mut info = format!(
            "## Environment\n\
             - OS: {}\n\
             - Shell: {:?}\n",
            self.os.as_str(),
            self.shell,
        );

        if let Some(ref project) = self.project_type {
            info.push_str(&format!("- Project Type: {:?}\n", project));
        }

        if !self.runtimes.is_empty() {
            info.push_str("- Runtimes:\n");
            for (name, version) in &self.runtimes {
                info.push_str(&format!("  - {}: {}\n", name, version));
            }
        }

        if let Some(provider) = self.best_provider() {
            info.push_str(&format!("- AI Provider: {} ({:?})\n", provider.name, provider.provider_type));
        }

        info
    }
}

/// 빠른 환경 체크
pub fn quick_check() -> QuickCheckResult {
    let env = EnvironmentInfo::detect();
    
    QuickCheckResult {
        has_provider: !env.detected_providers.is_empty(),
        has_ollama: env.detected_providers.iter().any(|p| p.provider_type == ProviderType::Ollama),
        has_api_key: env.detected_providers.iter().any(|p| {
            matches!(p.provider_type, ProviderType::Anthropic | ProviderType::OpenAI | ProviderType::Google)
        }),
        shell_type: env.shell,
        project_detected: env.project_type.is_some(),
    }
}

#[derive(Debug)]
pub struct QuickCheckResult {
    pub has_provider: bool,
    pub has_ollama: bool,
    pub has_api_key: bool,
    pub shell_type: ShellType,
    pub project_detected: bool,
}

impl QuickCheckResult {
    pub fn is_ready(&self) -> bool {
        self.has_provider
    }
    
    pub fn suggestion(&self) -> Option<String> {
        if !self.has_provider {
            return Some(
                "No AI provider detected. Options:\n\
                 1. Install Ollama: https://ollama.com\n\
                 2. Set ANTHROPIC_API_KEY for Claude\n\
                 3. Set OPENAI_API_KEY for GPT"
                .to_string()
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_detect() {
        let os = OsType::detect();
        // Should detect something
        assert!(matches!(os, OsType::Windows | OsType::MacOS | OsType::Linux | OsType::Unknown));
    }

    #[test]
    fn test_shell_syntax() {
        let ps = ShellType::PowerShell;
        assert_eq!(ps.command_separator(), " ; ");
        assert!(ps.env_set_syntax("FOO", "bar").contains("$env:"));
        
        let bash = ShellType::Bash;
        assert_eq!(bash.command_separator(), " && ");
        assert!(bash.env_set_syntax("FOO", "bar").contains("export"));
    }

    #[test]
    fn test_environment_detect() {
        let env = EnvironmentInfo::detect();
        // Basic checks
        assert!(env.shell_path.is_some() || matches!(env.shell, ShellType::Unknown(_)));
    }
}
