//! rust-wiki server binary.
//!
//! Default transport: MCP over stdio (newline-delimited JSON-RPC on
//! stdin/stdout — for MCP hosts like aiproxy that spawn the binary).
//! `rust-wiki serve` = streamable-HTTP on 0.0.0.0:<port>.
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
        Some("serve") => return serve(),
        Some("stdio") => {}
        Some(other) => {
            eprintln!(
                "unknown subcommand '{other}' — usage: rust-wiki [stdio] | serve | cron --space <name>"
            );
            std::process::exit(2);
        }
        None => {}
    }
    stdio()
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

/// Shared boot: vault root + personal space + scheduler. Both transports.
fn boot() -> anyhow::Result<Hub> {
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
        eprintln!(
            "bootstrapped personal space at {}",
            personal.space_root.display()
        );
    }
    let hub = Hub::new(root.clone());
    // Built-in maintenance scheduler: hourly mechanical cycle across all
    // spaces, on by default. WIKI_CRON_INTERVAL_SECS=0 disables.
    if let Some(handle) = rust_wiki::vault::watch::spawn(root.clone()) {
        eprintln!(
            "maintenance scheduler armed: every {}s across {}",
            rust_wiki::vault::watch::interval_from_env(),
            root.display()
        );
        let _ = handle; // joined implicitly at process exit
    }
    Ok(hub)
}

/// MCP over stdio: newline-delimited JSON-RPC on stdin/stdout. Default mode —
/// an MCP host (aiproxy) spawns the binary and speaks over the pipes.
/// stdout carries protocol only; diagnostics go to stderr.
fn stdio() -> anyhow::Result<()> {
    let hub = boot()?;
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<rust_wiki::server::RpcRequest>(&line) {
            Ok(req) => rust_wiki::server::handle_json_rpc(&hub, "stdio", &req),
            Err(e) => Some(rust_wiki::server::rpc_err(
                None,
                -32700,
                format!("parse error: {e}"),
            )),
        };
        if let Some(resp) = response {
            // Compact single line — the newline IS the message delimiter.
            serde_json::to_writer(&mut out, &resp)?;
            out.write_all(b"\n")?;
            out.flush()?;
        }
    }
    Ok(())
}

fn serve() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let hub = boot()?;
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port()).parse()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(rust_wiki::server::serve(hub, addr))
}
