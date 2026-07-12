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
                let mut current_stdin = stdin;
                let len = nodes.len();

                for (i, node) in nodes.iter().enumerate() {
                    // Determine where this step outputs to
                    let current_stdout = if i == len - 1 {
                        // The final node in the pipeline routes directly to the macro stdout
                        stdout
                    } else {
                        // Intermediate nodes pipe into the next stage
                        Stdio::piped()
                    };

                    // Recursively execute the node—whether it's a Leaf or another Sub-Pipeline
                    let mut spawned_children = node.run(current_stdin, current_stdout)?;

                    // If an intermediate node spawned a process, steal its stdout descriptor 
                    // to allocate it sequentially as the stdin for the next iteration
                    if i < len - 1 {
                        if let Some(last_child) = spawned_children.last_mut() {
                            if let Some(raw_stdout) = last_child.stdout.take() {
                                current_stdin = Stdio::from(raw_stdout);
                            } else {
                                current_stdin = Stdio::null();
                            }
                        }
                    }

                    children.extend(spawned_children);
                }

                Ok(children)
            }
        }
    }
}
