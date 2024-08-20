use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use warp::Filter;
use std::fs;

mod command_processor;
use command_processor::CommandProcessor;

#[tokio::main]
async fn main() {
    let processor = Arc::new(Mutex::new(CommandProcessor::new()));

    // Start the Unix socket listener for CLI
    let socket_path = "/tmp/little.sock";
    let socket_processor = processor.clone();
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
                    let proc = socket_processor.clone();
                    tokio::spawn(async move {
                        handle_cli_connection(stream, proc).await;
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
        .and(with_processor(processor.clone()))
        .and_then(handle_command);

    // Start the HTTP server
    warp::serve(command_route)
        .run(([127, 0, 0, 1], 3030))
        .await;
}

fn with_processor(
    processor: Arc<Mutex<CommandProcessor>>,
) -> impl Filter<Extract = (Arc<Mutex<CommandProcessor>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || processor.clone())
}

async fn handle_command(
    command: serde_json::Value,
    processor: Arc<Mutex<CommandProcessor>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let result = processor.lock().await.process_command(command).await;
    Ok(warp::reply::json(&result))
}

async fn handle_cli_connection(stream: tokio::net::UnixStream, processor: Arc<Mutex<CommandProcessor>>) {
    // Implement CLI command handling here
}