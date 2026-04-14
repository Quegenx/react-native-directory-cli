mod api;
mod cache;
mod commands;
mod env;
mod output;
mod scanner;
mod stopwords;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "rnd",
    version,
    about = "React Native Directory CLI — query 2400+ RN packages",
    long_about = "Query reactnative.directory from the terminal. Built for humans and AI agents.\nAuto-emits JSON when running inside Claude Code or Codex."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,

    #[arg(long, global = true, help = "Emit JSON (auto-on inside CLAUDECODE/CODEX)")]
    json: bool,

    #[arg(long, global = true, help = "Force pretty output even when piped or in an agent")]
    pretty: bool,

    #[arg(long, global = true, help = "Bypass cache and re-fetch from API")]
    refresh: bool,

    #[arg(long, global = true, help = "Suppress non-essential output")]
    quiet: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum FailOn {
    Any,
    Unmaintained,
    None,
}

#[derive(Subcommand)]
enum Cmd {
    #[command(about = "Search the directory by name, description, or topic")]
    Search {
        query: String,
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
    },
    #[command(about = "Show detailed info on one package")]
    Info { name: String },
    #[command(about = "Suggest alternatives to a package")]
    Alternatives {
        name: String,
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
    #[command(about = "List packages sorted by weekly downloads (native-only by default)")]
    Trending {
        #[arg(short, long, default_value_t = 25)]
        limit: usize,
        #[arg(long, help = "Include pure-JS libraries (off by default to surface RN-specific packages)")]
        include_js: bool,
    },
    #[command(about = "Find well-rated, actively-maintained native packages")]
    Discover {
        #[arg(short, long, default_value_t = 25)]
        limit: usize,
        #[arg(long, default_value_t = 70.0, help = "Minimum directory score (0-100)")]
        min_score: f64,
        #[arg(long, default_value_t = 90, help = "Only packages pushed within N days")]
        days: u32,
    },
    #[command(about = "Filter packages by category/platform/module-type/compat flags")]
    List {
        #[arg(long, help = "Topic keyword e.g. navigation, storage, camera")]
        category: Option<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Platforms (comma-separated) — must ALL be supported: ios,android,web,macos,tvos,visionos,windows"
        )]
        platform: Vec<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Compat targets (comma-separated) — must ALL be compatible: expo-go,fireos,horizon,vegaos"
        )]
        works_with: Vec<String>,
        #[arg(long, help = "Module type: expo, nitro, turbo")]
        module_type: Option<String>,
        #[arg(long, help = "Only packages supporting New Architecture")]
        new_arch: bool,
        #[arg(long, help = "Only packages with TypeScript types")]
        has_types: bool,
        #[arg(long, help = "Only packages with native code (iOS/Android)")]
        native: bool,
        #[arg(long, help = "Only packages with an Expo config plugin")]
        config_plugin: bool,
        #[arg(long, help = "Only packages tested in Expo's Nightly Program")]
        nightly: bool,
        #[arg(long, help = "Include archived/unmaintained packages (hidden by default)")]
        include_unmaintained: bool,
        #[arg(long, help = "Hide development tools (shown by default)")]
        no_dev: bool,
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },
    #[command(about = "Compare two packages side-by-side")]
    Compare { a: String, b: String },
    #[command(about = "Scan a project's package.json for issues")]
    Analyze {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(
            long,
            value_enum,
            default_value_t = FailOn::None,
            help = "Exit with code 1 when findings match: any | unmaintained | none"
        )]
        fail_on: FailOn,
    },
    #[command(about = "Inspect or manage the local cache")]
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    Info,
    Clear,
}

fn run() -> Result<i32> {
    let cli = Cli::parse();

    // Resolve output mode: explicit --pretty wins; else --json; else auto-detect agent/pipe.
    let json = if cli.pretty {
        false
    } else {
        cli.json || env::should_auto_json()
    };

    if !json {
        ui::disable_colors_if_piped();
    }

    match cli.command {
        Cmd::Search { query, limit } => {
            commands::search(&query, limit, cli.refresh, json)?;
            Ok(0)
        }
        Cmd::Info { name } => {
            commands::info(&name, cli.refresh, json)?;
            Ok(0)
        }
        Cmd::Alternatives { name, limit } => {
            commands::alternatives(&name, limit, cli.refresh, json)?;
            Ok(0)
        }
        Cmd::Trending { limit, include_js } => {
            commands::trending(limit, cli.refresh, include_js, json)?;
            Ok(0)
        }
        Cmd::Discover { limit, min_score, days } => {
            commands::discover(limit, min_score, days, cli.refresh, json)?;
            Ok(0)
        }
        Cmd::List {
            category,
            platform,
            works_with,
            module_type,
            new_arch,
            has_types,
            native,
            config_plugin,
            nightly,
            include_unmaintained,
            no_dev,
            limit,
        } => {
            commands::list(commands::ListFilters {
                category: category.as_deref(),
                platforms: &platform,
                works_with: &works_with,
                module_type: module_type.as_deref(),
                new_arch,
                has_types,
                native,
                config_plugin,
                nightly,
                include_unmaintained,
                no_dev,
                limit,
                refresh: cli.refresh,
                json,
            })?;
            Ok(0)
        }
        Cmd::Compare { a, b } => {
            commands::compare(&a, &b, cli.refresh, json)?;
            Ok(0)
        }
        Cmd::Analyze { path, fail_on } => {
            let found = commands::analyze(&path, cli.refresh, json, cli.quiet)?;
            let should_fail = match fail_on {
                FailOn::None => false,
                FailOn::Any => found.any_finding,
                FailOn::Unmaintained => found.has_unmaintained,
            };
            Ok(if should_fail { 1 } else { 0 })
        }
        Cmd::Cache { action } => {
            match action {
                CacheAction::Info => commands::cache_info()?,
                CacheAction::Clear => commands::cache_clear()?,
            }
            Ok(0)
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            ui::log::error(format!("{}", err));
            for cause in err.chain().skip(1) {
                ui::log::dim(format!("  caused by: {}", cause));
            }
            ExitCode::from(1)
        }
    }
}
