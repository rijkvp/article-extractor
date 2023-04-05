use clap::Parser;

mod args;

pub fn main() {
    let _args = args::Args::parse();
    println!("hello world");
}
