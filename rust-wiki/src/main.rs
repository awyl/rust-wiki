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

fn vault_root() -> std::path::PathBuf {
    match std::env::var("WIKI_VAULT_ROOT") {
        Ok(env) => std::path::PathBuf::from(env),
        Err(_) => load_file_config()
            .and_then(|c| c.vault_root)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(default_root),
    }
}

fn load_file_config() -> Option<FileConfig> {
    let raw = exe_dir().and_then(|d| std::fs::read_to_string(d.join("config.toml")).ok())?;
    toml::from_str(&raw).ok()
}

fn port() -> u16 {
    if let Ok(p) = std::env::var("WIKI_PORT") {
        if let Ok(n) = p.parse() {
            return n;
        }
    }
    load_file_config().and_then(|c| c.port).unwrap_or(8484)
}

fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("cron") => return cmd_cron(&args),
        Some("watch") => return cmd_watch(&args),
        Some(other) => {
            eprintln!("unknown subcommand '{other}' — usage: rust-wiki [serve] | watch | cron --space <name>");
            std::process::exit(2);
        }
        None => {}
    }
    serve()
}

fn cmd_watch(args: &[String]) -> anyhow::Result<()> {
    let mut space: Option<&str> = None;
    let mut schedule = "daily";
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--space" => {
                i += 1;
                space = args.get(i).map(|s| s.as_str());
            }
            "--schedule" => {
                i += 1;
                schedule = args.get(i).map(|s| s.as_str()).unwrap_or("daily");
            }
            other => anyhow::bail!("unknown flag '{other}'"),
        }
        i += 1;
    }
    let space = space.ok_or_else(|| anyhow::anyhow!("--space is required"))?;
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    println!(
        "{}",
        rust_wiki::vault::watch::crontab_line(space, schedule, &exe).map_err(anyhow::Error::msg)?
    );
    Ok(())
}

fn cmd_cron(args: &[String]) -> anyhow::Result<()> {
    let mut space: Option<&str> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--space" => {
                i += 1;
                space = args.get(i).map(|s| s.as_str());
            }
            other => anyhow::bail!("unknown flag '{other}'"),
        }
        i += 1;
    }
    let space = space.ok_or_else(|| anyhow::anyhow!("--space is required"))?;
    let root = vault_root();
    let hub = Hub::new(root);
    println!(
        "{}",
        rust_wiki::vault::watch::cron_cycle(&hub, space).map_err(anyhow::Error::msg)?
    );
    Ok(())
}

fn serve() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let root = vault_root();
    std::fs::create_dir_all(&root)?;
    // Personal layer is a first-class citizen: create it on boot so
    // layered recall works from the first request.
    let personal =
        rust_wiki::vault::layout::VaultPaths::new(&root, rust_wiki::vault::layout::SPACE_PERSONAL);
    if !personal.config_file().exists() {
        rust_wiki::vault::bootstrap::bootstrap(
            &personal,
            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        )?;
        tracing::info!(
            "bootstrapped personal space at {}",
            personal.space_root.display()
        );
    }
    let hub = Hub::new(root);
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port()).parse()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(rust_wiki::server::serve(hub, addr))
}
