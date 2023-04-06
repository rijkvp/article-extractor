use std::{path::PathBuf, process::exit};

use crate::args::{Args, Commands};
use clap::Parser;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use url::Url;

mod args;

pub fn main() {
    let args = Args::parse();

    let level = if args.debug {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    TermLogger::init(
        level,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .unwrap();

    match args.command {
        Commands::Readability {
            html,
            base_url,
            source_url,
        } => extract_readability(html, source_url, base_url),
    }

    log::info!("hello world");
}

fn extract_readability(
    html_file: Option<PathBuf>,
    source_url: Option<String>,
    base_url: Option<String>,
) {
    if html_file.is_none() && source_url.is_none() {
        log::error!("");
        exit(0);
    }

    let source_url = source_url.map(|url| Url::parse(&url).expect("invalid source url"));
    let base_url = base_url.map(|url| Url::parse(&url).expect("invalid base url"));
}
