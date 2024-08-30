use clap::{Parser, Subcommand};
use little::little_service_client::LittleServiceClient;
use little::CommandRequest;

pub mod little {
    tonic::include_proto!("little");
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start {
        #[arg(short, long)]
        name: Option<String>,
    },
    Stop,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut client = LittleServiceClient::connect("http://[::1]:50051").await?;

    let request = tonic::Request::new(CommandRequest {
        command: match &cli.command {
            Commands::Start { .. } => "start".to_string(),
            Commands::Stop => "stop".to_string(),
        },
        arguments: match &cli.command {
            Commands::Start { name } => {
                let mut args = std::collections::HashMap::new();
                if let Some(n) = name {
                    args.insert("name".to_string(), n.clone());
                }
                args
            }
            Commands::Stop => std::collections::HashMap::new(),
        },
    });

    let response = client.execute_command(request).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
