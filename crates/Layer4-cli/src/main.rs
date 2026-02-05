//! ForgeCode CLI - Main entry point

mod auto_config;
mod cli;
mod clipboard;
mod cost;
mod diff;
mod history;
mod hooks;
mod init;
mod markdown;
mod project;
mod session;
mod setup;
mod syntax;
mod tui;

// Re-exports
pub use clipboard::ClipboardManager;
pub use cost::CostTracker;
pub use diff::{DiffGenerator, DiffRenderer, StreamingDiff};
pub use history::HistoryManager;
pub use hooks::{HookManager, HookEvent, HookContext, HookResult};
pub use markdown::MarkdownRenderer;
pub use session::SessionManager;
pub use syntax::SyntaxHighlighter;

use clap::{Parser, Subcommand};
use forge_foundation::{provider_store, ProviderConfig, ProviderType};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// ForgeCode - AI-powered coding assistant for the terminal
#[derive(Parser, Debug)]
#[command(name = "forge")]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Run in non-interactive mode with a single prompt
    #[arg(short, long)]
    prompt: Option<String>,

    /// Continue from a specific session
    #[arg(short, long)]
    session: Option<String>,

    /// Use container mode for execution
    #[arg(long)]
    container: bool,

    /// Use local mode for execution
    #[arg(long)]
    local: bool,

    /// Provider to use (anthropic, openai, gemini, groq, ollama)
    #[arg(long)]
    provider: Option<String>,

    /// Model to use
    #[arg(long)]
    model: Option<String>,

    /// API key for the provider (overrides env and config)
    #[arg(long)]
    api_key: Option<String>,

    /// Base URL for the provider (for ollama or custom endpoints)
    #[arg(long)]
    base_url: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Skip auto-initialization check
    #[arg(long)]
    no_init: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize ForgeCode in the current directory
    Init {
        /// Force reinitialization even if already initialized
        #[arg(short, long)]
        force: bool,
    },
    /// List recent sessions
    Sessions {
        /// Number of sessions to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Continue the most recent session
    Continue,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Handle subcommands first
    if let Some(command) = args.command {
        match command {
            Command::Init { force } => {
                return init::init_project(force);
            }
            Command::Sessions { limit } => {
                return list_sessions_cmd(limit);
            }
            Command::Continue => {
                // Continue most recent session
                let data_dir = dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join(".forgecode");
                if let Ok(storage) = forge_foundation::Storage::new(&data_dir) {
                    if let Ok(sessions) = storage.get_sessions(Some(1)) {
                        if let Some(session) = sessions.first() {
                            println!("Continuing session: {}", &session.id[..8.min(session.id.len())]);
                            // TODO: Load session into TUI
                        } else {
                            println!("No recent sessions found. Start a new conversation.");
                        }
                    }
                }
                // Fall through to TUI
            }
        }
    }

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    // Check for initialization (unless skipped or prompt mode)
    if !args.no_init && args.prompt.is_none() {
        // 설정 파일이 없으면 설치 마법사 실행
        if setup::needs_setup() {
            println!("🔧 ForgeCode 첫 실행 - 설정이 필요합니다.\n");
            
            match setup::run_setup_wizard() {
                Ok(Some(config)) => {
                    if let Err(e) = setup::save_config(&config) {
                        eprintln!("설정 저장 실패: {}", e);
                    } else {
                        println!("\n✓ 설정 완료! ForgeCode를 시작합니다...\n");
                    }
                }
                Ok(None) => {
                    println!("\n설정이 취소되었습니다. 'forge init'으로 나중에 설정할 수 있습니다.");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("설정 마법사 오류: {}", e);
                    init::check_and_auto_init();
                }
            }
        } else {
            init::check_and_auto_init();
        }
    }

    // Load configuration
    let mut config = ProviderConfig::load().unwrap_or_else(|e| {
        eprintln!("Warning: Failed to load config: {}", e);
        ProviderConfig::default()
    });

    // Handle --provider option: set as default and ensure provider exists
    if let Some(provider_name) = &args.provider {
        let provider_type = match provider_name.as_str() {
            "anthropic" => ProviderType::Anthropic,
            "openai" => ProviderType::Openai,
            "gemini" => ProviderType::Gemini,
            "groq" => ProviderType::Groq,
            "ollama" => ProviderType::Ollama,
            _ => {
                eprintln!("Warning: Unknown provider '{}', using anthropic", provider_name);
                ProviderType::Anthropic
            }
        };

        // Create or update provider
        if !config.contains(provider_name) {
            let mut provider = provider_store::Provider::new(provider_type);

            // Apply model if specified
            if let Some(model) = &args.model {
                provider = provider.model(model.clone());
            }

            // Apply base_url if specified
            if let Some(base_url) = &args.base_url {
                provider = provider.base_url(base_url.clone());
            }

            // Apply api_key if specified
            if let Some(api_key) = &args.api_key {
                provider = provider.api_key(api_key.clone());
            }

            config.add(provider_name, provider);
        } else {
            // Update existing provider
            if let Some(provider) = config.get_mut(provider_name) {
                if let Some(model) = &args.model {
                    provider.model = Some(model.clone());
                }
                if let Some(base_url) = &args.base_url {
                    provider.base_url = Some(base_url.clone());
                }
                if let Some(api_key) = &args.api_key {
                    provider.api_key = Some(api_key.clone());
                }
            }
        }

        // Set as default provider
        config.set_default(provider_name);
        tracing::info!("Using provider: {}", provider_name);
    } else if let Some(api_key) = &args.api_key {
        // If only api_key is provided without --provider, apply to default (anthropic)
        config.set_api_key("anthropic", api_key);
        tracing::info!("Using API key from command line for provider: anthropic");
    }

    // Run based on mode
    if let Some(prompt) = args.prompt {
        // Non-interactive mode
        cli::run_once(&config, &prompt).await?;
    } else {
        // Interactive TUI mode
        tui::run(&config).await?;
    }

    Ok(())
}

/// List recent sessions
fn list_sessions_cmd(limit: usize) -> anyhow::Result<()> {
    let data_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".forgecode");
    let storage = forge_foundation::Storage::new(&data_dir)?;
    let sessions = storage.get_sessions(Some(limit as u32))?;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!("\n📋 Recent Sessions\n");
    println!("{:<10} {:<30} {:<15} {:<10} {:<8}", 
        "ID", "Title", "Model", "Messages", "Tokens");
    println!("{}", "-".repeat(80));

    for session in sessions {
        let id_short = if session.id.len() >= 8 { &session.id[..8] } else { &session.id };
        let title = session.title.clone().unwrap_or_else(|| "(untitled)".to_string());
        let title_display = if title.len() > 28 {
            format!("{}...", &title[..25])
        } else {
            title
        };
        let model = session.model.clone().unwrap_or_default();
        let model_display = if model.len() > 13 {
            format!("{}...", &model[..10])
        } else {
            model
        };
        let total_tokens = session.total_input_tokens + session.total_output_tokens;

        println!("{:<10} {:<30} {:<15} {:<10} {:<8}",
            id_short,
            title_display,
            model_display,
            session.message_count,
            total_tokens
        );
    }

    println!("\nUse 'forge continue' to resume the most recent session.");
    println!("Use 'forge --session <ID>' to resume a specific session.\n");

    Ok(())
}
