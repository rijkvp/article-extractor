use clap::{command, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Turn debug logging on
    #[arg(short, long)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Destination of resulting HTML file
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Only use the Readability parser
    Readability {
        /// Source HTML file
        #[arg(long, value_name = "FILE")]
        html: Option<PathBuf>,

        /// Source Url
        #[arg(long, value_name = "URL")]
        source_url: Option<String>,
    },
}
