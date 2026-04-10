// CLI 模块

use clap::{Parser, Subcommand};

pub mod commands;

#[derive(Parser)]
#[command(name = "mrouter")]
#[command(about = "Model Router - Terminal-based router for AI CLI tools", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Open TUI interface
    Tui,
    
    /// Daemon management
    Daemon(DaemonCommand),
    
    /// Switch to a provider
    Switch {
        /// Provider name
        provider: String,
    },
    
    /// List all providers
    List,
    
    /// Show current status
    Status,
    
    /// Run health check
    Health,
    
    /// Show usage statistics
    Stats {
        /// Export format (csv, json)
        #[arg(long)]
        export: Option<String>,
    },
    
    /// Proxy management
    Proxy(ProxyCommand),
}

#[derive(Parser)]
pub struct DaemonCommand {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start daemon
    Start {
        /// Enable auto-start on boot
        #[arg(long)]
        auto_start: bool,
    },
    /// Stop daemon
    Stop,
    /// Restart daemon
    Restart,
    /// Show daemon status
    Status,
    /// Show daemon logs
    Logs,
}

#[derive(Parser)]
pub struct ProxyCommand {
    #[command(subcommand)]
    pub action: ProxyAction,
}

#[derive(Subcommand)]
pub enum ProxyAction {
    /// Start proxy server
    Start,
    /// Stop proxy server
    Stop,
    /// Show proxy status
    Status,
    /// Show proxy logs
    Logs,
}

pub fn parse_args() -> Cli {
    Cli::parse()
}
