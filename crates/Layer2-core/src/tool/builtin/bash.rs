//! Bash Tool - Shell 명령 실행 도구
//!
//! Shell 명령을 실행합니다.
//! - Layer1 CommandAnalyzer로 위험도 분석
//! - 금지 명령어 자동 차단
//! - 타임아웃 지원
//! - 작업 디렉토리 유지

use async_trait::async_trait;
use forge_foundation::{
    command_analyzer, CommandRisk, PermissionAction, PermissionDef, PermissionStatus, Result,
    Tool, ToolContext, ToolMeta, ToolResult,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

/// Bash 도구 입력
#[derive(Debug, Deserialize)]
pub struct BashInput {
    /// 실행할 명령어
    pub command: String,

    /// 타임아웃 (밀리초, 기본: 120000 = 2분, 최대: 600000 = 10분)
    #[serde(default)]
    pub timeout: Option<u64>,

    /// 명령어 설명 (UI 표시용)
    #[serde(default)]
    pub description: Option<String>,
}

/// Bash 도구
pub struct BashTool;

impl BashTool {
    /// 새 인스턴스 생성
    pub fn new() -> Self {
        Self
    }

    /// 도구 이름
    pub const NAME: &'static str = "bash";

    /// 기본 타임아웃 (2분)
    const DEFAULT_TIMEOUT_MS: u64 = 120_000;

    /// 최대 타임아웃 (10분)
    const MAX_TIMEOUT_MS: u64 = 600_000;

    /// 최대 출력 크기 (30KB)
    const MAX_OUTPUT_SIZE: usize = 30_000;

    /// Windows 호환성을 위한 명령어 정규화
    /// - `&&` → `;` (PowerShell)
    /// - Unix 명령어 → PowerShell 대체
    fn normalize_command(command: &str, shell_config: &dyn forge_foundation::ShellConfig) -> String {
        use forge_foundation::ShellType;
        
        let shell_type = shell_config.shell_type();
        
        // PowerShell에서만 변환
        if !matches!(shell_type, ShellType::PowerShell) {
            return command.to_string();
        }
        
        let mut cmd = command.to_string();
        
        // && → ; (PowerShell 명령어 구분자)
        cmd = cmd.replace(" && ", " ; ");
        
        // 일반적인 Unix 명령어를 PowerShell 대체로 변환
        // 주의: 정확한 패턴 매칭 필요 (단어 경계)
        let replacements = [
            // 파일 작업
            ("cat ", "Get-Content "),
            ("ls ", "Get-ChildItem "),
            ("ls\n", "Get-ChildItem\n"),
            ("rm -rf ", "Remove-Item -Recurse -Force "),
            ("rm -r ", "Remove-Item -Recurse "),
            ("rm ", "Remove-Item "),
            ("cp ", "Copy-Item "),
            ("mv ", "Move-Item "),
            ("mkdir -p ", "New-Item -ItemType Directory -Force -Path "),
            ("mkdir ", "New-Item -ItemType Directory -Path "),
            ("touch ", "New-Item -ItemType File -Path "),
            // 텍스트 처리
            ("grep ", "Select-String -Pattern "),
            ("head -n ", "Select-Object -First "),
            ("tail -n ", "Select-Object -Last "),
            ("wc -l", "Measure-Object -Line"),
            // 기타
            ("pwd", "(Get-Location).Path"),
            ("echo ", "Write-Output "),
            ("which ", "Get-Command "),
            ("find ", "Get-ChildItem -Recurse -Filter "),
        ];
        
        for (unix, ps) in &replacements {
            // 명령어 시작 부분이나 ; 뒤에서만 변환
            if cmd.starts_with(unix) {
                cmd = format!("{}{}", ps, &cmd[unix.len()..]);
            }
            cmd = cmd.replace(&format!("; {}", unix), &format!("; {}", ps));
        }
        
        cmd
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new(Self::NAME)
            .display_name("Bash")
            .description("Execute shell commands directly. PREFERRED for: ls, cat, cargo build/test/--version, git commands, npm install, quick scripts. \
                         For long-running servers or watch processes that need background execution, use 'task_spawn' instead.")
            .category("execute")
            .permission(
                PermissionDef::new("bash.execute", "execute")
                    .risk_level(8)
                    .description("Execute shell command")
                    .requires_confirmation(true),
            )
            .permission(
                PermissionDef::new("bash.execute.safe", "execute")
                    .risk_level(2)
                    .description("Execute safe shell command (ls, pwd, etc.)")
                    .requires_confirmation(false),
            )
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 600000)"
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does"
                }
            },
            "required": ["command"]
        })
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let command = input.get("command")?.as_str()?;

        // CommandAnalyzer로 분석
        let analysis = command_analyzer().analyze(command);

        // 안전한 명령어는 권한 필요 없음
        if analysis.risk.can_auto_approve() {
            return None;
        }

        Some(PermissionAction::Execute {
            command: command.to_string(),
        })
    }

    async fn execute(&self, input: Value, context: &dyn ToolContext) -> Result<ToolResult> {
        // 입력 파싱
        let parsed: BashInput = serde_json::from_value(input.clone()).map_err(|e| {
            forge_foundation::Error::InvalidInput(format!("Invalid input: {}", e))
        })?;

        // 빈 명령어 체크
        if parsed.command.trim().is_empty() {
            return Ok(ToolResult::error("Command cannot be empty"));
        }

        // Windows 호환성: 명령어 자동 변환
        let command = Self::normalize_command(&parsed.command, context.shell_config());

        // CommandAnalyzer로 위험도 분석
        let analysis = command_analyzer().analyze(&command);

        // 금지 명령어 차단
        if analysis.risk == CommandRisk::Forbidden {
            return Ok(ToolResult::error(format!(
                "Command blocked: {}. Reason: {}",
                parsed.command,
                analysis.reason.unwrap_or_else(|| "Forbidden command".to_string())
            )));
        }

        // 대화형 명령어 경고
        if analysis.risk == CommandRisk::Interactive {
            return Ok(ToolResult::error(format!(
                "Interactive commands are not supported: {}. Use non-interactive alternatives.",
                parsed.command
            )));
        }

        // 권한 확인 (안전하지 않은 명령어만)
        if !analysis.risk.can_auto_approve() {
            if let Some(action) = self.required_permission(&input) {
                let status = context.check_permission(Self::NAME, &action).await;
                match status {
                    PermissionStatus::Denied => {
                        return Ok(ToolResult::error("Permission denied for command execution"));
                    }
                    PermissionStatus::Unknown => {
                        let desc = parsed.description.as_deref().unwrap_or(&parsed.command);
                        let granted = context
                            .request_permission(
                                Self::NAME,
                                &format!("Execute: {}", desc),
                                action,
                            )
                            .await?;
                        if !granted {
                            return Ok(ToolResult::error("Permission denied by user"));
                        }
                    }
                    _ => {}
                }
            }
        }

        // 타임아웃 설정
        let timeout_ms = parsed
            .timeout
            .unwrap_or(Self::DEFAULT_TIMEOUT_MS)
            .min(Self::MAX_TIMEOUT_MS);

        // Shell 설정 가져오기
        let shell_config = context.shell_config();
        let shell_exe = shell_config.executable();
        let shell_args = shell_config.exec_args();

        // 명령어 실행 (정규화된 명령어 사용)
        let mut cmd = Command::new(shell_exe);
        for arg in &shell_args {
            cmd.arg(arg);
        }
        cmd.arg(&command);

        // 작업 디렉토리 설정
        cmd.current_dir(context.working_dir());

        // 환경 변수 설정
        for (key, value) in context.env() {
            cmd.env(key, value);
        }

        // Shell 환경 변수 추가
        for (key, value) in shell_config.env_vars() {
            cmd.env(key, value);
        }

        // stdout/stderr 캡처
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // 프로세스 시작
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult::error(format!("Failed to spawn process: {}", e)));
            }
        };

        // 타임아웃과 함께 실행
        let result = timeout(Duration::from_millis(timeout_ms), async {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();

            // stdout 읽기
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_end(&mut stdout_buf).await;
            }

            // stderr 읽기
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_end(&mut stderr_buf).await;
            }

            // 프로세스 종료 대기
            let status = child.wait().await;

            (status, stdout_buf, stderr_buf)
        })
        .await;

        match result {
            Ok((status_result, stdout_buf, stderr_buf)) => {
                let status = match status_result {
                    Ok(s) => s,
                    Err(e) => {
                        return Ok(ToolResult::error(format!("Process error: {}", e)));
                    }
                };

                // 출력 변환
                let stdout = String::from_utf8_lossy(&stdout_buf);
                let stderr = String::from_utf8_lossy(&stderr_buf);

                // 출력 조합
                let mut output = String::new();

                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }

                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[stderr]\n");
                    output.push_str(&stderr);
                }

                // 출력 크기 제한
                if output.len() > Self::MAX_OUTPUT_SIZE {
                    output.truncate(Self::MAX_OUTPUT_SIZE);
                    output.push_str("\n... [output truncated]");
                }

                // 종료 코드 확인
                let exit_code = status.code().unwrap_or(-1);

                if status.success() {
                    if output.is_empty() {
                        Ok(ToolResult::success("[Command completed successfully with no output]"))
                    } else {
                        Ok(ToolResult::success(output))
                    }
                } else {
                    if output.is_empty() {
                        Ok(ToolResult::error(format!(
                            "Command failed with exit code {}",
                            exit_code
                        )))
                    } else {
                        Ok(ToolResult::error(format!(
                            "Exit code {}\n{}",
                            exit_code, output
                        )))
                    }
                }
            }
            Err(_) => {
                // 타임아웃 - 프로세스 강제 종료
                let _ = child.kill().await;
                Ok(ToolResult::error(format!(
                    "Command timed out after {} ms",
                    timeout_ms
                )))
            }
        }
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta() {
        let tool = BashTool::new();
        let meta = tool.meta();
        assert_eq!(meta.name, "bash");
        assert_eq!(meta.category, "execute");
    }

    #[test]
    fn test_schema() {
        let tool = BashTool::new();
        let schema = tool.schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["command"].is_object());
    }

    #[test]
    fn test_safe_command_no_permission() {
        let tool = BashTool::new();
        let input = json!({ "command": "ls -la" });
        // ls는 안전한 명령어이므로 권한 필요 없음
        assert!(tool.required_permission(&input).is_none());
    }

    #[test]
    fn test_dangerous_command_needs_permission() {
        let tool = BashTool::new();
        let input = json!({ "command": "rm file.txt" });
        // rm은 위험한 명령어이므로 권한 필요
        assert!(tool.required_permission(&input).is_some());
    }

    #[test]
    fn test_forbidden_command_analysis() {
        let analysis = command_analyzer().analyze("rm -rf /");
        assert_eq!(analysis.risk, CommandRisk::Forbidden);
    }

    #[test]
    fn test_interactive_command_analysis() {
        let analysis = command_analyzer().analyze("vim file.txt");
        assert_eq!(analysis.risk, CommandRisk::Interactive);
    }
}
