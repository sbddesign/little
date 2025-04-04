use clap::{Parser};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use warp::Filter;
use serde_json;
use little::little_service_server::{LittleService, LittleServiceServer};
use little::{CommandRequest, CommandResponse};
mod commands;
use commands::{Command, GetInfoResponse, GetAddressResponse, ListBalancesResponse, GetOfferResponse, PeerString, PeerDetailsResponse, StoredOfferDetails, ChannelDetailsResponse};
use ldk_node::Builder;
use ldk_node::bitcoin::Network;
use ldk_node::bitcoin::secp256k1::PublicKey;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::config::ChannelConfig;
use std::str::FromStr;
use names::Generator;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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

impl MyLittleService {
    async fn with_node<F, T>(&self, f: F) -> CommandResponse 
    where
        F: FnOnce(&ldk_node::Node) -> Result<T, Status>,
        T: serde::Serialize,
    {
        let node_lock = self.node.lock().await;
        if let Some(node) = node_lock.as_ref() {
            match f(node) {
                Ok(result) => CommandResponse {
                    status: "success".to_string(),
                    message: serde_json::to_string(&result).unwrap(),
                },
                Err(e) => CommandResponse {
                    status: "error".to_string(),
                    message: e.to_string(),
                }
            }
        } else {
            CommandResponse {
                status: "error".to_string(),
                message: "Node is not running".to_string(),
            }
        }
    }

    async fn store_offer(&self, offer_string: String, amount_sats: Option<u64>, description: String) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let offer = StoredOfferDetails {
            offer_string,
            amount_sats,
            description,
            created_at: now,
        };

        let offers_path = Path::new("./data/offers.json");
        let mut offers = if offers_path.exists() {
            let file = File::open(offers_path)
                .map_err(|e| format!("Failed to open offers file: {}", e))?;
            serde_json::from_reader::<_, Vec<StoredOfferDetails>>(file)
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        offers.push(offer);

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(offers_path)
            .map_err(|e| format!("Failed to open offers file for writing: {}", e))?;

        serde_json::to_writer_pretty(file, &offers)
            .map_err(|e| format!("Failed to write offers: {}", e))?;

        Ok(())
    }

    async fn load_offers(&self) -> Result<Vec<StoredOfferDetails>, String> {
        let offers_path = Path::new("./data/offers.json");
        if !offers_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(offers_path)
            .map_err(|e| format!("Failed to open offers file: {}", e))?;

        serde_json::from_reader(file)
            .map_err(|e| format!("Failed to read offers: {}", e))
    }

    async fn execute_unified_command(&self, command: Command) -> Result<serde_json::Value, String> {
        match command {
            Command::Start { name, lightning_port, grpc_port, http_port } => Ok(serde_json::json!({
                "name": name,
                "lightning_port": lightning_port,
                "grpc_port": grpc_port,
                "http_port": http_port,
            })),
            Command::Stop => {
                if let Some(node) = self.node.lock().await.take() {
                    if let Err(e) = node.stop() {
                        return Err(format!("Failed to stop node: {}", e));
                    }
                    let _ = self.shutdown_signal.send(());
                    Ok(serde_json::json!({}))
                } else {
                    Err("Node is not running".to_string())
                }
            },
            Command::GetInfo => Ok(serde_json::json!({
                "alias": self.alias,
                "node_id": self.node_id,
            })),
            Command::GetAddress => {
                let node_lock = self.node.lock().await;
                if let Some(node) = node_lock.as_ref() {
                    node.onchain_payment().new_address()
                        .map(|address| serde_json::json!({
                            "address": address.to_string()
                        }))
                        .map_err(|e| format!("Failed to get address: {}", e))
                } else {
                    Err("Node is not running".to_string())
                }
            },
            Command::ListBalances => {
                let node_lock = self.node.lock().await;
                if let Some(node) = node_lock.as_ref() {
                    let balances = node.list_balances();
                    Ok(serde_json::json!({
                        "total_onchain_balance_sats": balances.total_onchain_balance_sats,
                        "total_lightning_balance_sats": balances.total_lightning_balance_sats,
                        "spendable_onchain_balance_sats": balances.spendable_onchain_balance_sats,
                    }))
                } else {
                    Err("Node is not running".to_string())
                }
            },
            Command::GetOffer { amount_sats, description } => {
                let node_lock = self.node.lock().await;
                if let Some(node) = node_lock.as_ref() {
                    let desc = description.unwrap_or_else(|| "Payment request via Little".to_string());
                    println!("Creating offer with description: {}", desc);  // Debug log
                    let result = match amount_sats {
                        Some(sats) => {
                            println!("Fixed amount offer: {} sats", sats);  // Debug log
                            node.bolt12_payment().receive(
                                sats * 1000,     // convert sats to msats
                                &desc,           // description
                                Some(3600),      // expiry_secs (1 hour)
                                None,            // quantity
                            )
                        },
                        None => {
                            println!("Variable amount offer");  // Debug log
                            node.bolt12_payment().receive_variable_amount(
                                &desc,           // description
                                Some(3600),      // expiry_secs (1 hour)
                            )
                        },
                    };
                    
                    match result {
                        Ok(offer) => {
                            let offer_string = offer.to_string();
                            // Store the offer details
                            self.store_offer(
                                offer_string.clone(),
                                amount_sats,
                                desc,
                            ).await?;
                            
                            Ok(serde_json::json!({
                                "offer_string": offer_string
                            }))
                        },
                        Err(e) => Err(format!("Failed to create offer: {}", e))
                    }
                } else {
                    Err("Node is not running".to_string())
                }
            },
            Command::ListOffers => {
                let offers = self.load_offers().await?;
                Ok(serde_json::json!({
                    "offers": offers
                }))
            },
            Command::Connect { peer } => {
                let node_lock = self.node.lock().await;
                if let Some(node) = node_lock.as_ref() {
                    let node_id = PublicKey::from_str(&peer.node_id)
                        .map_err(|e| format!("Invalid node ID: {}", e))?;
                    let address = SocketAddress::from_str(&peer.address)
                        .map_err(|e| format!("Invalid address: {}", e))?;
                    
                    node.connect(node_id, address, true)
                        .map(|_| serde_json::json!({
                            "node_id": peer.node_id,
                            "address": peer.address,
                        }))
                        .map_err(|e| format!("Failed to connect to peer: {}", e))
                } else {
                    Err("Node is not running".to_string())
                }
            },
            Command::ListPeers => {
                let node_lock = self.node.lock().await;
                if let Some(node) = node_lock.as_ref() {
                    let peers = node.list_peers();
                    let peer_details: Vec<PeerDetailsResponse> = peers.into_iter()
                        .map(|peer| PeerDetailsResponse {
                            node_id: peer.node_id.to_string(),
                            address: peer.address.to_string(),
                            is_persisted: peer.is_persisted,
                        })
                        .collect();
                    
                    Ok(serde_json::json!({
                        "peers": peer_details
                    }))
                } else {
                    Err("Node is not running".to_string())
                }
            },
            Command::OpenChannel { 
                peer_pubkey, 
                address,
                amount_sats, 
                target_conf, 
                push_amount_sats,
                announced,
            } => {
                let node_lock = self.node.lock().await;
                if let Some(node) = node_lock.as_ref() {
                    let peer_pubkey = PublicKey::from_str(&peer_pubkey)
                        .map_err(|e| format!("Invalid peer public key: {}", e))?;
                    
                    let peer_addr = SocketAddress::from_str(&address)
                        .map_err(|e| format!("Invalid address: {}", e))?;

                    // Check if we're already connected to the peer
                    let peers = node.list_peers();
                    let is_connected = peers.iter().any(|p| p.node_id == peer_pubkey);

                    // If not connected, establish connection first
                    if !is_connected {
                        println!("Connecting to peer {}@{}", peer_pubkey, address);
                        if let Err(e) = node.connect(peer_pubkey, peer_addr.clone(), true) {
                            return Err(format!("Failed to connect to peer: {}", e));
                        }
                        // Give it a moment to establish the connection
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }

                    // Create a default channel config
                    let channel_config = ChannelConfig {
                        forwarding_fee_proportional_millionths: 1000, // 0.1%
                        forwarding_fee_base_msat: 1000,              // 1 sat
                        cltv_expiry_delta: 144,                      // ~24 hours
                        max_dust_htlc_exposure: ldk_node::config::MaxDustHTLCExposure::FixedLimit { limit_msat: 50000000 }, // 50k sats
                        force_close_avoidance_max_fee_satoshis: 1000,// 1000 sats
                        accept_underpaying_htlcs: false,             // Don't accept underpaying HTLCs
                    };

                    let conf_target = if target_conf > 0 { Some(target_conf as u64) } else { None };
                    let push_amount = if push_amount_sats > 0 { Some(push_amount_sats * 1000) } else { None }; // Convert to msats

                    println!("Opening {} channel with peer {}@{}", 
                        if announced { "announced" } else { "unannounced" },
                        peer_pubkey, address);

                    let result = if announced {
                        node.open_announced_channel(
                            peer_pubkey,
                            peer_addr,
                            amount_sats,
                            push_amount,
                            Some(channel_config),
                        )
                    } else {
                        node.open_channel(
                            peer_pubkey,
                            peer_addr,
                            amount_sats,
                            push_amount,
                            Some(channel_config),
                        )
                    };

                    result
                        .map(|channel_id| serde_json::json!({
                            "channel_id": format!("{}", channel_id.0),  // Convert u128 to string
                            "peer_pubkey": peer_pubkey.to_string(),
                            "address": address,
                            "amount_sats": amount_sats,
                            "push_amount_sats": push_amount_sats,
                            "announced": announced,
                        }))
                        .map_err(|e| format!("Failed to open channel: {}", e))
                } else {
                    Err("Node is not running".to_string())
                }
            },
            Command::ListChannels => {
                let node_lock = self.node.lock().await;
                if let Some(node) = node_lock.as_ref() {
                    let channels = node.list_channels();
                    let channel_details: Vec<ChannelDetailsResponse> = channels.into_iter()
                        .map(|channel| ChannelDetailsResponse {
                            channel_id: channel.channel_id.to_string(),
                            counterparty_node_id: channel.counterparty_node_id.to_string(),
                            funding_txo: channel.funding_txo.map(|txo| format!("{}:{}", txo.txid, txo.vout)),
                            channel_value_sats: channel.channel_value_sats,
                            unspendable_punishment_reserve: channel.unspendable_punishment_reserve,
                            user_channel_id: channel.user_channel_id.0.to_string(),
                            outbound_capacity_msat: channel.outbound_capacity_msat,
                            inbound_capacity_msat: channel.inbound_capacity_msat,
                            confirmations_required: channel.confirmations_required,
                            confirmations: channel.confirmations,
                            is_outbound: channel.is_outbound,
                            is_channel_ready: channel.is_channel_ready,
                            is_usable: channel.is_usable,
                            cltv_expiry_delta: channel.cltv_expiry_delta,
                        })
                        .collect();
                    
                    Ok(serde_json::json!({
                        "channels": channel_details
                    }))
                } else {
                    Err("Node is not running".to_string())
                }
            },
        }
    }
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
            Command::GetAddress => {
                self.with_node(|node| {
                    node.onchain_payment().new_address()
                        .map(|address| GetAddressResponse {
                            address: address.to_string(),
                        })
                        .map_err(|e| Status::internal(format!("Failed to get address: {}", e)))
                }).await
            },
            Command::ListBalances => {
                self.with_node(|node| {
                    let balances = node.list_balances();
                    Ok(ListBalancesResponse {
                        total_onchain_balance_sats: balances.total_onchain_balance_sats,
                        total_lightning_balance_sats: balances.total_lightning_balance_sats,
                        spendable_onchain_balance_sats: balances.spendable_onchain_balance_sats,
                    })
                }).await
            },
            _ => {
                match self.execute_unified_command(command).await {
                    Ok(result) => CommandResponse {
                        status: "success".to_string(),
                        message: serde_json::to_string(&result).unwrap(),
                    },
                    Err(e) => CommandResponse {
                        status: "error".to_string(),
                        message: e,
                    },
                }
            }
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
    let command = match command_str.as_str() {
        "start" => {
            let name = command["arguments"].get(0).and_then(|v| v.as_str()).map(|s| s.to_string());
            let lightning_port = command["arguments"].get("lightning_port")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .unwrap_or(9735);
            let grpc_port = command["arguments"].get("grpc_port")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .unwrap_or(50051);
            let http_port = command["arguments"].get("http_port")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .unwrap_or(3030);
            Command::Start { name, lightning_port, grpc_port, http_port }
        },
        "stop" => Command::Stop,
        "getinfo" => Command::GetInfo,
        "getaddress" => Command::GetAddress,
        "listbalances" => Command::ListBalances,
        "getoffer" => {
            let amount_sats = command["arguments"].get("amount_sats")
                .and_then(|v| v.as_u64());
            let description = command["arguments"].get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Command::GetOffer { amount_sats, description }
        },
        "connect" => {
            let peer_str = command["arguments"].get("peer")
                .and_then(|v| v.as_str())
                .ok_or_else(|| warp::reject::custom(InvalidCommand("Missing peer parameter".to_string())))?;
            let peer = commands::parse_peer_string(peer_str)
                .map_err(|e| warp::reject::custom(InvalidCommand(e)))?;
            Command::Connect { peer }
        },
        "listpeers" => Command::ListPeers,
        "listoffers" => Command::ListOffers,
        "openchannel" => {
            let args = &command["arguments"];
            let peer_pubkey = args.get("peer_pubkey")
                .and_then(|v| v.as_str())
                .ok_or_else(|| warp::reject::custom(InvalidCommand("Missing peer_pubkey parameter".to_string())))?
                .to_string();
            
            let amount_sats = args.get("amount_sats")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| warp::reject::custom(InvalidCommand("Missing amount_sats parameter".to_string())))?;

            let address = args.get("address")
                .and_then(|v| v.as_str())
                .ok_or_else(|| warp::reject::custom(InvalidCommand("Missing address parameter".to_string())))?
                .to_string();

            Command::OpenChannel {
                peer_pubkey,
                address,
                amount_sats,
                target_conf: args.get("target_conf")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
                    .unwrap_or(6),
                push_amount_sats: args.get("push_amount_sats")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                announced: args.get("announced")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            }
        },
        "listchannels" => Command::ListChannels,
        _ => return Err(warp::reject::custom(InvalidCommand(format!("Unknown command: {}", command_str))))
    };

    let result = service.execute_unified_command(command).await;
    let response = match result {
        Ok(message) => serde_json::json!({
            "status": "success",
            "message": message
        }),
        Err(e) => serde_json::json!({
            "status": "error",
            "message": e
        }),
    };

    Ok(warp::reply::json(&response))
}

#[derive(Debug)]
struct InvalidCommand(String);

impl warp::reject::Reject for InvalidCommand {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments first
    let cli = Cli::parse();
    
    // Extract port configurations if starting the node
    let (lightning_port, grpc_port, http_port) = match &cli.command {
        Command::Start { lightning_port, grpc_port, http_port, .. } => (*lightning_port, *grpc_port, *http_port),
        _ => (9735, 50051, 3030), // Default ports for other commands
    };

    let alias = match load_alias()? {
        Some(saved_alias) => saved_alias,
        None => {
            let mut generator = Generator::default();
            let new_alias = generator.next().unwrap();
            save_alias(&new_alias)?;
            new_alias
        }
    };

    let (node, node_id) = make_node(&alias, lightning_port);
    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

    let service = MyLittleService {
        state: Arc::new(Mutex::new(String::new())),
        alias,
        node_id,
        node: Arc::new(Mutex::new(Some(node))),
        shutdown_signal: Arc::new(shutdown_tx),
    };
    
    let grpc_addr = format!("[::1]:{}", grpc_port).parse()?;
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

    let http_addr = ([127, 0, 0, 1], http_port);
    
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
    builder.set_listening_addresses(vec![format!("0.0.0.0:{}", port).parse().unwrap()]);

    let node = builder.build().unwrap();
    node.start().unwrap();

    // Wait a moment for the node to initialize
    std::thread::sleep(std::time::Duration::from_secs(1));

    let node_id = node.node_id().to_string();
    println!("Node started successfully:");
    println!("  Alias: {}", alias);
    println!("  Node ID: {}", node_id);
    println!("  Listening on port: {}", port);

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

