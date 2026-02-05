//! Non-interactive CLI mode
//!
//! 단일 프롬프트를 처리하는 비대화형 모드입니다.
//! Layer3 Agent의 새로운 이벤트 시스템을 완전히 지원합니다.

use forge_agent::{Agent, AgentConfig, AgentContext, AgentEvent, MessageHistory};
use forge_core::ToolRegistry;
use forge_foundation::{PermissionService, ProviderConfig, Result};
use forge_provider::Gateway;
use forge_task::TaskManager;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Run a single prompt in non-interactive mode
pub async fn run_once(config: &ProviderConfig, prompt: &str) -> Result<()> {
    // Print header
    eprintln!("ForgeCode - Processing...\n");

    // Initialize components
    let gateway = Arc::new(Gateway::from_config(config)?);
    let tools = Arc::new(ToolRegistry::with_builtins());
    let permissions = Arc::new(PermissionService::with_auto_approve());

    let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Create task manager for long-running commands (servers, PTY)
    let task_manager = Arc::new(TaskManager::new(forge_task::TaskManagerConfig::default()).await);

    // Create agent context with task manager
    let ctx = Arc::new(
        AgentContext::new(gateway, tools, permissions, working_dir.clone())
            .with_task_manager(task_manager)
    );

    // Create agent with config
    let agent = Agent::with_config(ctx, AgentConfig::default());

    // Create message history
    let mut history = MessageHistory::new();

    // Create event channel
    let (tx, mut rx) = mpsc::channel(100);

    // Spawn event handler
    let event_handle = tokio::spawn(async move {
        let mut stdout = io::stdout();
        let mut current_turn = 0u32;
        let mut total_input = 0u32;
        let mut total_output = 0u32;

        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::Text(text) => {
                    print!("{}", text);
                    let _ = stdout.flush();
                }
                AgentEvent::Thinking => {
                    eprint!("\r[Turn {}] Thinking...  ", current_turn + 1);
                    let _ = io::stderr().flush();
                }
                AgentEvent::TurnStart { turn } => {
                    current_turn = turn;
                }
                AgentEvent::TurnComplete { turn } => {
                    eprintln!("\r[Turn {}] Complete     ", turn);
                }
                AgentEvent::ToolStart { tool_name, .. } => {
                    eprint!("\r[{}] Running...    ", tool_name);
                    let _ = io::stderr().flush();
                }
                AgentEvent::ToolComplete {
                    tool_name,
                    success,
                    result,
                    duration_ms,
                    ..
                } => {
                    let status = if success { "✓" } else { "✗" };
                    eprintln!(
                        "\r[{}] {} {} ({}ms)",
                        tool_name,
                        status,
                        truncate(&result, 60),
                        duration_ms
                    );
                }
                AgentEvent::Compressed {
                    tokens_before,
                    tokens_after,
                    tokens_saved,
                } => {
                    eprintln!(
                        "[Context] Compressed: {} → {} tokens (saved {})",
                        tokens_before, tokens_after, tokens_saved
                    );
                }
                AgentEvent::Done { .. } => {
                    println!(); // Final newline
                    eprintln!(
                        "\n[Stats] {} turns, {} input tokens, {} output tokens",
                        current_turn, total_input, total_output
                    );
                }
                AgentEvent::Error(e) => {
                    eprintln!("\n[Error] {}", e);
                }
                AgentEvent::Usage {
                    input_tokens,
                    output_tokens,
                } => {
                    total_input += input_tokens;
                    total_output += output_tokens;
                }
                AgentEvent::Paused => {
                    eprintln!("[Agent] Paused");
                }
                AgentEvent::Resumed => {
                    eprintln!("[Agent] Resumed");
                }
                AgentEvent::Stopped { reason } => {
                    eprintln!("[Agent] Stopped: {}", reason);
                }
            }
        }
    });

    // Run agent
    let session_id = uuid::Uuid::new_v4().to_string();
    let result = agent.run(&session_id, &mut history, prompt, tx).await;

    // Wait for event handler to finish
    let _ = event_handle.await;

    // Handle result
    if let Err(e) = result {
        eprintln!("[Error] Agent failed: {}", e);
    }

    Ok(())
}

/// Truncate a string for display
fn truncate(s: &str, max_len: usize) -> String {
    let s = s.replace('\n', " ").replace('\r', "");
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
