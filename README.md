# little

Little is a little lightning node. The goal, beyond just being a personal learning experiment, is to create a very opinionated lightning node that is designed for simply sending and receiving bitcoin. It exposes enough channel management to the user to get the job done, but assumes the existence of an LSP. Also uses minimal system resources.

## Devlopment

Run `cargo build` for dev or `cargo build --release` for prod.

Run the `littled` binary. This is the node.

Run `little-cli {command}` to interact with the node through CLI.

Run `curl -X POST -H "Content-Type: application/json" -d '{"command": "test", "subcommand": "subtest", "arguments": ["arg1", "arg2"]}' http://127.0.0.1:3030/little/api/v1/command` to interact with the node over REST API.
