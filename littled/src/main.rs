use clap::{Parser, Subcommand};
use std::fs;
use tokio::net::UnixListener;
use warp::Filter;

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
    // Start the Unix socket listener for CLI
    let socket_path = "/tmp/little.sock";
    tokio::spawn(async move {
        // Remove the old socket file if it exists
        if fs::metadata(socket_path).is_ok() {
            fs::remove_file(socket_path).unwrap_or_else(|e| {
                eprintln!("Failed to remove old socket file: {}", e);
                std::process::exit(1);
            });
        }

        let listener = UnixListener::bind(socket_path).unwrap_or_else(|e| {
            eprintln!("Failed to bind to socket: {}", e);
            std::process::exit(1);
        });
        println!("Server listening on {}", socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        handle_cli_connection(stream).await;
                    });
                }
                Err(e) => {
                    eprintln!("Failed to accept connection: {}", e);
                }
            }
        }
    });

    // Set up REST API routes
    let api = warp::path("little")
        .and(warp::path("api"))
        .and(warp::path("v1"))
        .and(warp::path("command"));

    let command_route = api
        .and(warp::post())
        .and(warp::body::json())
        .and_then(handle_command);

    // Start the HTTP server
    println!("Server is running on http://127.0.0.1:3030");
    warp::serve(command_route).run(([127, 0, 0, 1], 3030)).await;
}

async fn handle_command(command: serde_json::Value) -> Result<impl warp::Reply, warp::Rejection> {
    println!("Received command: {:?}", command);
    let result = serde_json::json!({"status": "received", "command": command});
    Ok(warp::reply::json(&result))
}

async fn handle_cli_connection(mut stream: tokio::net::UnixStream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buffer = [0; 1024];
    match stream.read(&mut buffer).await {
        Ok(n) => {
            let received = String::from_utf8_lossy(&buffer[..n]);
            println!("Received from CLI: {}", received);
            let response = "Command received\n";
            if let Err(e) = stream.write_all(response.as_bytes()).await {
                eprintln!("Failed to write to stream: {}", e);
            }
        }
        Err(e) => eprintln!("Failed to read from stream: {}", e),
    }
}
