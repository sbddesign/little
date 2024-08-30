use clap::{Parser};
use little::little_service_client::LittleServiceClient;
use little::CommandRequest;

pub mod little {
    tonic::include_proto!("little");
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: littled::commands::Command,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut client = LittleServiceClient::connect("http://[::1]:50051").await?;

    let request = tonic::Request::new(CommandRequest {
        command: serde_json::to_string(&cli.command)?,
        arguments: std::collections::HashMap::new(),
    });

    let response = client.execute_command(request).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
