use clap::{Parser, Subcommand};
use origami_core::imap::ImapBackend;
use origami_core::smtp::OrigamiSmtp;
use origami_core::MailBackend;

#[derive(Parser)]
#[command(name = "origami", version, about = "Origami debug CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Account operations
    Account {
        #[command(subcommand)]
        command: AccountCommands,
    },
    /// Mailbox (folder) operations
    Folder {
        #[command(subcommand)]
        command: FolderCommands,
    },
    /// Envelope (message metadata) operations
    Envelope {
        #[command(subcommand)]
        command: EnvelopeCommands,
    },
    /// Synchronize an account into the local store
    Sync {
        /// Account id (defaults to the default account)
        account: Option<String>,
    },
    /// Full-text search in the local store
    Search {
        /// FTS5 query (e.g. "digest", "from:alice")
        query: String,
    },
}

#[derive(Subcommand)]
enum AccountCommands {
    /// List configured accounts
    List,
    /// Verify IMAP/SMTP connectivity for an account
    Check {
        /// Account id (defaults to the default account)
        account: Option<String>,
    },
}

#[derive(Subcommand)]
enum FolderCommands {
    /// List mailboxes of an account
    List {
        /// Account id (defaults to the default account)
        account: Option<String>,
    },
}

#[derive(Subcommand)]
enum EnvelopeCommands {
    /// List envelopes of a mailbox, newest first
    List {
        /// Account id (defaults to the default account)
        account: Option<String>,
        /// Mailbox name (e.g. INBOX)
        #[arg(short, long, default_value = "INBOX")]
        mailbox: String,
        /// Page number (1 = most recent)
        #[arg(short, long, default_value = "1")]
        page: u32,
        /// Page size
        #[arg(short = 's', long, default_value = "20")]
        page_size: u32,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = origami_core::config::load()?;

    match cli.command {
        Commands::Account {
            command: AccountCommands::List,
        } => {
            if config.accounts.is_empty() {
                println!("no accounts configured (core v{})", origami_core::version());
                println!(
                    "config file: {}",
                    origami_core::config::config_path().display()
                );
            }
            for (id, account) in &config.accounts {
                let default = if account.default { " (default)" } else { "" };
                println!("{id}: {} <{}>{default}", account.name, account.email);
            }
        }

        Commands::Account {
            command: AccountCommands::Check { account },
        } => {
            let (id, account) = config.account(account.as_deref())?;
            println!("checking account `{id}` <{}>…", account.email);

            if let Some(imap) = &account.imap {
                let backend = ImapBackend::connect(id, imap).await?;
                let mailboxes = backend.list_mailboxes().await?;
                println!(
                    "  imap://{} — OK ({} mailboxes)",
                    imap.host,
                    mailboxes.len()
                );
            } else {
                println!("  imap — not configured");
            }

            if let Some(smtp) = &account.smtp {
                let _sender = OrigamiSmtp::connect(smtp).await?;
                println!("  smtp://{} — OK", smtp.host);
            } else {
                println!("  smtp — not configured");
            }
        }

        Commands::Folder {
            command: FolderCommands::List { account },
        } => {
            let (id, account) = config.account(account.as_deref())?;
            let imap = account
                .imap
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("account `{id}` has no IMAP config"))?;
            let backend = ImapBackend::connect(id, imap).await?;
            for mailbox in backend.list_mailboxes().await? {
                println!(
                    "{:30} {:10?} {:>6} total {:>6} unread",
                    mailbox.name, mailbox.role, mailbox.total, mailbox.unread
                );
            }
        }

        Commands::Envelope {
            command:
                EnvelopeCommands::List {
                    account,
                    mailbox,
                    page,
                    page_size,
                },
        } => {
            let (id, account) = config.account(account.as_deref())?;
            let imap = account
                .imap
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("account `{id}` has no IMAP config"))?;
            let backend = ImapBackend::connect(id, imap).await?;
            for envelope in backend.list_envelopes(&mailbox, page, page_size).await? {
                let from = envelope
                    .from
                    .first()
                    .map(|a| a.name.clone().unwrap_or_else(|| a.addr.clone()))
                    .unwrap_or_default();
                let date = envelope.date.unwrap_or_default();
                println!(
                    "uid {:>6} | {:20.20} | {:19.19} | {}",
                    envelope.server_uid.unwrap_or_default(),
                    from,
                    date,
                    envelope.subject,
                );
            }
        }

        Commands::Sync { account } => {
            let (id, account) = config.account(account.as_deref())?;
            let data_dir = data_dir()?;
            let store = origami_core::store::Store::open(&data_dir.join("db.sqlite3"))?;
            let blobs = origami_core::blob::BlobStore::open(&data_dir.join("blobs"))?;
            let engine = origami_core::sync::SyncEngine::new(store.clone(), blobs);

            println!("syncing account `{id}`…");
            let results = engine.sync_account(id, account).await?;
            for (folder, stats) in &results {
                println!(
                    "  {folder:20} +{} added ~{} changed -{} removed",
                    stats.added, stats.changed, stats.removed
                );
            }

            // Print local folder overview after sync.
            let account_db_id = store.upsert_account(id, &account.name, &account.email)?;
            for folder in store.list_folders(&account_db_id)? {
                println!(
                    "  {:20} {:>6} total {:>6} unread (local)",
                    folder.name, folder.total, folder.unread
                );
            }
        }

        Commands::Search { query } => {
            let store = origami_core::store::Store::open(&data_dir()?.join("db.sqlite3"))?;
            let hits = store.search(&query, 50)?;
            if hits.is_empty() {
                println!("no results for `{query}`");
            }
            for envelope in hits {
                let from = envelope
                    .from
                    .first()
                    .map(|a| a.name.clone().unwrap_or_else(|| a.addr.clone()))
                    .unwrap_or_default();
                println!("{:20.20} | {}", from, envelope.subject);
            }
        }
    }

    Ok(())
}

/// XDG data dir for Origami (~/.local/share/origami).
fn data_dir() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return Ok(std::path::PathBuf::from(dir).join("origami"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Ok(std::path::PathBuf::from(home).join(".local/share/origami"))
}
