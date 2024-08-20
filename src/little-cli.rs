use tokio::net::UnixStream;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: little-cli <command> [subcommand] [arguments...]");
        std::process::exit(1);
    }

    let socket_path = "/tmp/little.sock";
    let stream = match UnixStream::connect(socket_path).await {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("Failed to connect to the server: {}", e);
            std::process::exit(1);
        }
    };

    // Serialize and send the command to the server
    let command = serde_json::json!({
        "command": args[1],
        "subcommand": args.get(2).cloned().unwrap_or_default(),
        "arguments": args.get(3..).unwrap_or(&[]).to_vec(),
    });

    // Send the command and receive the response
    // Implement the communication protocol here

    // Print the response
}