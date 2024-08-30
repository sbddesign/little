use clap::{Parser};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use warp::Filter;
use serde_json;
use little::little_service_server::{LittleService, LittleServiceServer};
use little::{CommandRequest, CommandResponse};
mod commands;
use commands::Command;
use ldk_node::Builder;
use ldk_node::bitcoin::Network;
use names::Generator;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

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

    // Load or generate node alias
    let alias = match load_alias()? {
        Some(saved_alias) => saved_alias,
        None => {
            let mut generator = Generator::default();
            let new_alias = generator.next().unwrap();
            save_alias(&new_alias)?;
            new_alias
        }
    };

    // Create lightning node with loaded or generated alias
    let node = make_node(&alias, 9735);

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

fn make_node(alias: &str, port: u16) -> ldk_node::Node {
    let mut builder = Builder::new();
    builder.set_network(Network::Signet);
    builder.set_esplora_server("https://mutinynet.ltbl.io/api".to_string());
    builder.set_gossip_source_rgs("https://mutinynet.ltbl.io/snapshot".to_string());
    builder.set_storage_dir_path("./data".to_string());
    builder.set_listening_addresses(vec![format!("127.0.0.1:{}", port).parse().unwrap()]);

    let node = builder.build().unwrap();

    node.start().unwrap();

    println!("Node Public Key: {}", node.node_id());

    return node;
}

fn save_alias(alias: &str) -> std::io::Result<()> {
    let path = Path::new("./data/node_alias.txt");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)?;
    file.write_all(alias.as_bytes())?;
    Ok(())
}

fn load_alias() -> std::io::Result<Option<String>> {
    let path = Path::new("./data/node_alias.txt");
    if path.exists() {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(Some(contents))
    } else {
        Ok(None)
    }
}

