use clap::{Parser};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use warp::Filter;
use serde_json;
use little::little_service_server::{LittleService, LittleServiceServer};
use little::{CommandRequest, CommandResponse};
mod commands;
mod config;
use commands::{Command, GetAddressResponse, ListBalancesResponse, PeerDetailsResponse, StoredOfferDetails, ChannelDetailsResponse};
use config::{get_default_data_dir, load_config, save_config};
use ldk_node::Builder;
use ldk_node::bitcoin::Network;
use ldk_node::bitcoin::secp256k1::PublicKey;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::config::ChannelConfig;
use std::str::FromStr;
use std::path::PathBuf;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use shellexpand;
use rand::{thread_rng, Rng};
use std::error::Error;
use std::env;

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
    data_dir: PathBuf,
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

        let offers_path = self.data_dir.join("offers.json");
        let mut offers = if offers_path.exists() {
            let file = File::open(&offers_path)
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
            .open(&offers_path)
            .map_err(|e| format!("Failed to open offers file for writing: {}", e))?;

        serde_json::to_writer_pretty(file, &offers)
            .map_err(|e| format!("Failed to write offers: {}", e))?;

        Ok(())
    }

    async fn load_offers(&self) -> Result<Vec<StoredOfferDetails>, String> {
        let offers_path = self.data_dir.join("offers.json");
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
            Command::Start { name, lightning_port, grpc_port, http_port, data_dir: _ } => Ok(serde_json::json!({
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

                    println!("Current peer status: {}", if is_connected { "connected" } else { "not connected" });

                    // If not connected, establish connection first
                    if !is_connected {
                        println!("Attempting to connect to peer {}@{}", peer_pubkey, address);
                        match node.connect(peer_pubkey, peer_addr.clone(), true) {
                            Ok(_) => {
                                println!("Successfully connected to peer");
                                // Give it a moment to establish the connection
                                std::thread::sleep(std::time::Duration::from_secs(2));
                                
                                // Verify the connection was successful
                                let peers = node.list_peers();
                                if !peers.iter().any(|p| p.node_id == peer_pubkey) {
                                    return Err("Failed to establish connection with peer".to_string());
                                }
                            },
                            Err(e) => {
                                return Err(format!("Failed to connect to peer: {}", e));
                            }
                        }
                    }

                    // Create a default channel config
                    let channel_config = ldk_node::config::ChannelConfig {
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

                    match result {
                        Ok(channel_id) => {
                            println!("Successfully opened channel with ID: {}", channel_id.0);
                            Ok(serde_json::json!({
                                "channel_id": format!("{}", channel_id.0),  // Convert u128 to string
                                "peer_pubkey": peer_pubkey.to_string(),
                                "address": address,
                                "amount_sats": amount_sats,
                                "push_amount_sats": push_amount_sats,
                                "announced": announced,
                            }))
                        },
                        Err(e) => {
                            println!("Failed to open channel: {}", e);
                            Err(format!("Failed to open channel: {}", e))
                        }
                    }
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
            Command::Start { name, lightning_port, grpc_port, http_port, data_dir: None }
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
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Parse command line arguments
    let mut args = env::args().skip(1);
    let mut data_dir = None;
    let mut lightning_port = None;
    let mut grpc_port = None;
    let mut http_port = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--datadir" => {
                if let Some(dir) = args.next() {
                    data_dir = Some(PathBuf::from(shellexpand::tilde(&dir).into_owned()));
                }
            }
            "--lightningport" => {
                if let Some(port) = args.next() {
                    lightning_port = Some(port.parse::<u16>().unwrap());
                }
            }
            "--grpcport" => {
                if let Some(port) = args.next() {
                    grpc_port = Some(port.parse::<u16>().unwrap());
                }
            }
            "--httpport" => {
                if let Some(port) = args.next() {
                    http_port = Some(port.parse::<u16>().unwrap());
                }
            }
            _ => {}
        }
    }

    // Determine data directory
    let data_dir = if let Some(dir) = data_dir {
        dir
    } else {
        get_default_data_dir()
    };

    println!("Using data directory: {}", data_dir.display());

    // Create data directory if it doesn't exist
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
    }

    // Load or create config
    let config = load_config(&data_dir)?;

    // Create shutdown signal
    let (shutdown_sender, _) = tokio::sync::broadcast::channel(1);
    let shutdown_signal = Arc::new(shutdown_sender);

    // Start the node with the correct data directory
    let (node, node_id) = make_node(&config.node_alias, config.lightning_port, &data_dir);

    // Create service with the correct data directory
    let service = MyLittleService {
        state: Arc::new(Mutex::new("initialized".to_string())),
        alias: config.node_alias,
        node_id,
        node: Arc::new(Mutex::new(Some(node))),
        shutdown_signal: shutdown_signal.clone(),
        data_dir: data_dir.clone(),
    };

    // Start gRPC server
    let grpc_addr = format!("[::1]:{}", config.grpc_port).parse()?;
    let grpc_service = LittleServiceServer::new(service.clone());
    let grpc_server = Server::builder()
        .add_service(grpc_service)
        .serve(grpc_addr);

    println!("gRPC server listening on {}", grpc_addr);

    // Start HTTP server
    let http_addr = format!("127.0.0.1:{}", config.http_port);
    let http_routes = warp::post()
        .and(warp::path("little"))
        .and(warp::path("api"))
        .and(warp::path("v1"))
        .and(warp::path("command"))
        .and(warp::body::json())
        .and(warp::any().map(move || service.clone()))
        .and_then(handle_http_command);

    let http_shutdown_signal = shutdown_signal.clone();
    let (_, http_server) = warp::serve(http_routes)
        .bind_with_graceful_shutdown(
            http_addr.parse::<std::net::SocketAddr>()
                .map_err(|e| format!("Failed to parse HTTP address: {}", e))?,
            async move {
                let mut shutdown_receiver = http_shutdown_signal.subscribe();
                shutdown_receiver.recv().await.ok();
                println!("Shutting down HTTP server...");
            },
        );

    println!("HTTP server listening on http://{}", http_addr);

    // Start both servers
    let shutdown_signal_clone = shutdown_signal.clone();
    tokio::select! {
        _ = grpc_server => {},
        _ = http_server => {},
        _ = async move {
            // Wait for shutdown signal
            let mut shutdown_receiver = shutdown_signal_clone.subscribe();
            shutdown_receiver.recv().await.ok();
            println!("Shutting down servers...");
        } => {},
    }

    Ok(())
}

fn make_node(alias: &str, port: u16, data_dir: &Path) -> (ldk_node::Node, String) {
    let mut builder = Builder::new();
    builder.set_network(Network::Signet);
    builder.set_chain_source_esplora("https://mutinynet.ltbl.io/api".to_string(), None);
    builder.set_gossip_source_rgs("https://mutinynet.ltbl.io/snapshot".to_string());
    builder.set_storage_dir_path(data_dir.to_string_lossy().to_string());
    
    // Configure listening address
    let listening_address = format!("0.0.0.0:{}", port).parse().unwrap();
    builder.set_listening_addresses(vec![listening_address]);

    // Configure alias
    builder.set_node_alias(alias.to_string());
    
    let node = builder.build().unwrap();
    node.start().unwrap();

    // Wait a moment for the node to initialize
    std::thread::sleep(std::time::Duration::from_secs(1));

    let node_id = node.node_id().to_string();
    println!("Node started successfully:");
    println!("  Alias: {}", alias);
    println!("  Node ID: {}", node_id);
    println!("  Listening on port: {}", port);
    println!("  Data directory: {}", data_dir.display());

    (node, node_id)
}

