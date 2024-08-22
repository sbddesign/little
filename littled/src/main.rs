use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use warp::Filter;

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

#[derive(Clone)]
struct MyLittleService {
    // Add any shared state here
    state: Arc<Mutex<String>>,
}

#[tonic::async_trait]
impl LittleService for MyLittleService {
    async fn execute_command(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let req = request.into_inner();
        println!("Received gRPC command: {:?}", req);

        // Here you would implement the actual command execution logic
        let response = CommandResponse {
            status: "received".to_string(),
            message: format!("Executed command: {}", req.command),
        };

        Ok(Response::new(response))
    }
}

async fn handle_http_command(
    command: serde_json::Value,
    service: MyLittleService,
) -> Result<impl warp::Reply, warp::Rejection> {
    println!("Received HTTP command: {:?}", command);

    // Here you would implement the actual command execution logic
    let response = serde_json::json!({
        "status": "received",
        "message": format!("Executed command: {}", command["command"])
    });

    Ok(warp::reply::json(&response))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = MyLittleService {
        state: Arc::new(Mutex::new(String::new())),
    };
    let service_clone = service.clone();

    // gRPC server
    let grpc_addr = "[::1]:50051".parse()?;
    let grpc_service = LittleServiceServer::new(service);
    let grpc_server = Server::builder().add_service(grpc_service).serve(grpc_addr);

    println!("gRPC server listening on {}", grpc_addr);

    // HTTP server
    let http_addr = ([127, 0, 0, 1], 3030);
    let http_routes = warp::post()
        .and(warp::path("little"))
        .and(warp::path("api"))
        .and(warp::path("v1"))
        .and(warp::path("command"))
        .and(warp::body::json())
        .and(warp::any().map(move || service_clone.clone()))
        .and_then(handle_http_command);

    let http_server = warp::serve(http_routes).run(http_addr);

    println!("HTTP server listening on http://{:?}", http_addr);

    // Run both servers concurrently
    tokio::join!(grpc_server, http_server);

    Ok(())
}
