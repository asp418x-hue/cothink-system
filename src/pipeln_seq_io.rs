use crate::data_struct::{CommandNode, Execute};
use std::process::{Child, Command, Stdio};

impl Execute for CommandNode {
    fn run(&self, stdin: Stdio, stdout: Stdio) -> Result<Vec<Child>, std::io::Error> {
        match self {
            CommandNode::Leaf { program, args } => {
                // Spawn the leaf process with the passed-down I/O handles
                let child = Command::new(program)
                    .args(args)
                    .stdin(stdin)
                    .stdout(stdout)
                    .spawn()?;
                Ok(vec![child])
            }
            CommandNode::Pipeline(nodes) => {
                let mut children = Vec::new();
                let mut stdout_opt = Some(stdout);
                let mut stdin_opt = Some(stdin);
                let len = nodes.len();

                for (i, node) in nodes.iter().enumerate() {
                    // Determine where this step outputs to
                    let current_stdout = if i == len - 1 {
                        // The final node in the pipeline routes directly to the macro stdout
                        stdout_opt.take().expect("stdout must be available for the final stage")
                    } else {
                        // Intermediate nodes pipe into the next stage
                        Stdio::piped()
                    };

                    // Extract the stdin for this step
                    let current_stdin = stdin_opt.take().unwrap_or_else(Stdio::null);

                    // Recursively execute the node—whether it's a Leaf or another Sub-Pipeline
                    let mut spawned_children = node.run(current_stdin, current_stdout)?;

                    // If an intermediate node spawned a process, steal its stdout descriptor 
                    // to allocate it sequentially as the stdin for the next iteration
                    if i < len - 1 {
                        let mut next_stdin = None;
                        if let Some(last_child) = spawned_children.last_mut() {
                            if let Some(raw_stdout) = last_child.stdout.take() {
                                next_stdin = Some(Stdio::from(raw_stdout));
                            }
                        }
                        stdin_opt = Some(next_stdin.unwrap_or_else(Stdio::null));
                    }

                    children.extend(spawned_children);
                }

                Ok(children)
            }
        }
    }
}

// ============================================================================
// Secure Token Exchange Patterns (Main Orchestrator & Child processes)
// ============================================================================

use tokio::process::Command as TokioCommand;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;

/// Pattern 1: Spawning a child with an environment variable set specifically for it.
/// This prevents credentials from showing up in command-line arguments (visible in 'ps').
pub async fn spawn_subagent_with_env(
    program: &str,
    args: &[&str],
    auth_token: &str,
) -> Result<(), std::io::Error> {
    let mut child = TokioCommand::new(program)
        .args(args)
        .env("AUTH_TOKEN", auth_token) // Injected securely to the child
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;

    child.wait().await?;
    Ok(())
}

/// Pattern 2: Piping credentials directly into the child process's standard input (stdin)
/// at startup, then immediately closing the pipe. Extremely secure.
pub async fn spawn_subagent_with_stdin(
    program: &str,
    args: &[&str],
    auth_token: &str,
) -> Result<(), std::io::Error> {
    let mut child = TokioCommand::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(auth_token.as_bytes()).await?;
        stdin.flush().await?;
        // stdin gets closed here, signaling EOF to the child
    }

    child.wait().await?;
    Ok(())
}

/// Pattern 3: Using a Unix Domain Socket (UDS) for dynamic or bidirectional token exchange
/// (e.g., token rotation, handshakes) between parent and child.
pub async fn run_orchestrator_uds(
    socket_path: &str,
    program: &str,
    args: &[&str],
    auth_token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Bind to the unix domain socket path
    let listener = UnixListener::bind(socket_path)?;

    // Spawn the child subagent, passing the socket path as an argument
    let mut child_args: Vec<String> = args.iter().map(|&s| s.to_string()).collect();
    child_args.push(socket_path.to_string());
    
    TokioCommand::new(program)
        .args(child_args)
        .spawn()?;

    // Wait for the child to connect and stream the token
    if let Ok((mut stream, _)) = listener.accept().await {
        stream.write_all(auth_token.as_bytes()).await?;
        stream.flush().await?;
    }

    Ok(())
}
