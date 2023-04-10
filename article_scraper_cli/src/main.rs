use std::path::Path;
use std::{path::PathBuf, process::exit};

use crate::args::{Args, Commands};
use article_scraper::FtrConfigEntry;
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
        Commands::Ftr {
            html,
            base_url,
            source_url,
            config,
        } => extract_ftr(html, source_url, base_url, config, args.output).await,
    }
}

async fn extract_ftr(
    html_file: Option<PathBuf>,
    source_url: Option<String>,
    base_url: Option<String>,
    config: Option<String>,
    output: Option<PathBuf>,
) {
    let base_url = base_url.map(|url| Url::parse(&url).expect("invalid base url"));
    let html = get_html(html_file, source_url).await;

    let config = if let Some(config_path) = config {
        match FtrConfigEntry::parse_path(Path::new(&config_path)).await {
            Ok(entry) => Some(entry),
            Err(error) => {
                log::error!("Failed to parse config entry {config_path}: {error}");
                exit(0);
            }
        }
    } else {
        None
    };

    let full_text_parser = FullTextParser::new(None).await;
    let result = match full_text_parser
        .parse_offline(&html, config.as_ref(), base_url)
        .await
    {
        Ok(res) => res,
        Err(err) => {
            log::error!("Failed to extract content with ftr: {err}");
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

async fn extract_readability(
    html_file: Option<PathBuf>,
    source_url: Option<String>,
    base_url: Option<String>,
    output: Option<PathBuf>,
) {
    let base_url = base_url.map(|url| Url::parse(&url).expect("invalid base url"));
    let html = get_html(html_file, source_url).await;
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

async fn get_html(html_file: Option<PathBuf>, source_url: Option<String>) -> String {
    if html_file.is_none() && source_url.is_none() {
        log::error!("either need a source html file or source url");
        exit(0);
    }

    if html_file.is_some() && source_url.is_some() {
        log::error!("load source from html file or url? only specify one of the two options");
        exit(0);
    }

    let source_url = source_url.map(|url| Url::parse(&url).expect("invalid source url"));

    if let Some(source_url) = source_url {
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
    }
}
