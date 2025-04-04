use clap::{Parser};
use little::little_service_client::LittleServiceClient;
use little::CommandRequest;
use std::process::Command as ProcessCommand;
use tokio::time::sleep;
use std::time::Duration;
use std::error::Error;
use std::fmt;
use std::env;
use std::path::PathBuf;

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
    /// Port for the gRPC API (default: 50051)
    #[arg(long = "grpcport", default_value = "50051")]
    grpc_port: u16,

    #[command(subcommand)]
    command: littled::commands::Command,
}

async fn is_daemon_running(grpc_port: u16) -> bool {
    match LittleServiceClient::connect(format!("http://[::1]:{}", grpc_port)).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

async fn start_daemon(lightning_port: u16, grpc_port: u16, http_port: u16) -> Result<(), CliError> {
    println!("Starting littled daemon...");
    
    // Get the path to the current executable
    let current_exe = env::current_exe()
        .map_err(|e| CliError::DaemonStart(format!("Failed to get current executable path: {}", e)))?;
    
    // Get the directory containing the current executable
    let current_dir = current_exe.parent()
        .ok_or_else(|| CliError::DaemonStart("Failed to get parent directory".to_string()))?;
    
    // Construct the path to littled in the same directory
    let daemon_path = current_dir.join("littled");
    
    // Check if the daemon exists
    if !daemon_path.exists() {
        return Err(CliError::DaemonStart(format!(
            "Daemon binary not found at {}. Please ensure littled is in the same directory as little-cli.",
            daemon_path.display()
        )));
    }

    let mut child = ProcessCommand::new(daemon_path)
        .arg("start")
        .arg("--lightningport")
        .arg(lightning_port.to_string())
        .arg("--grpcport")
        .arg(grpc_port.to_string())
        .arg("--httpport")
        .arg(http_port.to_string())
        .spawn()
        .map_err(|e| CliError::DaemonStart(e.to_string()))?;

    // Give the daemon a moment to start
    sleep(Duration::from_secs(2)).await;
    
    // Check if process is still running
    match child.try_wait()
        .map_err(|e| CliError::DaemonStart(e.to_string()))? {
        Some(status) => {
            Err(CliError::DaemonStart(format!("Process exited with status {}", status)))
        },
        None => Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // If this is a start command and daemon isn't running, start it
    if let littled::commands::Command::Start { lightning_port, grpc_port, http_port, .. } = cli.command {
        if !is_daemon_running(grpc_port).await {
            start_daemon(lightning_port, grpc_port, http_port).await?;
        }
    }

    // Try to connect to the daemon
    let mut client = LittleServiceClient::connect(format!("http://[::1]:{}", cli.grpc_port)).await
        .map_err(|e| CliError::Connection(e.to_string()))?;

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

    Ok(())
}
