// Server skeleton - wraps CLI commands as HTTP/MCP endpoints

pub struct KeelServer {
    // TODO: Hold graph in memory, expose via HTTP/MCP
}

impl KeelServer {
    pub fn new() -> Self {
        KeelServer {}
    }
}

impl Default for KeelServer {
    fn default() -> Self {
        Self::new()
    }
}
