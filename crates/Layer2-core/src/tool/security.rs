//! Tool Security - 보안 유틸리티
//!
//! 도구 실행 시 보안 검증을 위한 유틸리티 제공
//!
//! ## 기능
//! - 경로 검증 (path traversal 방지)
//! - 허용 경로 제한
//! - 위험 경로 차단

use std::path::{Path, PathBuf};

/// 경로 검증 결과
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathValidation {
    /// 경로가 유효함
    Valid,

    /// 경로가 허용 범위 밖
    OutsideAllowedRoots { path: PathBuf, allowed: Vec<PathBuf> },

    /// 위험한 경로
    DangerousPath { path: PathBuf, reason: String },

    /// 경로 탈출 시도
    PathTraversal { path: PathBuf, resolved: PathBuf },

    /// 심볼릭 링크 탈출
    SymlinkEscape { path: PathBuf, target: PathBuf },
}

impl PathValidation {
    pub fn is_valid(&self) -> bool {
        matches!(self, PathValidation::Valid)
    }

    pub fn error_message(&self) -> Option<String> {
        match self {
            PathValidation::Valid => None,
            PathValidation::OutsideAllowedRoots { path, allowed } => Some(format!(
                "Path '{}' is outside allowed directories: {:?}",
                path.display(),
                allowed.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
            )),
            PathValidation::DangerousPath { path, reason } => {
                Some(format!("Dangerous path '{}': {}", path.display(), reason))
            }
            PathValidation::PathTraversal { path, resolved } => Some(format!(
                "Path traversal detected: '{}' resolves to '{}'",
                path.display(),
                resolved.display()
            )),
            PathValidation::SymlinkEscape { path, target } => Some(format!(
                "Symlink escape detected: '{}' points to '{}'",
                path.display(),
                target.display()
            )),
        }
    }
}

/// 경로 검증기
pub struct PathValidator {
    /// 허용된 루트 경로들
    allowed_roots: Vec<PathBuf>,

    /// 위험한 경로 패턴
    dangerous_patterns: Vec<String>,

    /// 심볼릭 링크 검사 여부
    check_symlinks: bool,

    /// path traversal 검사 여부
    check_traversal: bool,
}

impl PathValidator {
    /// 새 검증기 생성
    pub fn new() -> Self {
        Self {
            allowed_roots: Vec::new(),
            dangerous_patterns: default_dangerous_patterns(),
            check_symlinks: true,
            check_traversal: true,
        }
    }

    /// 허용 루트 추가
    pub fn with_allowed_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.allowed_roots.push(root.into());
        self
    }

    /// 여러 허용 루트 추가
    pub fn with_allowed_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.allowed_roots.extend(roots);
        self
    }

    /// 위험 패턴 추가
    pub fn with_dangerous_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.dangerous_patterns.push(pattern.into());
        self
    }

    /// 심볼릭 링크 검사 비활성화
    pub fn disable_symlink_check(mut self) -> Self {
        self.check_symlinks = false;
        self
    }

    /// path traversal 검사 비활성화
    pub fn disable_traversal_check(mut self) -> Self {
        self.check_traversal = false;
        self
    }

    /// 경로 검증
    pub fn validate(&self, path: &Path) -> PathValidation {
        // 1. 절대 경로로 변환
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(path),
                Err(_) => return PathValidation::DangerousPath {
                    path: path.to_path_buf(),
                    reason: "Cannot resolve relative path".to_string(),
                },
            }
        };

        // 2. 정규화된 경로 얻기 (canonicalize 없이 수동 정규화)
        let normalized = normalize_path(&path);

        // 3. path traversal 검사 (.. 포함 여부)
        if self.check_traversal {
            // 원본 경로에 .. 이 있고 정규화 후 다른 위치로 가면 traversal
            let path_str = path.to_string_lossy();
            if path_str.contains("..") {
                // 실제 경로가 존재하면 canonicalize로 확인
                if let Ok(resolved) = path.canonicalize() {
                    if !normalized.starts_with(&resolved) && !resolved.starts_with(&normalized) {
                        return PathValidation::PathTraversal {
                            path: path.clone(),
                            resolved,
                        };
                    }
                }
            }
        }

        // 4. 위험 경로 패턴 검사
        let path_str = normalized.to_string_lossy().to_lowercase();
        for pattern in &self.dangerous_patterns {
            if path_str.contains(&pattern.to_lowercase()) {
                return PathValidation::DangerousPath {
                    path: normalized.clone(),
                    reason: format!("Contains dangerous pattern: {}", pattern),
                };
            }
        }

        // 5. 심볼릭 링크 검사
        if self.check_symlinks && normalized.exists() {
            if let Ok(metadata) = normalized.symlink_metadata() {
                if metadata.file_type().is_symlink() {
                    if let Ok(target) = std::fs::read_link(&normalized) {
                        let resolved_target = if target.is_absolute() {
                            target.clone()
                        } else {
                            normalized.parent().unwrap_or(Path::new("/")).join(&target)
                        };

                        // 심볼릭 링크 타겟이 허용 범위를 벗어나는지 확인
                        if !self.allowed_roots.is_empty() {
                            let target_in_allowed = self.allowed_roots.iter().any(|root| {
                                resolved_target.starts_with(root)
                            });

                            if !target_in_allowed {
                                return PathValidation::SymlinkEscape {
                                    path: normalized,
                                    target: resolved_target,
                                };
                            }
                        }
                    }
                }
            }
        }

        // 6. 허용 루트 검사 (설정된 경우만)
        if !self.allowed_roots.is_empty() {
            let in_allowed = self.allowed_roots.iter().any(|root| {
                normalized.starts_with(root)
            });

            if !in_allowed {
                return PathValidation::OutsideAllowedRoots {
                    path: normalized,
                    allowed: self.allowed_roots.clone(),
                };
            }
        }

        PathValidation::Valid
    }

    /// 빠른 검증 (성공 여부만)
    pub fn is_valid(&self, path: &Path) -> bool {
        self.validate(path).is_valid()
    }
}

impl Default for PathValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// 기본 위험 경로 패턴
fn default_dangerous_patterns() -> Vec<String> {
    let mut patterns = vec![
        // SSH/보안 (cross-platform, use both / and \)
        ".ssh".to_string(),
        ".gnupg".to_string(),

        // 클라우드 자격증명
        ".aws".to_string(),
        ".kube".to_string(),
        ".azure".to_string(),
    ];

    // Unix-specific patterns
    #[cfg(not(windows))]
    {
        patterns.extend(vec![
            "/etc/passwd".to_string(),
            "/etc/shadow".to_string(),
            "/etc/sudoers".to_string(),
            "/proc/".to_string(),
            "/sys/".to_string(),
            "/dev/".to_string(),
        ]);
    }

    // Windows-specific patterns
    #[cfg(windows)]
    {
        patterns.extend(vec![
            "\\windows\\system32".to_string(),
            "\\windows\\syswow64".to_string(),
            "\\system32\\".to_string(),
            "\\syswow64\\".to_string(),
        ]);
    }

    patterns
}

/// 경로 정규화 (canonicalize 없이)
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                // ".." 이면 마지막 컴포넌트 제거 (루트가 아닌 경우)
                if !components.is_empty() {
                    components.pop();
                }
            }
            std::path::Component::CurDir => {
                // "." 무시
            }
            _ => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

/// 민감한 경로인지 빠른 확인
pub fn is_sensitive_path(path: &str) -> bool {
    let sensitive_patterns = [
        ".env",
        ".ssh",
        "credentials",
        "secrets",
        ".pem",
        ".key",
        "_rsa",
        "_ed25519",
        ".aws",
        ".config/gcloud",
        ".kube",
        "token",
        "password",
        "api_key",
        "apikey",
    ];

    let path_lower = path.to_lowercase();
    sensitive_patterns.iter().any(|p| path_lower.contains(p))
}

/// 안전한 파일 확장자인지 확인
pub fn is_safe_extension(path: &Path) -> bool {
    let safe_extensions = [
        // 소스 코드
        "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "c", "cpp", "h", "hpp",
        "cs", "rb", "php", "swift", "kt", "scala", "clj", "ex", "exs", "elm", "hs",

        // 마크업/설정
        "html", "css", "scss", "sass", "less", "xml", "json", "yaml", "yml", "toml",
        "ini", "cfg", "conf",

        // 문서
        "md", "txt", "rst", "adoc",

        // 쉘
        "sh", "bash", "zsh", "fish", "ps1", "bat", "cmd",

        // 기타
        "sql", "graphql", "proto", "lock",
    ];

    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| safe_extensions.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from("C:\\Users\\test\\projects")
        } else {
            PathBuf::from("/home/user/projects")
        }
    }

    fn test_nested_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from("C:\\Users\\test\\projects\\myapp\\src\\main.rs")
        } else {
            PathBuf::from("/home/user/projects/myapp/src/main.rs")
        }
    }

    fn test_outside_path() -> PathBuf {
        if cfg!(windows) {
            // Use a path that's outside allowed root but not dangerous
            PathBuf::from("D:\\OtherDrive\\files\\document.txt")
        } else {
            PathBuf::from("/var/log/test.log")
        }
    }

    fn test_dangerous_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from("C:\\Users\\test\\.ssh\\id_rsa")
        } else {
            PathBuf::from("/home/user/.ssh/id_rsa")
        }
    }

    #[test]
    fn test_path_validator_valid() {
        let validator = PathValidator::new()
            .with_allowed_root(test_root());

        let result = validator.validate(&test_nested_path());
        assert!(result.is_valid());
    }

    #[test]
    fn test_path_validator_outside_allowed() {
        let validator = PathValidator::new()
            .with_allowed_root(test_root());

        let result = validator.validate(&test_outside_path());
        assert!(!result.is_valid());
        assert!(matches!(result, PathValidation::OutsideAllowedRoots { .. }));
    }

    #[test]
    fn test_path_validator_dangerous() {
        let validator = PathValidator::new();

        let result = validator.validate(&test_dangerous_path());
        assert!(!result.is_valid());
        assert!(matches!(result, PathValidation::DangerousPath { .. }));
    }

    #[test]
    fn test_path_normalization() {
        // Skip on Windows as path normalization differs
        #[cfg(not(windows))]
        {
            let path = Path::new("/home/user/projects/../.ssh/id_rsa");
            let normalized = normalize_path(path);
            assert_eq!(normalized, PathBuf::from("/home/user/.ssh/id_rsa"));
        }

        #[cfg(windows)]
        {
            let path = Path::new("C:\\Users\\test\\projects\\..\\docs\\file.txt");
            let normalized = normalize_path(path);
            assert_eq!(normalized, PathBuf::from("C:\\Users\\test\\docs\\file.txt"));
        }
    }

    #[test]
    fn test_is_sensitive_path() {
        // Cross-platform sensitive patterns
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path("secrets/api.key"));
        assert!(is_sensitive_path(".ssh/id_rsa"));
        assert!(!is_sensitive_path("projects/main.rs"));
    }

    #[test]
    fn test_is_safe_extension() {
        assert!(is_safe_extension(Path::new("main.rs")));
        assert!(is_safe_extension(Path::new("config.json")));
        assert!(is_safe_extension(Path::new("readme.md")));
        assert!(!is_safe_extension(Path::new("program.exe")));
        assert!(!is_safe_extension(Path::new("library.dll")));
    }

    #[test]
    fn test_validation_error_message() {
        let result = PathValidation::DangerousPath {
            path: PathBuf::from("/etc/passwd"),
            reason: "System file".to_string(),
        };
        assert!(result.error_message().unwrap().contains("Dangerous"));
    }
}
