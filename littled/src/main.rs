use clap::{Parser};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use warp::Filter;
use serde_json;
use little::little_service_server::{LittleService, LittleServiceServer};
use little::{CommandRequest, CommandResponse};
mod commands;
use commands::{Command, GetInfoResponse};
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
    state: Arc<Mutex<String>>,
    alias: String,
    node_id: String,
    node: Arc<Mutex<Option<ldk_node::Node>>>,
    shutdown_signal: Arc<tokio::sync::broadcast::Sender<()>>,
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

        let response = match command {
            Command::Start { name } => CommandResponse {
                status: "started".to_string(),
                message: format!("Started with name: {:?}", name),
            },
            Command::Stop => {
                // Gracefully stop the LDK node
                if let Some(node) = self.node.lock().await.take() {
                    println!("Stopping LDK node...");
                    // Spawn a blocking task to stop the node
                    tokio::task::spawn_blocking(move || {
                        if let Err(e) = node.stop() {
                            eprintln!("Error stopping node: {}", e);
                        }
                    });
                }
                
                // Signal shutdown to the servers
                let _ = self.shutdown_signal.send(());
                
                CommandResponse {
                    status: "stopped".to_string(),
                    message: "Node shutdown initiated".to_string(),
                }
            },
            Command::GetInfo => CommandResponse {
                status: "info".to_string(),
                message: serde_json::to_string(&GetInfoResponse {
                    alias: self.alias.clone(),
                    public_key: self.node_id.clone(),
                }).unwrap(),
            },
        };

        Ok(Response::new(response))
    }
}

async fn handle_http_command(
    command: serde_json::Value,
    service: MyLittleService,
) -> Result<impl warp::Reply, warp::Rejection> {
    println!("Received HTTP command: {:?}", command);

    let command_str = command["command"].as_str().unwrap_or("").to_lowercase();

    let response = match command_str.as_str() {
        "start" => {
            let name = command["arguments"].get(0).and_then(|v| v.as_str()).unwrap_or("default");
            serde_json::json!({
                "status": "started",
                "message": format!("Started with name: {}", name)
            })
        },
        "stop" => {
            // Gracefully stop the LDK node
            if let Some(node) = service.node.lock().await.take() {
                println!("Stopping LDK node...");
                // Spawn a blocking task to stop the node
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = node.stop() {
                        eprintln!("Error stopping node: {}", e);
                    }
                });
            }
            
            // Signal shutdown to the servers
            let _ = service.shutdown_signal.send(());
            
            serde_json::json!({
                "status": "stopped",
                "message": "Node shutdown initiated"
            })
        },
        "getinfo" => serde_json::json!({
            "status": "info",
            "message": {
                "alias": service.alias,
                "node_id": service.node_id,
            }
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
    let alias = match load_alias()? {
        Some(saved_alias) => saved_alias,
        None => {
            let mut generator = Generator::default();
            let new_alias = generator.next().unwrap();
            save_alias(&new_alias)?;
            new_alias
        }
    };

    let (node, node_id) = make_node(&alias, 9735);
    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

    let service = MyLittleService {
        state: Arc::new(Mutex::new(String::new())),
        alias,
        node_id,
        node: Arc::new(Mutex::new(Some(node))),
        shutdown_signal: Arc::new(shutdown_tx),
    };
    
    let grpc_addr = "[::1]:50051".parse()?;
    let grpc_service = LittleServiceServer::new(service.clone());
    
    // Create a new receiver for gRPC server
    let mut grpc_shutdown_rx = service.shutdown_signal.subscribe();
    let grpc_server = Server::builder().add_service(grpc_service).serve_with_shutdown(
        grpc_addr,
        async move {
            grpc_shutdown_rx.recv().await.ok();
            println!("Shutting down gRPC server...");
        },
    );

    println!("gRPC server listening on {}", grpc_addr);

    let http_addr = ([127, 0, 0, 1], 3030);
    
    // Create a new clone for the HTTP routes
    let http_service = service.clone();
    let http_routes = warp::post()
        .and(warp::path("little"))
        .and(warp::path("api"))
        .and(warp::path("v1"))
        .and(warp::path("command"))
        .and(warp::body::json())
        .and(warp::any().map(move || http_service.clone()))
        .and_then(handle_http_command);

    // Create a new receiver for HTTP server
    let mut http_shutdown_rx = service.shutdown_signal.subscribe();
    let (_, http_server) = warp::serve(http_routes)
        .bind_with_graceful_shutdown(
            http_addr,
            async move {
                http_shutdown_rx.recv().await.ok();
                println!("Shutting down HTTP server...");
            },
        );

    println!("HTTP server listening on http://{:?}", http_addr);

    tokio::join!(
        grpc_server,
        http_server,
    );

    println!("Servers shut down successfully");
    Ok(())
}

fn make_node(alias: &str, port: u16) -> (ldk_node::Node, String) {
    let mut builder = Builder::new();
    builder.set_network(Network::Signet);
    builder.set_chain_source_esplora("https://mutinynet.ltbl.io/api".to_string(), None);
    builder.set_gossip_source_rgs("https://mutinynet.ltbl.io/snapshot".to_string());
    builder.set_storage_dir_path("./data".to_string());
    builder.set_listening_addresses(vec![format!("127.0.0.1:{}", port).parse().unwrap()]);

    let node = builder.build().unwrap();
    node.start().unwrap();

    let node_id = node.node_id().to_string();

    println!("Node alias: {}", alias);
    println!("Node public key: {}", node_id);

    (node, node_id)
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

