use std::io::Read;

use cothink_system::dispatch::{AgentDispatchConfig, dispatch_agents};
use cothink_system::history::{next_orchestrator_id, record};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let task_ids = std::env::args()
        .skip(1)
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|_| format!("invalid task ID '{arg}': expected a nonnegative integer"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if task_ids.is_empty() {
        return Err("usage: cothink-system <task-id> [task-id ...] (task context is read from stdin)".into());
    }

    let mut context = String::new();
    std::io::stdin().read_to_string(&mut context)?;

    let orchestrator_id = next_orchestrator_id();
    record(
        orchestrator_id,
        0,
        "orch_start",
        true,
        format!("task_count={}", task_ids.len()),
    );

    let results = dispatch_agents(
        orchestrator_id,
        &task_ids,
        &context,
        AgentDispatchConfig::python_agent(32),
    )
    .await?;

    let succeeded = results.iter().filter(|result| result.success).count();
    let failed = results.len() - succeeded;
    record(
        orchestrator_id,
        0,
        "orch_finish",
        failed == 0,
        format!("succeeded={succeeded}, failed={failed}"),
    );

    println!("{}", serde_json::to_string_pretty(&results)?);
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
