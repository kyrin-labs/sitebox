//! sitebox-rs — agent-native engine for SiteBox.
//!
//! The engine owns four things the Node dashboard never did:
//!   1. a machine-readable contract per site (`sitebox.toml`)
//!   2. a static server that is identical for every site (and correct)
//!   3. a check catalog with stable ids, so "done" is provable
//!   4. a single supervisor with persisted logs
//!
//! The dashboard is one client of this engine, not the owner of the behaviour.

pub mod assets;
pub mod checks;
pub mod dashboard;
pub mod deploy;
pub mod manifest;
pub mod mcp;
pub mod pipeline;
pub mod registry;
pub mod serve;
pub mod supervisor;
pub mod util;

pub use manifest::{CheckCfg, Manifest, PipelineStep, SiteMeta};
pub use registry::{Site, SiteConfig};
