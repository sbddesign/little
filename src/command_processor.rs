use serde_json::Value;

pub struct CommandProcessor {
    // Add any necessary state here
}

impl CommandProcessor {
    pub fn new() -> Self {
        CommandProcessor {
            // Initialize state
        }
    }

    pub async fn process_command(&mut self, command: Value) -> Value {
        // Process the command and return the result
        // This method will be called by both CLI and REST handlers
        todo!()
    }
}
