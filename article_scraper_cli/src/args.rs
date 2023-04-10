use clap::{command, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Turn debug logging on
    #[arg(short, long)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Commands,

    /// Destination of resulting HTML file
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Only use the Readability parser
    Readability {
        /// Source HTML file
        #[arg(long, value_name = "FILE")]
        html: Option<PathBuf>,

        /// Base to complete relative Url
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,

        /// Source Url to download HTML from
        #[arg(long, value_name = "URL")]
        source_url: Option<String>,
    },
    Ftr {
        /// Source HTML file
        #[arg(long, value_name = "FILE")]
        html: Option<PathBuf>,

        /// Base to complete relative Url
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,

        /// Source Url to download HTML from
        #[arg(long, value_name = "URL")]
        source_url: Option<String>,

        /// The Ftr config to use
        /// Otherwise source_url and base_url will be used
        #[arg(long, value_name = "domain")]
        config: Option<String>,
    },
}
