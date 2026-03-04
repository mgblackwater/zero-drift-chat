mod app;
mod config;
mod core;
mod providers;
mod storage;
mod tui;

use std::path::PathBuf;

use clap::Parser;

use crate::app::App;
use crate::config::AppConfig;
use crate::storage::{AddressBook, Database};

#[derive(Parser, Debug)]
#[command(name = "zero-drift-chat", about = "Unified messaging TUI")]
struct Cli {
    /// Path to config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Reset local cache and WhatsApp session (keeps address book)
    #[arg(long)]
    reset: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load config
    let config_path = cli
        .config
        .unwrap_or_else(|| PathBuf::from("configs/default.toml"));
    let config = AppConfig::load(&config_path)?;

    // Set up data directory
    let data_dir = PathBuf::from(&config.general.data_dir);
    std::fs::create_dir_all(&data_dir)?;

    // Set up file-based logging
    let log_file = data_dir.join("zero-drift.log");
    let file_appender = tracing_appender::rolling::never(&data_dir, "zero-drift.log");
    let log_level = if cli.debug {
        "debug"
    } else {
        &config.general.log_level
    };

    let subscriber = tracing_subscriber::fmt()
        .with_writer(file_appender)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with_ansi(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    tracing::info!("Starting zero-drift-chat");
    tracing::info!("Config loaded from: {}", config_path.display());
    tracing::info!("Data dir: {}", data_dir.display());
    tracing::info!("Log file: {}", log_file.display());

    // Handle --reset: delete main DB and WhatsApp session, keep address book
    if cli.reset {
        let db_path = data_dir.join("zero-drift.db");
        let wa_path = data_dir.join("whatsapp-session.db");
        let mut failed = false;
        for base in [&db_path, &wa_path] {
            // Also clean up SQLite WAL/SHM journal files
            let wal = base.with_extension("db-wal");
            let shm = base.with_extension("db-shm");
            for path in [base.clone(), wal, shm] {
                if path.exists() {
                    match std::fs::remove_file(&path) {
                        Ok(_) => {
                            eprintln!("Deleted {}", path.display());
                            tracing::info!("Reset: deleted {}", path.display());
                        }
                        Err(e) => {
                            eprintln!("Warning: could not delete {}: {}", path.display(), e);
                            tracing::warn!("Reset: failed to delete {}: {}", path.display(), e);
                            failed = true;
                        }
                    }
                }
            }
        }
        if failed {
            eprintln!("Some files could not be deleted. Close any other instances and try again.");
        } else {
            eprintln!("Reset complete. Address book preserved.");
        }
    }

    // Open databases
    let db_path = data_dir.join("zero-drift.db");
    let db = Database::open(db_path.to_str().unwrap_or("zero-drift.db"))?;

    let ab_path = data_dir.join("addressbook.db");
    let address_book = AddressBook::open(ab_path.to_str().unwrap_or("addressbook.db"))?;

    // Run app
    let mut app = App::new(config, db, address_book, config_path);
    app.run().await?;

    tracing::info!("zero-drift-chat exited cleanly");
    Ok(())
}
