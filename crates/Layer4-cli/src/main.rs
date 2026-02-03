//! ForgeCode CLI - Main entry point

mod cli;
mod tui;

use clap::Parser;
use forge_foundation::ProviderConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// ForgeCode - AI-powered coding assistant for the terminal
#[derive(Parser, Debug)]
#[command(name = "forge")]
#[command(author, version, about, long_about = None)]
struct Args {
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

    /// Provider to use (anthropic, openai, ollama)
    #[arg(long)]
    provider: Option<String>,

    /// Model to use
    #[arg(long)]
    model: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    // Load configuration
    let config = ProviderConfig::load().unwrap_or_else(|e| {
        eprintln!("Warning: Failed to load config: {}", e);
        ProviderConfig::default()
    });

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
