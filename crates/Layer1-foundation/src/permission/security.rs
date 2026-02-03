//! Security - 위험 명령어 및 민감 경로 관리
//!
//! Layer2-tool에서 사용할 보안 체크 기능을 제공합니다.
//! - 위험 명령어 탐지 (forbidden, dangerous, caution)
//! - 민감 경로 탐지
//! - 안전한 명령어 목록

use regex::Regex;
use std::sync::OnceLock;

// ============================================================
// 위험 명령어 분류
// ============================================================

/// 명령어 위험도 분류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRisk {
    /// 안전 - 자동 승인 가능 (ls, pwd, echo 등)
    Safe,
    /// 주의 필요 - 세션 승인 (mkdir, touch, cp 등)
    Caution,
    /// 위험 - 매번 확인 (rm, mv, git push 등)
    Dangerous,
    /// 금지 - 항상 차단 (rm -rf /, fork bomb 등)
    Forbidden,
    /// 대화형 - 특수 처리 (vim, htop 등)
    Interactive,
    /// 알 수 없음 - 기본적으로 확인 필요
    Unknown,
}

impl CommandRisk {
    /// 위험도 점수 (0-10)
    pub fn score(&self) -> u8 {
        match self {
            CommandRisk::Safe => 0,
            CommandRisk::Caution => 3,
            CommandRisk::Interactive => 4,
            CommandRisk::Unknown => 5,
            CommandRisk::Dangerous => 7,
            CommandRisk::Forbidden => 10,
        }
    }

    /// 자동 승인 가능 여부
    pub fn can_auto_approve(&self) -> bool {
        matches!(self, CommandRisk::Safe)
    }

    /// 세션 승인으로 충분한지
    pub fn session_approve_ok(&self) -> bool {
        matches!(
            self,
            CommandRisk::Safe | CommandRisk::Caution | CommandRisk::Interactive
        )
    }

    /// 차단 여부
    pub fn is_blocked(&self) -> bool {
        matches!(self, CommandRisk::Forbidden)
    }
}

// ============================================================
// 금지 명령어 패턴 (항상 차단)
// ============================================================

/// 금지된 명령어 패턴들
pub fn forbidden_patterns() -> Vec<ForbiddenPattern> {
    vec![
        // 시스템 파괴
        ForbiddenPattern::new("rm -rf /", "Root filesystem deletion"),
        ForbiddenPattern::new("rm -rf /*", "Root filesystem deletion"),
        ForbiddenPattern::new("rm -fr /", "Root filesystem deletion"),
        ForbiddenPattern::regex(r"rm\s+(-[rf]+\s+)+/\s*$", "Root filesystem deletion"),
        ForbiddenPattern::regex(r"rm\s+(-[rf]+\s+)+/\*", "Root filesystem deletion"),
        // Fork bomb
        ForbiddenPattern::new(":(){ :|:& };:", "Fork bomb"),
        ForbiddenPattern::regex(r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:", "Fork bomb"),
        ForbiddenPattern::contains(".444", "Possible obfuscated fork bomb"),
        // 디스크 파괴
        ForbiddenPattern::regex(r"dd\s+if=.*of=/dev/[sh]d[a-z]", "Disk overwrite"),
        ForbiddenPattern::regex(r">\s*/dev/[sh]d[a-z]", "Disk overwrite"),
        ForbiddenPattern::regex(r"mkfs\.", "Filesystem format"),
        // 시스템 종료
        ForbiddenPattern::new("shutdown", "System shutdown"),
        ForbiddenPattern::new("reboot", "System reboot"),
        ForbiddenPattern::new("init 0", "System halt"),
        ForbiddenPattern::new("init 6", "System reboot"),
        ForbiddenPattern::new("halt", "System halt"),
        ForbiddenPattern::new("poweroff", "System poweroff"),
        // 권한 파괴
        ForbiddenPattern::regex(r"chmod\s+(-R\s+)?777\s+/", "Dangerous permission change"),
        ForbiddenPattern::regex(r"chown\s+(-R\s+)?.*\s+/", "Dangerous ownership change"),
        // 네트워크 악용
        ForbiddenPattern::contains("| nc ", "Potential reverse shell"),
        ForbiddenPattern::contains("| netcat ", "Potential reverse shell"),
        ForbiddenPattern::regex(r"bash\s+-i\s+>&\s*/dev/tcp", "Reverse shell"),
        ForbiddenPattern::contains("/dev/tcp/", "Network device access"),
        // 암호화/랜섬웨어 패턴
        ForbiddenPattern::regex(r"openssl\s+enc\s+.*-in\s+/", "Bulk encryption"),
        ForbiddenPattern::regex(r"gpg\s+.*--encrypt.*\s+/", "Bulk encryption"),
        // 히스토리 삭제 (증거 인멸)
        ForbiddenPattern::new("history -c", "History clear"),
        ForbiddenPattern::regex(r">\s*~/\.bash_history", "History deletion"),
        ForbiddenPattern::regex(r"rm\s+.*\.bash_history", "History deletion"),
        // 커널 모듈
        ForbiddenPattern::regex(r"insmod\s+", "Kernel module insertion"),
        ForbiddenPattern::regex(r"modprobe\s+", "Kernel module loading"),
        // 프로세스 무차별 종료
        ForbiddenPattern::new("killall -9", "Mass process kill"),
        ForbiddenPattern::new("pkill -9", "Mass process kill"),
    ]
}

/// 금지 패턴 정의
#[derive(Debug, Clone)]
pub struct ForbiddenPattern {
    pub pattern: PatternType,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub enum PatternType {
    Exact(String),
    Contains(String),
    Regex(String),
}

impl ForbiddenPattern {
    pub fn new(exact: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            pattern: PatternType::Exact(exact.into()),
            reason: reason.into(),
        }
    }

    pub fn contains(substring: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            pattern: PatternType::Contains(substring.into()),
            reason: reason.into(),
        }
    }

    pub fn regex(pattern: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            pattern: PatternType::Regex(pattern.into()),
            reason: reason.into(),
        }
    }

    /// 명령어가 이 패턴에 매칭되는지 확인
    pub fn matches(&self, command: &str) -> bool {
        match &self.pattern {
            PatternType::Exact(s) => command.trim() == s,
            PatternType::Contains(s) => command.contains(s),
            PatternType::Regex(r) => Regex::new(r)
                .map(|re| re.is_match(command))
                .unwrap_or(false),
        }
    }
}

// ============================================================
// 위험 명령어 패턴 (확인 필요)
// ============================================================

/// 위험한 명령어들 (삭제, 수정, 시스템 변경)
pub fn dangerous_commands() -> Vec<&'static str> {
    vec![
        // 파일 삭제
        "rm",
        "rmdir",
        "unlink",
        // 파일 이동/이름변경
        "mv",
        // Git 위험 명령
        "git push",
        "git push -f",
        "git push --force",
        "git reset --hard",
        "git clean -fd",
        "git checkout -- .",
        // 패키지 관리
        "npm publish",
        "cargo publish",
        "pip uninstall",
        // 데이터베이스
        "DROP",
        "DELETE FROM",
        "TRUNCATE",
        // 서비스 관리
        "systemctl stop",
        "systemctl disable",
        "service stop",
        // 환경 변수
        "export PATH=",
        "unset PATH",
    ]
}

/// 주의가 필요한 명령어들
pub fn caution_commands() -> Vec<&'static str> {
    vec![
        // 파일 생성/수정
        "touch",
        "mkdir",
        "cp",
        "ln",
        // 권한 변경 (제한적)
        "chmod",
        "chown",
        // Git 일반 명령
        "git add",
        "git commit",
        "git stash",
        "git merge",
        "git rebase",
        "git pull",
        // 패키지 설치
        "npm install",
        "pip install",
        "cargo add",
        "apt install",
        "brew install",
        // 에디터 (파일 수정 가능)
        "nano",
        "sed -i",
        // 빌드/테스트
        "cargo build",
        "npm run",
        "make",
    ]
}

// ============================================================
// 안전한 명령어 (자동 승인)
// ============================================================

/// 안전한 명령어들 (읽기 전용, 정보 조회)
pub fn safe_commands() -> Vec<&'static str> {
    vec![
        // 파일 시스템 조회
        "ls",
        "dir",
        "pwd",
        "cd",
        "cat",
        "head",
        "tail",
        "less",
        "more",
        "file",
        "stat",
        "wc",
        "find",
        "locate",
        "tree",
        "du",
        "df",
        // 텍스트 처리 (읽기)
        "grep",
        "rg",
        "ag",
        "awk",
        "sed", // -i 없이
        "sort",
        "uniq",
        "cut",
        "tr",
        "diff",
        // 시스템 정보
        "whoami",
        "id",
        "hostname",
        "uname",
        "date",
        "uptime",
        "free",
        "top",
        "htop",
        "ps",
        "pgrep",
        // 환경
        "env",
        "printenv",
        "echo",
        "printf",
        "which",
        "whereis",
        "type",
        // Git 읽기
        "git status",
        "git log",
        "git diff",
        "git branch",
        "git show",
        "git remote -v",
        "git tag",
        // 버전 확인
        "node --version",
        "npm --version",
        "python --version",
        "cargo --version",
        "rustc --version",
        "go version",
        "java --version",
        // 패키지 조회
        "npm list",
        "pip list",
        "cargo tree",
        // 빌드/테스트 (대부분 안전)
        "cargo check",
        "cargo test",
        "cargo clippy",
        "npm test",
        "pytest",
    ]
}

/// 대화형 명령어들 (특수 처리 필요)
pub fn interactive_commands() -> Vec<&'static str> {
    vec![
        "vim",
        "nvim",
        "vi",
        "nano",
        "emacs",
        "htop",
        "top",
        "less",
        "more",
        "man",
        "python",
        "python3",
        "node",
        "irb",
        "ghci",
        "psql",
        "mysql",
        "sqlite3",
        "mongosh",
        "redis-cli",
        "ssh",
        "telnet",
        "ftp",
        "sftp",
    ]
}

// ============================================================
// 민감 경로
// ============================================================

/// 민감한 파일 패턴들
pub fn sensitive_file_patterns() -> Vec<SensitivePath> {
    vec![
        // 환경 변수 / 시크릿
        SensitivePath::new("**/.env", "Environment variables", 8),
        SensitivePath::new("**/.env.*", "Environment variables", 8),
        SensitivePath::new("**/.env.local", "Local environment", 8),
        SensitivePath::new("**/secrets.*", "Secrets file", 9),
        SensitivePath::new("**/credentials*", "Credentials", 9),
        SensitivePath::new("**/*.secret", "Secret file", 9),
        // SSH / 암호화 키
        SensitivePath::new("~/.ssh/**", "SSH directory", 10),
        SensitivePath::new("**/*.pem", "PEM certificate", 9),
        SensitivePath::new("**/*.key", "Private key", 9),
        SensitivePath::new("**/*_rsa", "RSA key", 10),
        SensitivePath::new("**/*_dsa", "DSA key", 10),
        SensitivePath::new("**/*_ecdsa", "ECDSA key", 10),
        SensitivePath::new("**/*_ed25519", "ED25519 key", 10),
        SensitivePath::new("**/id_rsa*", "RSA identity", 10),
        // 클라우드 자격 증명
        SensitivePath::new("~/.aws/**", "AWS credentials", 10),
        SensitivePath::new("~/.azure/**", "Azure credentials", 10),
        SensitivePath::new("~/.gcloud/**", "GCloud credentials", 10),
        SensitivePath::new("~/.config/gcloud/**", "GCloud config", 10),
        SensitivePath::new("**/.npmrc", "NPM credentials", 7),
        SensitivePath::new("**/.pypirc", "PyPI credentials", 7),
        SensitivePath::new(
            "**/docker-compose*.yml",
            "Docker compose (may have secrets)",
            5,
        ),
        // 데이터베이스
        SensitivePath::new("**/*.sqlite", "SQLite database", 6),
        SensitivePath::new("**/*.db", "Database file", 6),
        SensitivePath::new("**/database.yml", "Database config", 7),
        // 설정 파일
        SensitivePath::new("~/.config/**", "User config", 5),
        SensitivePath::new("~/.gitconfig", "Git config", 4),
        SensitivePath::new("~/.netrc", "Network credentials", 8),
        SensitivePath::new("~/.gnupg/**", "GPG keys", 10),
        // 히스토리 (개인정보)
        SensitivePath::new("~/.bash_history", "Bash history", 6),
        SensitivePath::new("~/.zsh_history", "Zsh history", 6),
        SensitivePath::new("~/.node_repl_history", "Node REPL history", 5),
        SensitivePath::new("~/.python_history", "Python history", 5),
        // 브라우저/앱 데이터
        SensitivePath::new("~/.mozilla/**", "Firefox data", 7),
        SensitivePath::new("~/.chrome/**", "Chrome data", 7),
        SensitivePath::new("~/Library/Application Support/**", "macOS app data", 6),
        // 시스템 파일 (Linux/macOS)
        SensitivePath::new("/etc/passwd", "System users", 8),
        SensitivePath::new("/etc/shadow", "Password hashes", 10),
        SensitivePath::new("/etc/sudoers", "Sudo config", 9),
        SensitivePath::new("/etc/hosts", "Hosts file", 5),
    ]
}

/// 민감 경로 정의
#[derive(Debug, Clone)]
pub struct SensitivePath {
    pub pattern: String,
    pub description: String,
    pub risk_level: u8,
}

impl SensitivePath {
    pub fn new(pattern: impl Into<String>, description: impl Into<String>, risk_level: u8) -> Self {
        Self {
            pattern: pattern.into(),
            description: description.into(),
            risk_level: risk_level.min(10),
        }
    }

    /// 경로가 이 패턴에 매칭되는지 확인
    pub fn matches(&self, path: &str) -> bool {
        let pattern = &self.pattern;

        // ~ 를 홈 디렉토리로 확장
        let expanded_pattern = if pattern.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                pattern.replacen("~", &home.display().to_string(), 1)
            } else {
                pattern.clone()
            }
        } else {
            pattern.clone()
        };

        // glob 매칭
        glob::Pattern::new(&expanded_pattern)
            .map(|p| p.matches(path))
            .unwrap_or(false)
    }
}

// ============================================================
// 명령어 분석기
// ============================================================

/// 명령어 분석 결과
#[derive(Debug, Clone)]
pub struct CommandAnalysis {
    pub command: String,
    pub risk: CommandRisk,
    pub risk_score: u8,
    pub matched_pattern: Option<String>,
    pub reason: Option<String>,
}

/// 명령어 분석기 (캐시된 패턴 사용)
pub struct CommandAnalyzer {
    forbidden: Vec<ForbiddenPattern>,
    dangerous: Vec<String>,
    caution: Vec<String>,
    safe: Vec<String>,
    interactive: Vec<String>,
}

static ANALYZER: OnceLock<CommandAnalyzer> = OnceLock::new();

/// 전역 분석기 접근
pub fn analyzer() -> &'static CommandAnalyzer {
    ANALYZER.get_or_init(CommandAnalyzer::new)
}

impl CommandAnalyzer {
    pub fn new() -> Self {
        Self {
            forbidden: forbidden_patterns(),
            dangerous: dangerous_commands().into_iter().map(String::from).collect(),
            caution: caution_commands().into_iter().map(String::from).collect(),
            safe: safe_commands().into_iter().map(String::from).collect(),
            interactive: interactive_commands()
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }

    /// 명령어 분석
    pub fn analyze(&self, command: &str) -> CommandAnalysis {
        let command = command.trim();
        let first_word = command.split_whitespace().next().unwrap_or("");

        // 1. 금지 패턴 확인
        for pattern in &self.forbidden {
            if pattern.matches(command) {
                return CommandAnalysis {
                    command: command.to_string(),
                    risk: CommandRisk::Forbidden,
                    risk_score: 10,
                    matched_pattern: Some(format!("{:?}", pattern.pattern)),
                    reason: Some(pattern.reason.clone()),
                };
            }
        }

        // 2. 대화형 명령어 확인
        if self.interactive.iter().any(|c| first_word == c) {
            return CommandAnalysis {
                command: command.to_string(),
                risk: CommandRisk::Interactive,
                risk_score: 4,
                matched_pattern: Some(first_word.to_string()),
                reason: Some("Interactive command".to_string()),
            };
        }

        // 3. 위험 명령어 확인
        for dangerous in &self.dangerous {
            if command.starts_with(dangerous) || first_word == dangerous {
                return CommandAnalysis {
                    command: command.to_string(),
                    risk: CommandRisk::Dangerous,
                    risk_score: 7,
                    matched_pattern: Some(dangerous.clone()),
                    reason: Some("Potentially destructive command".to_string()),
                };
            }
        }

        // 4. 주의 명령어 확인
        for caution in &self.caution {
            if command.starts_with(caution) || first_word == caution {
                return CommandAnalysis {
                    command: command.to_string(),
                    risk: CommandRisk::Caution,
                    risk_score: 3,
                    matched_pattern: Some(caution.clone()),
                    reason: Some("Command requires caution".to_string()),
                };
            }
        }

        // 5. 안전 명령어 확인
        for safe in &self.safe {
            if command.starts_with(safe) || first_word == safe {
                return CommandAnalysis {
                    command: command.to_string(),
                    risk: CommandRisk::Safe,
                    risk_score: 0,
                    matched_pattern: Some(safe.clone()),
                    reason: None,
                };
            }
        }

        // 6. 알 수 없음
        CommandAnalysis {
            command: command.to_string(),
            risk: CommandRisk::Unknown,
            risk_score: 5,
            matched_pattern: None,
            reason: Some("Unknown command - requires confirmation".to_string()),
        }
    }

    /// 금지 명령어인지 확인
    pub fn is_forbidden(&self, command: &str) -> bool {
        self.forbidden.iter().any(|p| p.matches(command))
    }

    /// 안전 명령어인지 확인
    pub fn is_safe(&self, command: &str) -> bool {
        let first_word = command.split_whitespace().next().unwrap_or("");
        self.safe
            .iter()
            .any(|c| command.starts_with(c) || first_word == c)
    }
}

impl Default for CommandAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// 경로 분석기
pub struct PathAnalyzer {
    patterns: Vec<SensitivePath>,
}

static PATH_ANALYZER: OnceLock<PathAnalyzer> = OnceLock::new();

/// 전역 경로 분석기 접근
pub fn path_analyzer() -> &'static PathAnalyzer {
    PATH_ANALYZER.get_or_init(PathAnalyzer::new)
}

impl PathAnalyzer {
    pub fn new() -> Self {
        Self {
            patterns: sensitive_file_patterns(),
        }
    }

    /// 경로 분석
    pub fn analyze(&self, path: &str) -> Option<&SensitivePath> {
        self.patterns.iter().find(|p| p.matches(path))
    }

    /// 민감 경로인지 확인
    pub fn is_sensitive(&self, path: &str) -> bool {
        self.patterns.iter().any(|p| p.matches(path))
    }

    /// 민감도 점수 반환
    pub fn sensitivity_score(&self, path: &str) -> u8 {
        self.patterns
            .iter()
            .filter(|p| p.matches(path))
            .map(|p| p.risk_level)
            .max()
            .unwrap_or(0)
    }
}

impl Default for PathAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forbidden_detection() {
        let analyzer = CommandAnalyzer::new();

        assert!(analyzer.is_forbidden("rm -rf /"));
        assert!(analyzer.is_forbidden("rm -rf /*"));
        assert!(analyzer.is_forbidden(":(){ :|:& };:"));
        assert!(analyzer.is_forbidden("shutdown"));

        assert!(!analyzer.is_forbidden("rm file.txt"));
        assert!(!analyzer.is_forbidden("ls -la"));
    }

    #[test]
    fn test_safe_detection() {
        let analyzer = CommandAnalyzer::new();

        assert!(analyzer.is_safe("ls -la"));
        assert!(analyzer.is_safe("pwd"));
        assert!(analyzer.is_safe("git status"));
        assert!(analyzer.is_safe("cat file.txt"));

        assert!(!analyzer.is_safe("rm file.txt"));
        assert!(!analyzer.is_safe("unknown_command"));
    }

    #[test]
    fn test_command_analysis() {
        let analyzer = CommandAnalyzer::new();

        let result = analyzer.analyze("ls -la");
        assert_eq!(result.risk, CommandRisk::Safe);
        assert_eq!(result.risk_score, 0);

        let result = analyzer.analyze("rm -rf /");
        assert_eq!(result.risk, CommandRisk::Forbidden);
        assert_eq!(result.risk_score, 10);

        let result = analyzer.analyze("rm file.txt");
        assert_eq!(result.risk, CommandRisk::Dangerous);

        let result = analyzer.analyze("vim file.txt");
        assert_eq!(result.risk, CommandRisk::Interactive);
    }

    #[test]
    fn test_path_sensitivity() {
        let analyzer = PathAnalyzer::new();

        assert!(analyzer.is_sensitive("/home/user/.env"));
        assert!(analyzer.is_sensitive("/home/user/.ssh/id_rsa"));

        // 일반 파일은 민감하지 않음
        assert!(!analyzer.is_sensitive("/home/user/code/main.rs"));
    }
}
