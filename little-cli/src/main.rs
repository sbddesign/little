use clap::{Parser};
use little::little_service_client::LittleServiceClient;
use little::CommandRequest;
use std::process::Command as ProcessCommand;
use std::error::Error;
use std::fmt;
use std::env;
use std::path::PathBuf;
use littled::config::{get_default_data_dir, load_config};
use littled::commands::Command;

pub mod little {
    tonic::include_proto!("little");
}

#[derive(Debug)]
enum CliError {
    DaemonStart(String),
    Connection(String),
    Command(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::DaemonStart(msg) => write!(f, "Failed to start daemon: {}", msg),
            CliError::Connection(msg) => write!(f, "Connection error: {}", msg),
            CliError::Command(msg) => write!(f, "Command error: {}", msg),
        }
    }
}

impl Error for CliError {}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Data directory path (default: ~/.little)
    #[arg(long = "datadir")]
    data_dir: Option<String>,

    #[command(subcommand)]
    command: littled::commands::Command,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Determine data directory
    let data_dir = if let Some(dir) = &cli.data_dir {
        PathBuf::from(shellexpand::tilde(dir).into_owned())
    } else {
        get_default_data_dir()
    };

    // Load config to get ports
    let config = load_config(&data_dir)?;

    // If this is a start command, start the daemon first
    match cli.command {
        littled::commands::Command::Start { name, data_dir, .. } => {
            // Start the daemon
            let mut daemon = ProcessCommand::new("littled");
            daemon.arg("start");
            
            // Use the CLI's data_dir if provided, otherwise use the one from the command
            let data_dir_path = cli.data_dir.as_ref().or(data_dir.as_ref());
            if let Some(dir) = data_dir_path {
                daemon.arg("--datadir").arg(dir);
            }
            
            if let Some(node_name) = &name {
                daemon.arg("--name").arg(node_name);
            }
            
            let mut child = daemon.spawn()?;
            println!("Starting littled daemon...");
            std::thread::sleep(std::time::Duration::from_secs(2));
            
            // Load config to get ports
            let data_dir = if let Some(dir) = data_dir_path {
                PathBuf::from(shellexpand::tilde(dir).into_owned())
            } else {
                get_default_data_dir()
            };
            
            let config = load_config(&data_dir)?;
            
            // Connect to the daemon using the port from the config file
            let channel = tonic::transport::Channel::from_shared(format!("http://[::1]:{}", config.grpc_port))
                .map_err(|e| CliError::Connection(e.to_string()))?
                .connect()
                .await?;
            
            let mut client = LittleServiceClient::new(channel);
            
            // Send the start command with default values for the ports (they will be loaded from config)
            let request = tonic::Request::new(CommandRequest {
                command: serde_json::to_string(&Command::Start { 
                    name, 
                    data_dir: None, // Don't need to pass data_dir here since it's already set in the daemon
                    lightning_port: config.lightning_port,
                    grpc_port: config.grpc_port,
                    http_port: config.http_port,
                })?,
                arguments: std::collections::HashMap::new(),
            });
            
            let response = client.execute_command(request).await?;
            println!("Response: {:?}", response.into_inner());
            
            // Wait for the daemon to exit
            child.wait()?;
        },
        _ => {
            // Try to connect to the daemon using the port from the config file
            let mut client = LittleServiceClient::connect(format!("http://[::1]:{}", config.grpc_port)).await
                .map_err(|e| CliError::Connection(format!("Failed to connect to daemon on port {}: {}", config.grpc_port, e)))?;

            let request = tonic::Request::new(CommandRequest {
                command: serde_json::to_string(&cli.command)
                    .map_err(|e| CliError::Command(e.to_string()))?,
                arguments: std::collections::HashMap::new(),
            });

            let response = client.execute_command(request).await
                .map_err(|e| CliError::Command(e.to_string()))?;
            
            // Format the response in a cleaner way
            let response = response.into_inner();
            if response.status == "error" {
                println!("Error: {}", response.message);
            } else {
                // Try to parse the message as JSON and pretty print it
                match serde_json::from_str::<serde_json::Value>(&response.message) {
                    Ok(json) => {
                        println!("{}", serde_json::to_string_pretty(&json).unwrap());
                    },
                    Err(_) => {
                        // If it's not valid JSON, just print the message as is
                        println!("{}", response.message);
                    }
                }
            }
        }
    }

    Ok(())
}
