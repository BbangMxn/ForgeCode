//! Non-interactive CLI mode

use forge_agent::{Agent, AgentContext, AgentEvent, MessageHistory};
use forge_foundation::{PermissionService, ProviderConfig, Result};
use forge_provider::Gateway;
use forge_tool::ToolRegistry;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Run a single prompt in non-interactive mode
pub async fn run_once(config: &ProviderConfig, prompt: &str) -> Result<()> {
    println!("ForgeCode - Processing...\n");

    // Initialize components
    let gateway = Arc::new(Gateway::from_config(config)?);
    let tools = Arc::new(ToolRegistry::with_builtins());
    let permissions = Arc::new(PermissionService::with_auto_approve());

    let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Create agent context
    let ctx = Arc::new(AgentContext::new(gateway, tools, permissions, working_dir));

    // Create agent
    let agent = Agent::new(ctx);

    // Create message history
    let mut history = MessageHistory::new();

    // Create event channel
    let (tx, mut rx) = mpsc::channel(100);

    // Spawn event handler
    let event_handle = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::Text(text) => {
                    print!("{}", text);
                }
                AgentEvent::ToolStart { tool_name, .. } => {
                    println!("\n[{}] Running...", tool_name);
                }
                AgentEvent::ToolComplete {
                    tool_name,
                    success,
                    result,
                    ..
                } => {
                    let status = if success { "✓" } else { "✗" };
                    println!("[{}] {} {}", tool_name, status, truncate(&result, 100));
                }
                AgentEvent::Done { .. } => {
                    println!();
                }
                AgentEvent::Error(e) => {
                    eprintln!("\nError: {}", e);
                }
                AgentEvent::Usage {
                    input_tokens,
                    output_tokens,
                } => {
                    println!("\n[Tokens: {} in, {} out]", input_tokens, output_tokens);
                }
                _ => {}
            }
        }
    });

    // Run agent
    let session_id = uuid::Uuid::new_v4().to_string();
    let _ = agent.run(&session_id, &mut history, prompt, tx).await;

    // Wait for event handler to finish
    let _ = event_handle.await;

    Ok(())
}

/// Truncate a string for display
fn truncate(s: &str, max_len: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len])
    }
}
