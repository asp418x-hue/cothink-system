use serde_json::Value;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::history::record;

#[derive(Clone, Debug)]
pub struct AgentDispatchConfig {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub max_concurrent: usize,
    pub timeout: Duration,
    pub time_dilation_factor: f64,
    pub max_subjective_time: Duration,
}

impl AgentDispatchConfig {
    pub fn python_agent(max_concurrent: usize) -> Self {
        Self {
            program: OsString::from("python3"),
            args: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("agent.py")
                .into_os_string()],
            max_concurrent,
            timeout: Duration::from_secs(120),
            time_dilation_factor: 1.0,
            max_subjective_time: Duration::from_secs(600),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SubjectiveTiming {
    pub objective_elapsed_ns: u64,
    pub subjective_elapsed_ns: u64,
    pub dilation_factor: f64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AgentDispatchResult {
    pub task_id: usize,
    pub success: bool,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub timing: SubjectiveTiming,
}

/// Dispatches task IDs to child agent processes and records their outcomes.
/// Each agent receives the shared task context on stdin and must return one
/// JSON object containing its task ID and a `status` of `success` or `error`.
pub async fn dispatch_agents(
    orchestrator_id: u64,
    task_ids: &[usize],
    context: &str,
    config: AgentDispatchConfig,
) -> Result<Vec<AgentDispatchResult>, String> {
    if config.max_concurrent == 0 {
        return Err("max_concurrent must be greater than zero".to_string());
    }
    if !config.time_dilation_factor.is_finite() || config.time_dilation_factor <= 0.0 {
        return Err("time_dilation_factor must be finite and greater than zero".to_string());
    }
    if config.max_subjective_time.is_zero() {
        return Err("max_subjective_time must be greater than zero".to_string());
    }

    let mut seen = HashSet::with_capacity(task_ids.len());
    if task_ids.iter().any(|task_id| !seen.insert(*task_id)) {
        return Err("task IDs must be unique within a dispatch".to_string());
    }

    let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
    let mut tasks = JoinSet::new();

    for &task_id in task_ids {
        let config = config.clone();
        let context = context.to_string();
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let permit = match semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(error) => {
                    return failed_result(
                        task_id,
                        format!("dispatch semaphore closed: {error}"),
                        subjective_timing(
                            Duration::ZERO,
                            task_id,
                            config.time_dilation_factor,
                            config.max_subjective_time,
                        ),
                    );
                }
            };

            record(
                orchestrator_id,
                task_id,
                "subagent_start",
                true,
                String::new(),
            );
            let objective_start = Instant::now();
            let result = run_agent(task_id, &context, &config).await;
            let timing = subjective_timing(
                objective_start.elapsed(),
                task_id,
                config.time_dilation_factor,
                config.max_subjective_time,
            );
            drop(permit);

            match result {
                Ok(output) => {
                    record(
                        orchestrator_id,
                        task_id,
                        "subagent_success",
                        true,
                        output.to_string(),
                    );
                    AgentDispatchResult {
                        task_id,
                        success: true,
                        output: Some(output),
                        error: None,
                        timing,
                    }
                }
                Err(error) => {
                    record(
                        orchestrator_id,
                        task_id,
                        "subagent_fail",
                        false,
                        error.clone(),
                    );
                    failed_result(task_id, error, timing)
                }
            }
        });
    }

    let mut results = Vec::with_capacity(task_ids.len());
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(result) => results.push(result),
            Err(error) => return Err(format!("agent task panicked or was cancelled: {error}")),
        }
    }
    results.sort_by_key(|result| result.task_id);
    Ok(results)
}

async fn run_agent(
    task_id: usize,
    context: &str,
    config: &AgentDispatchConfig,
) -> Result<Value, String> {
    let mut command = Command::new(&config.program);
    command
        .args(&config.args)
        .arg(task_id.to_string())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start agent process: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(context.as_bytes())
            .await
            .map_err(|error| format!("failed to send task context: {error}"))?;
    }

    let output = tokio::time::timeout(config.timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("agent task {task_id} exceeded {:?} timeout", config.timeout))?
        .map_err(|error| format!("failed waiting for agent process: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("agent process exited with status {:?}", output.status.code())
        } else {
            format!("agent process failed: {stderr}")
        });
    }

    let response: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("agent returned invalid JSON: {error}"))?;
    if response.get("task_id").and_then(Value::as_u64) != Some(task_id as u64) {
        return Err(format!("agent response task_id did not match {task_id}"));
    }

    match response.get("status").and_then(Value::as_str) {
        Some("success") => Ok(response),
        Some("error") => Err(response
            .get("error_msg")
            .and_then(Value::as_str)
            .or_else(|| response.get("error").and_then(Value::as_str))
            .unwrap_or("agent reported an error")
            .to_string()),
        _ => Err("agent response must include status 'success' or 'error'".to_string()),
    }
}

fn failed_result(
    task_id: usize,
    error: String,
    timing: SubjectiveTiming,
) -> AgentDispatchResult {
    AgentDispatchResult {
        task_id,
        success: false,
        output: None,
        error: Some(error),
        timing,
    }
}

fn subjective_timing(
    objective: Duration,
    task_id: usize,
    dilation_factor: f64,
    max_subjective_time: Duration,
) -> SubjectiveTiming {
    let max_subjective_ns = max_subjective_time.as_nanos().min(u64::MAX as u128) as u64;
    let agent_config = subjective_time_expansion::AgentConfig::new(format!("agent-{task_id}"))
        .with_dilation_factor(dilation_factor)
        .with_max_subjective_time(max_subjective_ns);
    let objective_elapsed_ns = objective.as_nanos().min(u64::MAX as u128) as u64;
    let subjective_elapsed_ns = ((objective_elapsed_ns as f64
        * agent_config.dilation_factor)
        .min(agent_config.max_subjective_ns as f64)) as u64;

    SubjectiveTiming {
        objective_elapsed_ns,
        subjective_elapsed_ns,
        dilation_factor: agent_config.dilation_factor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_config(script: &str, max_concurrent: usize) -> AgentDispatchConfig {
        AgentDispatchConfig {
            program: OsString::from("sh"),
            args: vec![
                OsString::from("-c"),
                OsString::from(script),
                OsString::from("agent-test"),
            ],
            max_concurrent,
            timeout: Duration::from_secs(5),
            time_dilation_factor: 1.0,
            max_subjective_time: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn dispatches_agents_and_returns_validated_results() {
        let config = shell_config(
            "context=$(cat); printf '{\"task_id\":%s,\"status\":\"success\",\"resolution\":\"%s\"}\\n' \"$1\" \"$context\"",
            2,
        );
        let results = dispatch_agents(9001, &[7, 3], "inspection complete", config)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].task_id, 3);
        assert_eq!(results[1].task_id, 7);
        assert!(results.iter().all(|result| result.success));
        assert_eq!(
            results[0].output.as_ref().unwrap()["resolution"],
            "inspection complete"
        );
    }

    #[tokio::test]
    async fn reports_agent_failure_as_a_task_result() {
        let config = shell_config(
            "printf '{\"task_id\":%s,\"status\":\"error\",\"error_msg\":\"denied\"}' \"$1\"",
            1,
        );
        let results = dispatch_agents(9002, &[4], "", config).await.unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert_eq!(results[0].error.as_deref(), Some("denied"));
    }

    #[tokio::test]
    async fn rejects_zero_concurrency_and_duplicate_task_ids() {
        let mut config = shell_config("exit 0", 0);
        assert!(dispatch_agents(9003, &[1], "", config.clone())
            .await
            .is_err());

        config.max_concurrent = 1;
        assert!(dispatch_agents(9003, &[1, 1], "", config)
            .await
            .is_err());
    }

    #[test]
    fn subjective_time_expands_and_contracts_with_the_macro_scale() {
        let expanded = subjective_timing(
            Duration::from_millis(8),
            1,
            2.5,
            Duration::from_secs(1),
        );
        let contracted = subjective_timing(
            Duration::from_millis(8),
            2,
            0.5,
            Duration::from_secs(1),
        );

        assert_eq!(expanded.subjective_elapsed_ns, 20_000_000);
        assert_eq!(contracted.subjective_elapsed_ns, 4_000_000);
    }

    #[test]
    fn subjective_time_obeys_the_per_agent_cap() {
        let timing = subjective_timing(
            Duration::from_secs(4),
            3,
            10.0,
            Duration::from_secs(5),
        );

        assert_eq!(timing.subjective_elapsed_ns, 5_000_000_000);
    }
}
