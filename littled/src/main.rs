use clap::{Parser, Subcommand};
use tonic::{transport::Server, Request, Response, Status};

use little::little_service_server::{LittleService, LittleServiceServer};
use little::{CommandRequest, CommandResponse};

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

#[derive(Default)]
pub struct MyLittleService {}

#[tonic::async_trait]
impl LittleService for MyLittleService {
    async fn execute_command(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let req = request.into_inner();
        println!("Received command: {:?}", req);

        // Here you would implement the actual command execution logic
        let response = CommandResponse {
            status: "received".to_string(),
            message: format!("Executed command: {}", req.command),
        };

        Ok(Response::new(response))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let service = MyLittleService::default();

    println!("LittleService server listening on {}", addr);

    Server::builder()
        .add_service(LittleServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
