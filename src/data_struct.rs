use std::collections::HashMap;
use std::process::{Child, Stdio};

// The core identifier for tracking background jobs
pub type JobId = u32;

// The tracking map for active background roots
pub struct JobTable {
    pub active_jobs: HashMap<JobId, Vec<Child>>,
}

// The fractal interface: individual leaves and macro-pipelines implement this exact trait
pub trait Execute {
    fn run(&self, stdin: Stdio, stdout: Stdio) -> Result<Vec<Child>, std::io::Error>;
}

// The self-similar tree structure
pub enum CommandNode {
    // A single, leaf-level system command (e.g., "grep")
    Leaf { program: String, args: Vec<String> },
    // A pipeline containing a sequential vector of nested CommandNodes
    Pipeline(Vec<CommandNode>),
}
