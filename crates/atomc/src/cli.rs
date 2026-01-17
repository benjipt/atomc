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
    #[arg(long, value_enum)]
    pub diff_mode: Option<DiffMode>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub include_untracked: bool,
    #[arg(long = "no-include-untracked", action = ArgAction::SetTrue, conflicts_with = "include_untracked")]
    pub no_include_untracked: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, action = ArgAction::SetTrue)]
    pub log_diff: bool,
    #[arg(long = "no-log-diff", action = ArgAction::SetTrue, conflicts_with = "log_diff")]
    pub no_log_diff: bool,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl PlanArgs {
    pub fn include_untracked_override(&self) -> Option<bool> {
        if self.no_include_untracked {
            Some(false)
        } else if self.include_untracked {
            Some(true)
        } else {
            None
        }
    }

    pub fn log_diff_override(&self) -> Option<bool> {
        if self.no_log_diff {
            Some(false)
        } else if self.log_diff {
            Some(true)
        } else {
            None
        }
    }
}

#[derive(Args, Debug)]
pub struct ApplyArgs {
    #[arg(long)]
    pub repo: PathBuf,
    #[arg(long = "diff-file")]
    pub diff_file: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub diff_mode: Option<DiffMode>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub include_untracked: bool,
    #[arg(long = "no-include-untracked", action = ArgAction::SetTrue, conflicts_with = "include_untracked")]
    pub no_include_untracked: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, action = ArgAction::SetTrue)]
    pub log_diff: bool,
    #[arg(long = "no-log-diff", action = ArgAction::SetTrue, conflicts_with = "log_diff")]
    pub no_log_diff: bool,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub execute: bool,
    #[arg(long)]
    pub cleanup_on_error: bool,
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl ApplyArgs {
    pub fn include_untracked_override(&self) -> Option<bool> {
        if self.no_include_untracked {
            Some(false)
        } else if self.include_untracked {
            Some(true)
        } else {
            None
        }
    }

    pub fn log_diff_override(&self) -> Option<bool> {
        if self.no_log_diff {
            Some(false)
        } else if self.log_diff {
            Some(true)
        } else {
            None
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
    #[arg(long, action = ArgAction::SetTrue)]
    pub log_diff: bool,
    #[arg(long = "no-log-diff", action = ArgAction::SetTrue, conflicts_with = "log_diff")]
    pub no_log_diff: bool,
}

impl ServeArgs {
    pub fn log_diff_override(&self) -> Option<bool> {
        if self.no_log_diff {
            Some(false)
        } else if self.log_diff {
            Some(true)
        } else {
            None
        }
    }
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
