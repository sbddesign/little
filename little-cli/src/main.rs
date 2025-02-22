use clap::{Parser};
use little::little_service_client::LittleServiceClient;
use little::CommandRequest;
use std::process::Command as ProcessCommand;
use tokio::time::sleep;
use std::time::Duration;
use std::error::Error;
use std::fmt;

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
    #[command(subcommand)]
    command: littled::commands::Command,
}

async fn is_daemon_running() -> bool {
    match LittleServiceClient::connect("http://[::1]:50051").await {
        Ok(_) => true,
        Err(_) => false,
    }
}

async fn start_daemon() -> Result<(), CliError> {
    println!("Starting littled daemon...");
    let mut child = ProcessCommand::new("./target/debug/littled")
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
    if matches!(cli.command, littled::commands::Command::Start { .. }) && !is_daemon_running().await {
        start_daemon().await?;
    }

    // Try to connect to the daemon
    let mut client = LittleServiceClient::connect("http://[::1]:50051").await
        .map_err(|e| CliError::Connection(e.to_string()))?;

    let request = tonic::Request::new(CommandRequest {
        command: serde_json::to_string(&cli.command)
            .map_err(|e| CliError::Command(e.to_string()))?,
        arguments: std::collections::HashMap::new(),
    });

    let response = client.execute_command(request).await
        .map_err(|e| CliError::Command(e.to_string()))?;
    
    println!("RESPONSE={:?}", response);

    Ok(())
}
