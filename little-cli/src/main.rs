use clap::{Parser, Subcommand};
use tokio::net::UnixStream;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // Define your subcommands here
    Start {
        #[arg(short, long)]
        name: Option<String>,
    },
    Stop,
    // Add more subcommands as needed
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let socket_path = "/tmp/little.sock";
    let stream = match UnixStream::connect(socket_path).await {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("Failed to connect to the server: {}", e);
            std::process::exit(1);
        }
    };

    // Serialize the command to send to the server
    let command = match &cli.command {
        Commands::Start { name } => serde_json::json!({
            "command": "start",
            "arguments": { "name": name }
        }),
        Commands::Stop => serde_json::json!({
            "command": "stop"
        }),
        // Handle other commands
    };

    // Send the command and receive the response
    // Implement the communication protocol here

    // Print the response
    println!("Response: {:?}", command);
}
