use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "atomc")]
#[command(about = "Local-first atomic commit planner and executor")]
#[command(version)]
pub struct Cli {
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,
    #[arg(long)]
    pub quiet: bool,
    #[arg(long)]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Plan(PlanArgs),
    Apply(ApplyArgs),
    Serve(ServeArgs),
}

#[derive(Args, Debug)]
pub struct PlanArgs {
    #[arg(long)]
    pub repo: Option<PathBuf>,
    #[arg(long = "diff-file")]
    pub diff_file: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = DiffMode::All)]
    pub diff_mode: DiffMode,
    #[arg(long, default_value_t = true)]
    pub include_untracked: bool,
    #[arg(long = "no-include-untracked", action = ArgAction::SetTrue, conflicts_with = "include_untracked")]
    pub no_include_untracked: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[allow(dead_code)] // Used once CLI wiring is implemented.
impl PlanArgs {
    pub fn resolved_include_untracked(&self) -> bool {
        if self.no_include_untracked {
            false
        } else {
            self.include_untracked
        }
    }
}

#[derive(Args, Debug)]
pub struct ApplyArgs {
    #[arg(long)]
    pub repo: PathBuf,
    #[arg(long = "diff-file")]
    pub diff_file: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = DiffMode::All)]
    pub diff_mode: DiffMode,
    #[arg(long, default_value_t = true)]
    pub include_untracked: bool,
    #[arg(long = "no-include-untracked", action = ArgAction::SetTrue, conflicts_with = "include_untracked")]
    pub no_include_untracked: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub execute: bool,
    #[arg(long)]
    pub cleanup_on_error: bool,
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[allow(dead_code)] // Used once CLI wiring is implemented.
impl ApplyArgs {
    pub fn resolved_include_untracked(&self) -> bool {
        if self.no_include_untracked {
            false
        } else {
            self.include_untracked
        }
    }
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(long, default_value_t = 49152)]
    pub port: u16,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long, value_enum, default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,
    #[arg(long, default_value_t = 60)]
    pub request_timeout: u64,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum DiffMode {
    Worktree,
    Staged,
    All,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum OutputFormat {
    Json,
    Human,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum LogFormat {
    Json,
    Text,
}
