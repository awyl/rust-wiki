//! rust-wiki server binary.
//!
//! Config resolution (KISS): env WIKI_VAULT_ROOT / WIKI_PORT override
//! `<exe_dir>/config.toml`; final fallback: `<exe_dir>/vaults`, port 8484.

use rust_wiki::hub::{default_root, Hub};

#[derive(Debug, serde::Deserialize, Default)]
struct FileConfig {
    port: Option<u16>,
    vault_root: Option<String>,
}

fn port() -> u16 {
    if let Ok(p) = std::env::var("WIKI_PORT") {
        if let Ok(n) = p.parse() {
            return n;
        }
    }
    exe_dir()
        .and_then(|d| std::fs::read_to_string(d.join("config.toml")).ok())
        .and_then(|raw| toml::from_str::<FileConfig>(&raw).ok())
        .and_then(|c| c.port)
        .unwrap_or(8484)
}

fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let root = default_root();
    std::fs::create_dir_all(&root)?;
    // Personal layer is a first-class citizen: create it on boot so
    // layered recall works from the first request.
    let personal = rust_wiki::vault::layout::VaultPaths::new(&root, rust_wiki::vault::layout::SPACE_PERSONAL);
    if !personal.config_file().exists() {
        rust_wiki::vault::bootstrap::bootstrap(&personal, &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))?;
        tracing::info!("bootstrapped personal space at {}", personal.space_root.display());
    }
    let hub = Hub::new(root);
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port()).parse()?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(rust_wiki::server::serve(hub, addr))
}
