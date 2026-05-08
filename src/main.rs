//! Sentrits CLI entry point

use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;

use pty_claude::auth::default_authorizer::DefaultAuthorizer;
use pty_claude::auth::default_pairing_service::DefaultPairingService;
use pty_claude::auth::pairing::PairingService;
use pty_claude::net::{AppState, ServerConfig, run_servers};
use pty_claude::service::observation_store::ObservationStore;
use pty_claude::session::registry::SessionRegistry;
use pty_claude::store::file_store::{FileHostConfigStore, FilePairingStore, FileSessionStore};
use pty_claude::store::host_config::HostIdentity;

#[derive(Parser, Debug)]
#[command(name = "pty-claude")]
#[command(version = "0.2.5")]
#[command(about = "Terminal session management daemon", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Start the daemon server
    Serve {
        /// Admin bind address
        #[arg(long, default_value = "127.0.0.1")]
        admin_host: String,
        /// Admin port
        #[arg(long, default_value_t = 18085)]
        admin_port: u16,
        /// Remote bind address
        #[arg(long, default_value = "0.0.0.0")]
        remote_host: String,
        /// Remote port
        #[arg(long, default_value_t = 18086)]
        remote_port: u16,
        /// Data directory
        #[arg(long)]
        datadir: Option<String>,
        /// Disable UDP discovery
        #[arg(long)]
        no_discovery: bool,
    },
    /// Service management
    Service {
        #[command(subcommand)]
        action: ServiceCommands,
    },
    /// Local PTY session
    LocalPty {
        /// Command to run (default: /bin/sh)
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },
    /// Session management
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },
    /// Records management
    Records {
        #[command(subcommand)]
        action: RecordsCommands,
    },
    /// Host management
    Host {
        #[command(subcommand)]
        action: HostCommands,
    },
    /// Evidence operations
    Evidence {
        #[command(subcommand)]
        action: EvidenceCommands,
    },
    /// Observations
    Observations {
        #[command(subcommand)]
        action: ObservationsCommands,
    },
    /// Capture operations
    Capture {
        #[command(subcommand)]
        action: CaptureCommands,
    },
}

#[derive(Parser, Debug)]
enum ServiceCommands {
    /// Install system service
    Install,
    /// Print service configuration
    Print,
}

#[derive(Parser, Debug)]
enum SessionCommands {
    /// List sessions
    List,
    /// Show session details
    Show {
        /// Session ID
        session_id: String,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Start a new session
    Start {
        /// Session title
        #[arg(long)]
        title: Option<String>,
        /// Workspace path
        #[arg(long)]
        workspace: Option<String>,
        /// Provider type
        #[arg(long)]
        provider: Option<String>,
        /// Attach after creation
        #[arg(long)]
        attach: bool,
    },
    /// Attach to a session
    Attach {
        /// Session ID
        session_id: String,
    },
    /// Observe a session
    Observe {
        /// Session ID
        session_id: String,
    },
    /// Stop a session
    Stop {
        /// Session ID
        session_id: String,
    },
    /// Clear inactive sessions
    Clear,
}

#[derive(Parser, Debug)]
enum RecordsCommands {
    /// List launch records
    List,
    /// Show record details
    Show {
        /// Record ID
        record_id: String,
    },
}

#[derive(Parser, Debug)]
enum HostCommands {
    /// Show host status
    Status,
    /// Set host display name
    SetName {
        /// Display name
        name: String,
    },
    /// Set hub configuration
    SetHub {
        /// Hub URL
        hub_url: String,
        /// Hub token
        hub_token: String,
    },
    /// Clear hub configuration
    ClearHub,
}

#[derive(Parser, Debug)]
enum EvidenceCommands {
    /// Tail evidence
    Tail {
        /// Session ID
        session_id: String,
        /// Number of lines
        #[arg(long, default_value_t = 200)]
        lines: usize,
    },
    /// Search evidence
    Search {
        /// Session ID
        session_id: String,
        /// Search query
        query: String,
    },
}

#[derive(Parser, Debug)]
enum ObservationsCommands {
    /// List observations
    List,
}

#[derive(Parser, Debug)]
enum CaptureCommands {
    /// Start capture session
    Start {
        /// Session title
        #[arg(long)]
        title: Option<String>,
        /// Workspace path
        #[arg(long)]
        workspace: Option<String>,
    },
}

fn main() {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::fmt::init();

    match args.command {
        Commands::Serve {
            admin_host,
            admin_port,
            remote_host,
            remote_port,
            datadir,
            no_discovery: _,
        } => {
            let data_dir = datadir.unwrap_or_else(|| {
                dirs::data_local_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("pty-claude")
                    .to_string_lossy()
                    .to_string()
            });
            let data_path = std::path::PathBuf::from(&data_dir);

            // Ensure data directory exists
            std::fs::create_dir_all(&data_path).unwrap_or_else(|e| {
                eprintln!("Failed to create data directory '{}': {}", data_dir, e);
                std::process::exit(1);
            });

            // Initialize stores
            let host_identity = HostIdentity::default();
            let host_config_store = Arc::new(
                FileHostConfigStore::new(data_path.clone())
            );
            let session_store = Arc::new(
                FileSessionStore::new(data_path.clone())
            );
            let pairing_store = Arc::new(
                FilePairingStore::new(data_path)
            );

            // Initialize services
            let authorizer = Arc::new(DefaultAuthorizer::new(pairing_store.clone()));
            let pairing_service: Arc<RwLock<dyn PairingService>> = Arc::new(RwLock::new(
                DefaultPairingService::new(pairing_store)
            ));
            let observation_store = Arc::new(RwLock::new(ObservationStore::default()));
            let session_registry = Arc::new(SessionRegistry::new());

            let state = AppState {
                authorizer,
                pairing_service,
                session_store,
                host_config_store,
                observation_store,
                session_registry,
                terminal_password: std::env::var("PTY_PASSWORD").unwrap_or_else(|_| "1111".to_string()),
            };

            let config = ServerConfig {
                admin_host,
                admin_port,
                remote_host,
                remote_port,
                remote_tls: false,
                cors_origins: vec!["*".to_string()],
            };

            println!("pty-claude v0.2.5 starting...");
            println!("  Admin:  {}:{}", config.admin_host, config.admin_port);
            println!("  Remote: {}:{}", config.remote_host, config.remote_port);
            println!("  Data:   {}", data_dir);

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if let Err(e) = run_servers(config, state).await {
                    eprintln!("Server error: {}", e);
                    std::process::exit(1);
                }
            });
        }
        Commands::Service { action } => {
            match action {
                ServiceCommands::Install => {
                    eprintln!("service install not yet implemented in Rust");
                }
                ServiceCommands::Print => {
                    eprintln!("service print not yet implemented in Rust");
                }
            }
            std::process::exit(1);
        }
        Commands::LocalPty { command } => {
            let cmd = if command.is_empty() {
                vec!["/bin/sh".to_string()]
            } else {
                command
            };
            eprintln!("local-pty with command: {:?}", cmd);
            eprintln!("not yet implemented in Rust");
            std::process::exit(1);
        }
        Commands::Session { action } => {
            match action {
                SessionCommands::List => {
                    eprintln!("session list not yet implemented in Rust");
                }
                SessionCommands::Show { session_id, json } => {
                    eprintln!("session show {} (json={}) not yet implemented", session_id, json);
                }
                SessionCommands::Start { title, workspace, provider, attach } => {
                    eprintln!("session start title={:?} workspace={:?} provider={:?} attach={}",
                        title, workspace, provider, attach);
                    eprintln!("not yet implemented in Rust");
                }
                SessionCommands::Attach { session_id } => {
                    eprintln!("session attach {} not yet implemented", session_id);
                }
                SessionCommands::Observe { session_id } => {
                    eprintln!("session observe {} not yet implemented", session_id);
                }
                SessionCommands::Stop { session_id } => {
                    eprintln!("session stop {} not yet implemented", session_id);
                }
                SessionCommands::Clear => {
                    eprintln!("session clear not yet implemented");
                }
            }
            std::process::exit(1);
        }
        Commands::Records { action } => {
            match action {
                RecordsCommands::List => {
                    eprintln!("records list not yet implemented");
                }
                RecordsCommands::Show { record_id } => {
                    eprintln!("records show {} not yet implemented", record_id);
                }
            }
            std::process::exit(1);
        }
        Commands::Host { action } => {
            match action {
                HostCommands::Status => {
                    eprintln!("host status not yet implemented");
                }
                HostCommands::SetName { name } => {
                    eprintln!("host set-name {} not yet implemented", name);
                }
                HostCommands::SetHub { hub_url, hub_token: _ } => {
                    eprintln!("host set-hub {} *** not yet implemented", hub_url);
                }
                HostCommands::ClearHub => {
                    eprintln!("host clear-hub not yet implemented");
                }
            }
            std::process::exit(1);
        }
        Commands::Evidence { action } => {
            match action {
                EvidenceCommands::Tail { session_id, lines } => {
                    eprintln!("evidence tail {} (lines={}) not yet implemented", session_id, lines);
                }
                EvidenceCommands::Search { session_id, query } => {
                    eprintln!("evidence search {} '{}' not yet implemented", session_id, query);
                }
            }
            std::process::exit(1);
        }
        Commands::Observations { action } => {
            match action {
                ObservationsCommands::List => {
                    eprintln!("observations list not yet implemented");
                }
            }
            std::process::exit(1);
        }
        Commands::Capture { action } => {
            match action {
                CaptureCommands::Start { title, workspace } => {
                    eprintln!("capture start title={:?} workspace={:?} not yet implemented", title, workspace);
                }
            }
            std::process::exit(1);
        }
    }
}