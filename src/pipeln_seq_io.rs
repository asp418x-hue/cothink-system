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
