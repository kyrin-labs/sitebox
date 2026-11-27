//! `sitebox-mcp` — stdio MCP server exposing the engine as typed tools.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "sitebox-mcp", version, about = "MCP server for SiteBox")]
struct Args {
    #[arg(long, env = "SITEBOX_ROOT")]
    root: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.root.unwrap_or_else(|| {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("sitebox"))
            .unwrap_or_else(|_| PathBuf::from("/home/l2s/sitebox"))
    });
    eprintln!("sitebox-mcp: root {}", root.display());
    sitebox_rs::mcp::serve_stdio(root).await
}
