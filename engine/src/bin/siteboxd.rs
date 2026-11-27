//! `siteboxd` — dashboard API + supervisor in one binary.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;

use sitebox_rs::dashboard::{self, DashState};
use sitebox_rs::studio::StudioState;
use sitebox_rs::supervisor::Supervisor;

#[derive(Parser, Debug)]
#[command(name = "siteboxd", version, about = "SiteBox dashboard (Rust engine)")]
struct Args {
    #[arg(long, env = "SITEBOX_ROOT")]
    root: Option<PathBuf>,
    #[arg(long, default_value_t = 4445)]
    port: u16,
    #[arg(long)]
    ui: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.root.unwrap_or_else(|| {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("sitebox"))
            .unwrap_or_else(|_| PathBuf::from("/home/l2s/sitebox"))
    });
    let ui = args.ui.unwrap_or_else(|| root.join("dashboard/public"));
    if !ui.is_dir() {
        eprintln!(
            "warning: UI folder not found at {} — API only",
            ui.display()
        );
    }
    let sup = Arc::new(Supervisor::new(root.clone()));
    let client = reqwest::Client::new();
    let studio = StudioState::new(root.clone(), sup.clone(), client.clone());
    let state = DashState {
        root: root.clone(),
        ui,
        sup,
        client,
        registry_error: Default::default(),
        studio,
    };
    dashboard::serve(args.port, state).await
}
