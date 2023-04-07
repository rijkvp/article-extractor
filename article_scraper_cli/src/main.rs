use std::{path::PathBuf, process::exit};

use crate::args::{Args, Commands};
use article_scraper::FullTextParser;
use article_scraper::Readability;
use clap::Parser;
use reqwest::header::HeaderMap;
use reqwest::Client;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use url::Url;

mod args;

#[tokio::main]
async fn main() {
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
        } => extract_readability(html, source_url, base_url, args.output).await,
    }

    log::info!("hello world");
}

async fn extract_readability(
    html_file: Option<PathBuf>,
    source_url: Option<String>,
    base_url: Option<String>,
    output: Option<PathBuf>,
) {
    if html_file.is_none() && source_url.is_none() {
        log::error!("either need a source html file or source url");
        exit(0);
    }

    if html_file.is_some() && source_url.is_some() {
        log::error!("load source from html file or url? only specify one of the two options");
        exit(0);
    }

    let source_url = source_url.map(|url| Url::parse(&url).expect("invalid source url"));
    let base_url = base_url.map(|url| Url::parse(&url).expect("invalid base url"));

    let html = if let Some(source_url) = source_url {
        match FullTextParser::download(&source_url, &Client::new(), HeaderMap::new()).await {
            Ok(html) => html,
            Err(err) => {
                log::error!("Failed to download html from url: {err}");
                exit(0);
            }
        }
    } else if let Some(source_file) = html_file {
        match std::fs::read_to_string(&source_file) {
            Ok(html) => html,
            Err(err) => {
                log::error!("Failed to read file {source_file:?}: {err}");
                exit(0);
            }
        }
    } else {
        unreachable!()
    };

    let result = match Readability::extract_from_str(&html, base_url).await {
        Ok(res) => res,
        Err(err) => {
            log::error!("Failed to extract content with readability: {err}");
            exit(0);
        }
    };

    let output = if let Some(output) = output {
        output
    } else {
        PathBuf::from("result.html")
    };

    match std::fs::write(&output, result) {
        Ok(()) => log::info!("successfully written result to {output:?}"),
        Err(err) => {
            log::error!("Failed to write to file {output:?}: {err}");
            exit(0);
        }
    }
}
