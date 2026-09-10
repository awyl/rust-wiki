//! rust-wiki server binary.
//!
//! Default transport: MCP over stdio (newline-delimited JSON-RPC on
//! stdin/stdout — for MCP hosts like aiproxy that spawn the binary).
//! `rust-wiki serve` = streamable-HTTP on 0.0.0.0:<port>.
//! Config resolution (KISS): env WIKI_VAULT_ROOT / WIKI_PORT override
//! `<exe_dir>/config.toml`; final fallback: `<exe_dir>/vaults`, port 8484.

use rust_wiki::hub::{default_root, Hub};

fn vault_root() -> std::path::PathBuf {
    rust_wiki::config::get()
        .vault_root
        .clone()
        .unwrap_or_else(default_root)
}

fn port() -> u16 {
    rust_wiki::config::get().port
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("cron") => return cmd_cron(&args),
        Some("migrate-okf") => return cmd_migrate_okf(&args),
        Some("serve") => return serve(),
        Some("stdio") => {}
        Some(other) => {
            eprintln!(
                "unknown subcommand '{other}' — usage: rust-wiki [stdio] | serve | cron --space <name> | migrate-okf --space <name>"
            );
            std::process::exit(2);
        }
        None => {}
    }
    stdio()
}

fn space_arg(args: &[String]) -> anyhow::Result<&str> {
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
    space.ok_or_else(|| anyhow::anyhow!("--space is required"))
}

fn cmd_cron(args: &[String]) -> anyhow::Result<()> {
    let space = space_arg(args)?;
    let root = vault_root();
    let hub = Hub::new(root);
    println!(
        "{}",
        rust_wiki::vault::watch::cron_cycle(&hub, space).map_err(anyhow::Error::msg)?
    );
    Ok(())
}

/// Upgrade a legacy space to OKF v0.2 (config key + projection rebuild).
fn cmd_migrate_okf(args: &[String]) -> anyhow::Result<()> {
    let space = space_arg(args)?;
    let vault = rust_wiki::vault::VaultPaths::new(&vault_root(), space);
    println!(
        "{}",
        rust_wiki::vault::okf::migrate(&vault).map_err(anyhow::Error::msg)?
    );
    Ok(())
}

/// Shared boot: vault root + personal space + scheduler. Both transports.
fn boot() -> anyhow::Result<Hub> {
    let root = vault_root();
    eprintln!(
        "rust-wiki v{} (vault root {})",
        env!("CARGO_PKG_VERSION"),
        root.display()
    );
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
    // One-time OKF rollout: bring every deployed vault onto the same
    // deterministic projections. Idempotent — already-migrated spaces are
    // skipped, so this is a no-op scan after the first boot.
    // ponytail: temporary bridge — delete `migrate_all` + this block once
    // every deployed vault carries `knowledge_format`.
    for (space, result) in rust_wiki::vault::okf::migrate_all(&root) {
        match result {
            Ok(report) => eprintln!("okf migration {space}: {report}"),
            Err(e) => eprintln!("okf migration {space}: skipped — {e}"),
        }
    }
    // Git backing at the vault root (init on first boot; auto-commit on
    // idle, pull --rebase with abort-on-conflict, push when upstream
    // exists). Best-effort — never blocks serving.
    match rust_wiki::vault::git::ensure_repo(&root) {
        Ok(true) => eprintln!("git backing initialized at {}", root.display()),
        Ok(false) => {}
        Err(e) => eprintln!("git backing unavailable: {e}"),
    }
    let hub = Hub::new(root.clone());
    // Rescan every space after a pull moved HEAD (human/Obsidian edits
    // need reindexing to become visible).
    let rescan_root = root.clone();
    let rescan = move || {
        let Ok(entries) = std::fs::read_dir(&rescan_root) else {
            return;
        };
        for name in entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.join("config.json").is_file())
            .filter_map(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
        {
            let v = rust_wiki::vault::layout::VaultPaths::new(&rescan_root, &name);
            if let Err(e) = rust_wiki::vault::registry::rebuild_metadata(&v) {
                eprintln!("git rescan {name}: {e}");
            }
        }
    };
    if rust_wiki::vault::git::spawn_git(root.clone(), rescan).is_some() {
        eprintln!(
            "git backing armed: every {}s, commit after {}s idle",
            rust_wiki::vault::git::tick_secs_from_env(),
            rust_wiki::vault::git::idle_secs_from_env()
        );
    }
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
