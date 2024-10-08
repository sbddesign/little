use clap::{Parser};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use warp::Filter;
use serde_json;
// use commands::Command;

use little::little_service_server::{LittleService, LittleServiceServer};
use little::{CommandRequest, CommandResponse};

mod commands;
use commands::Command;

pub mod little {
    tonic::include_proto!("little");
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
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

        let command: Command = serde_json::from_str(&req.command)
            .map_err(|e| Status::invalid_argument(format!("Invalid command: {}", e)))?;

        // Here you would implement the actual command execution logic
        let response = match command {
            Command::Start { name } => CommandResponse {
                status: "started".to_string(),
                message: format!("Started with name: {:?}", name),
            },
            Command::Stop => CommandResponse {
                status: "stopped".to_string(),
                message: "Stopped".to_string(),
            },
            Command::GetInfo => CommandResponse {
                status: "info".to_string(),
                message: "GetInfo not implemented yet".to_string(),
            },
        };

        Ok(Response::new(response))
    }
}

async fn handle_http_command(
    command: serde_json::Value,
    _service: MyLittleService,
) -> Result<impl warp::Reply, warp::Rejection> {
    println!("Received HTTP command: {:?}", command);

    let command_str = command["command"].as_str().unwrap_or("").to_lowercase();
    
    // Here you would implement the actual command execution logic
    let response = match command_str.as_str() {
        "start" => {
            let name = command["arguments"].get(0).and_then(|v| v.as_str()).unwrap_or("default");
            serde_json::json!({
                "status": "started",
                "message": format!("Started with name: {}", name)
            })
        },
        "stop" => serde_json::json!({
            "status": "stopped",
            "message": "Stopped"
        }),
        "getinfo" => serde_json::json!({
            "status": "info",
            "message": "GetInfo not implemented yet"
        }),
        _ => return Err(warp::reject::custom(InvalidCommand(format!("Unknown command: {}", command_str))))
    };

    Ok(warp::reply::json(&response))
}

#[derive(Debug)]
struct InvalidCommand(String);

impl warp::reject::Reject for InvalidCommand {}

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
