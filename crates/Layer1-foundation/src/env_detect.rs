//! Environment Detection - 터미널/OS 환경 자동 감지
//!
//! LLM이 올바른 명령어를 생성하도록 환경 정보를 제공합니다.

use std::env;
use std::path::PathBuf;

/// 운영체제 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsType {
    Windows,
    MacOS,
    Linux,
    Unknown,
}

impl OsType {
    pub fn detect() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOS
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unknown
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::MacOS => "macOS",
            Self::Linux => "Linux",
            Self::Unknown => "Unknown",
        }
    }
}

/// 셸 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    PowerShell,
    Cmd,
    Bash,
    Zsh,
    Fish,
    Sh,
    Unknown,
}

impl ShellKind {
    pub fn detect() -> Self {
        // Check SHELL environment variable (Unix)
        if let Ok(shell) = env::var("SHELL") {
            let shell_lower = shell.to_lowercase();
            if shell_lower.contains("zsh") {
                return Self::Zsh;
            } else if shell_lower.contains("bash") {
                return Self::Bash;
            } else if shell_lower.contains("fish") {
                return Self::Fish;
            } else if shell_lower.contains("/sh") {
                return Self::Sh;
            }
        }

        // Check PSModulePath (PowerShell indicator on Windows)
        if env::var("PSModulePath").is_ok() {
            return Self::PowerShell;
        }

        // Check ComSpec (CMD on Windows)
        if env::var("ComSpec").is_ok() && env::var("PSModulePath").is_err() {
            return Self::Cmd;
        }

        // Default based on OS
        if cfg!(windows) {
            Self::PowerShell
        } else {
            Self::Bash
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::PowerShell => "PowerShell",
            Self::Cmd => "CMD",
            Self::Bash => "Bash",
            Self::Zsh => "Zsh",
            Self::Fish => "Fish",
            Self::Sh => "sh",
            Self::Unknown => "Unknown",
        }
    }

    /// 명령어 구분자 (chaining)
    pub fn command_separator(&self) -> &'static str {
        match self {
            Self::PowerShell | Self::Cmd => ";",
            _ => "&&",
        }
    }

    /// null device
    pub fn null_device(&self) -> &'static str {
        match self {
            Self::PowerShell | Self::Cmd => "NUL",
            _ => "/dev/null",
        }
    }
}

/// 전체 환경 정보
#[derive(Debug, Clone)]
pub struct Environment {
    pub os: OsType,
    pub shell: ShellKind,
    pub home_dir: Option<PathBuf>,
    pub current_dir: PathBuf,
    pub username: Option<String>,
    pub hostname: Option<String>,
    pub arch: &'static str,
    pub has_cargo: bool,
    pub has_node: bool,
    pub has_python: bool,
    pub has_git: bool,
}

impl Environment {
    /// 현재 환경 감지
    pub fn detect() -> Self {
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        
        Self {
            os: OsType::detect(),
            shell: ShellKind::detect(),
            home_dir: dirs::home_dir(),
            current_dir,
            username: env::var("USER")
                .or_else(|_| env::var("USERNAME"))
                .ok(),
            hostname: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok()),
            arch: std::env::consts::ARCH,
            has_cargo: which::which("cargo").is_ok(),
            has_node: which::which("node").is_ok(),
            has_python: which::which("python").is_ok() || which::which("python3").is_ok(),
            has_git: which::which("git").is_ok(),
        }
    }

    /// LLM 시스템 프롬프트용 환경 정보 문자열
    pub fn to_system_info(&self) -> String {
        let mut info = format!(
            "## Environment\n\
            - OS: {} ({})\n\
            - Shell: {}\n\
            - Working Directory: {}\n",
            self.os.name(),
            self.arch,
            self.shell.name(),
            self.current_dir.display()
        );

        // Available tools
        let mut tools = Vec::new();
        if self.has_cargo {
            tools.push("cargo/rust");
        }
        if self.has_node {
            tools.push("node/npm");
        }
        if self.has_python {
            tools.push("python");
        }
        if self.has_git {
            tools.push("git");
        }
        
        if !tools.is_empty() {
            info.push_str(&format!("- Available: {}\n", tools.join(", ")));
        }

        // Shell-specific notes
        match self.shell {
            ShellKind::PowerShell => {
                info.push_str("\n### PowerShell Notes\n");
                info.push_str("- Use `;` to chain commands (not `&&`)\n");
                info.push_str("- Use `$env:VAR` for environment variables\n");
                info.push_str("- Use `-ErrorAction SilentlyContinue` instead of `2>/dev/null`\n");
            }
            ShellKind::Cmd => {
                info.push_str("\n### CMD Notes\n");
                info.push_str("- Use `&` or `&&` to chain commands\n");
                info.push_str("- Use `%VAR%` for environment variables\n");
                info.push_str("- Use `2>NUL` to suppress errors\n");
            }
            _ => {
                info.push_str("\n### Shell Notes\n");
                info.push_str("- Use `&&` to chain commands\n");
                info.push_str("- Use `$VAR` for environment variables\n");
            }
        }

        info
    }

    /// 명령어 변환 (크로스 플랫폼)
    pub fn translate_command(&self, cmd: &str) -> String {
        if self.os != OsType::Windows {
            return cmd.to_string();
        }

        // Windows용 명령어 변환
        let cmd = cmd.trim();
        
        // 기본 Unix 명령어 → PowerShell/CMD 변환
        match cmd.split_whitespace().next() {
            Some("ls") => self.translate_ls(cmd),
            Some("cat") => cmd.replacen("cat ", "type ", 1),
            Some("rm") => self.translate_rm(cmd),
            Some("cp") => cmd.replacen("cp ", "copy ", 1),
            Some("mv") => cmd.replacen("mv ", "move ", 1),
            Some("mkdir") => {
                if cmd.contains("-p") {
                    cmd.replace("mkdir -p ", "mkdir ")
                } else {
                    cmd.to_string()
                }
            }
            Some("touch") => {
                // touch file → New-Item file (PowerShell)
                if let ShellKind::PowerShell = self.shell {
                    cmd.replacen("touch ", "New-Item -ItemType File -Force ", 1)
                } else {
                    cmd.replacen("touch ", "type nul > ", 1)
                }
            }
            Some("which") => {
                if let ShellKind::PowerShell = self.shell {
                    cmd.replacen("which ", "Get-Command ", 1)
                } else {
                    cmd.replacen("which ", "where ", 1)
                }
            }
            Some("grep") => {
                if let ShellKind::PowerShell = self.shell {
                    cmd.replacen("grep ", "Select-String -Pattern ", 1)
                } else {
                    cmd.replacen("grep ", "findstr ", 1)
                }
            }
            Some("clear") => {
                if let ShellKind::PowerShell = self.shell {
                    "Clear-Host".to_string()
                } else {
                    "cls".to_string()
                }
            }
            _ => cmd.to_string(),
        }
    }

    fn translate_ls(&self, cmd: &str) -> String {
        match self.shell {
            ShellKind::PowerShell => {
                if cmd == "ls" || cmd == "ls " {
                    "Get-ChildItem".to_string()
                } else if cmd.contains("-la") || cmd.contains("-al") {
                    cmd.replace("ls -la", "Get-ChildItem -Force")
                        .replace("ls -al", "Get-ChildItem -Force")
                } else if cmd.contains("-l") {
                    cmd.replacen("ls -l", "Get-ChildItem | Format-Table", 1)
                } else if cmd.contains("-a") {
                    cmd.replacen("ls -a", "Get-ChildItem -Force", 1)
                } else {
                    cmd.replacen("ls ", "Get-ChildItem ", 1)
                }
            }
            ShellKind::Cmd => {
                cmd.replacen("ls", "dir", 1)
            }
            _ => cmd.to_string(),
        }
    }

    fn translate_rm(&self, cmd: &str) -> String {
        match self.shell {
            ShellKind::PowerShell => {
                if cmd.contains("-rf") || cmd.contains("-r") {
                    cmd.replace("rm -rf ", "Remove-Item -Recurse -Force ")
                        .replace("rm -r ", "Remove-Item -Recurse ")
                } else if cmd.contains("-f") {
                    cmd.replacen("rm -f ", "Remove-Item -Force ", 1)
                } else {
                    cmd.replacen("rm ", "Remove-Item ", 1)
                }
            }
            ShellKind::Cmd => {
                if cmd.contains("-rf") || cmd.contains("-r") {
                    cmd.replace("rm -rf ", "rmdir /s /q ")
                        .replace("rm -r ", "rmdir /s ")
                } else {
                    cmd.replacen("rm ", "del ", 1)
                }
            }
            _ => cmd.to_string(),
        }
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::detect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_detection() {
        let os = OsType::detect();
        #[cfg(windows)]
        assert_eq!(os, OsType::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(os, OsType::MacOS);
        #[cfg(target_os = "linux")]
        assert_eq!(os, OsType::Linux);
    }

    #[test]
    fn test_environment_info() {
        let env = Environment::detect();
        let info = env.to_system_info();
        assert!(info.contains("OS:"));
        assert!(info.contains("Shell:"));
    }

    #[test]
    #[cfg(windows)]
    fn test_command_translation() {
        let env = Environment {
            os: OsType::Windows,
            shell: ShellKind::PowerShell,
            ..Environment::detect()
        };
        
        assert!(env.translate_command("ls -la").contains("Get-ChildItem"));
        assert!(env.translate_command("rm -rf temp").contains("Remove-Item"));
    }
}
