#!/bin/bash
################################################################################
# cothink-shell Integration & Hooks Documentation
################################################################################

cat << 'EOF'

╔══════════════════════════════════════════════════════════════════════════════╗
║                  cothink-shell Hooks Integration Guide                      ║
╚══════════════════════════════════════════════════════════════════════════════╝

## What are Hooks?

Hooks are callback functions that execute at specific points in cothink-shell's
execution lifecycle. They allow you to:

- Monitor execution
- Customize behavior
- Integrate with external systems
- Collect metrics
- Validate inputs
- Handle errors
- Modify environment variables

## Hook Types

### Execution Hooks
- hook_pre_execute(cmd)        | Before executing a shell command
- hook_post_execute(exit, cmd) | After executing a shell command

### Passthrough Hooks
- hook_pre_passthrough(args...) | Before passing args to cothink-system binary
- hook_post_passthrough(exit, args...) | After binary execution completes

### Interactive Mode Hooks
- hook_pre_interactive()       | When entering interactive mode
- hook_post_interactive(exit)  | When exiting interactive mode
- hook_pre_command(cmd, args)  | Before each interactive command
- hook_post_command(cmd, args) | After each interactive command

### Login Shell Hooks
- hook_pre_login()             | Before login shell initialization
- hook_post_login(exit)        | After login shell exits

### Task Hooks
- hook_task_spawned(task_id, worker_id)  | Task spawned event
- hook_task_completed(id, exit, duration) | Task completion event
- hook_pre_run(agent_count)    | Before 'run' command
- hook_post_run(exit, count)   | After 'run' command

### Command Hooks
- hook_pre_bash_shell()        | Before 'shell' command drops to bash
- hook_post_bash_shell(exit)   | After returning from bash
- hook_status_display()        | When 'status' command displays info
- hook_pre_exit(cmd)           | Before 'exit' command

## Where to Define Hooks

Hooks are loaded from (in order):
1. ~/.cothink_hooks              (User-level, highest priority)
2. ~/.cothink_hooks.local        (Local machine-specific overrides)
3. ${COTHINK_HOME}/.cothink_hooks (Project-level defaults)

## Hook Function Signature

All hooks follow the pattern:

    hook_<hook_name>() {
        # Arguments are passed positionally
        # Return value is ignored (but exit code 1 could signal error)
        :
    }

Arguments vary by hook type - check the section for your specific hook.

## Example: Simple Logging Hook

~/.cothink_hooks:

    hook_pre_execute() {
        local cmd="$1"
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] Executing: $cmd" >> ~/.cothink_shell.log
    }

Usage: `cothink-shell -c "run 8"` will log the execution

## Example: Environment Setup Hook

~/.cothink_hooks:

    hook_pre_passthrough() {
        # Set up environment variables before binary execution
        export RUST_LOG="info"
        export COTHINK_PROFILE="production"
        
        # Could check resources, validate state, etc.
        local mem=$(free -b | awk 'NR==2 {print $7}')
        if [[ $mem -lt 536870912 ]]; then  # 512MB
            echo "WARNING: Low memory available" >&2
        fi
    }

Usage: `cothink-shell --workers 4` will set environment before running

## Example: Metrics Collection

~/.cothink_hooks:

    hook_post_execute() {
        local exit_code="$1"
        local cmd="$2"
        local timestamp=$(date +%s%3N)
        
        # Send metrics to external system
        echo "{\"cmd\":\"$cmd\",\"exit\":$exit_code,\"ts\":$timestamp}" | \
            curl -X POST --data-binary @- http://localhost:8086/write
    }

## Example: Task-Level Instrumentation

~/.cothink_hooks:

    hook_task_spawned() {
        local task_id="$1"
        local worker_id="$2"
        
        # Could integrate with distributed tracing
        # curl -X POST http://jaeger:14268/api/traces \
        #   -d '{"traceID":"'"$task_id"'","spanID":"'"$worker_id"'"}'
        
        echo "Task $task_id -> Worker $worker_id" >> ~/.cothink_tasks.log
    }

    hook_task_completed() {
        local task_id="$1"
        local exit_code="$2"
        local duration="$3"
        
        echo "Task $task_id done: ${duration}ms (exit: $exit_code)" >> ~/.cothink_tasks.log
    }

## Example: Interactive Mode Enhancement

~/.cothink_hooks:

    hook_pre_interactive() {
        clear
        echo "╔════════════════════════════════════════════════════════════╗"
        echo "║  cothink-shell $(date '+%Y-%m-%d %H:%M:%S')                    ║"
        echo "╚════════════════════════════════════════════════════════════╝"
        echo ""
    }

    hook_post_command() {
        local cmd="$1"
        
        # Could show command timing, resource usage, etc.
        # run 'time cothink-system' via hook
    }

## Integration with External Systems

### Prometheus Metrics

    hook_post_passthrough() {
        local exit="$1"
        cat <<METRICS | curl -X POST --data-binary @- \
            http://localhost:9091/metrics/job/cothink
# HELP cothink_execution_total Total executions
# TYPE cothink_execution_total counter
cothink_execution_total{exit_code="$exit"} 1
METRICS
    }

### Grafana Annotations

    hook_post_execute() {
        local exit="$1"
        local cmd="$2"
        
        curl -X POST http://localhost:3000/api/annotations \
          -H 'Content-Type: application/json' \
          -d "{
            \"text\": \"Command: $cmd (exit: $exit)\",
            \"tags\": [\"cothink\"],
            \"time\": $(date +%s)000
          }"
    }

### Slack Notifications

    hook_post_passthrough() {
        local exit="$1"
        shift
        local args=("$@")
        
        if [[ $exit -ne 0 ]]; then
            curl -X POST $SLACK_WEBHOOK \
              -H 'Content-Type: application/json' \
              -d "{\"text\": \"cothink-shell failed: exit code $exit\"}"
        fi
    }

## Advanced: Conditional Hooks

    hook_pre_passthrough() {
        # Only run if task file specified
        if [[ "$*" == *"--tasks"* ]]; then
            echo "Validating task file..."
            # validation logic
        fi
    }

## Advanced: Chaining Multiple Actions

    hook_post_execute() {
        local exit_code="$1"
        local cmd="$2"
        
        # Log to file
        echo "$cmd: $exit_code" >> ~/.cothink_history
        
        # Update database
        sqlite3 ~/.cothink_metrics.db \
            "INSERT INTO commands (cmd, exit_code, timestamp) VALUES ('$cmd', $exit_code, datetime('now'))"
        
        # Notify monitoring system
        curl -s -X POST http://localhost:8080/metrics -d "cmd=$cmd&exit=$exit_code" &
    }

## Debugging Hooks

To debug hooks, enable bash debugging in your hook file:

    set -x  # Enable debug output
    
    hook_pre_execute() {
        echo "DEBUG: Hook called with: $*"
        # your code here
    }
    
    set +x  # Disable debug output

Or use a separate debug hook:

    hook_debug_info() {
        echo "=== Debug Info ===" >&2
        echo "Shell: $SHELL" >&2
        echo "PWD: $PWD" >&2
        echo "Args: $*" >&2
    }

Then call it: `run_hook "debug_info" "arg1" "arg2"`

## Best Practices

1. **Keep hooks fast** - They execute in the shell's main thread
2. **Handle errors** - Use || true for non-critical operations
3. **Use background jobs** - Send long-running tasks to background (&)
4. **Environment isolation** - Don't pollute global environment
5. **Logging** - Use descriptive log messages with timestamps
6. **Documentation** - Comment your hooks for maintainability
7. **Version control** - Keep .cothink_hooks in version control
8. **Testing** - Test hooks before deploying to production

## Common Patterns

### Rate Limiting
    hook_pre_execute() {
        if [[ -f ~/.cothink_last_exec ]]; then
            local last=$(cat ~/.cothink_last_exec)
            local now=$(date +%s)
            if [[ $((now - last)) -lt 1 ]]; then
                echo "Rate limited" >&2
                return 1
            fi
        fi
        date +%s > ~/.cothink_last_exec
    }

### State Machine
    hook_post_execute() {
        local exit="$1"
        local state_file="$HOME/.cothink_state"
        
        if [[ $exit -eq 0 ]]; then
            echo "success" > "$state_file"
        else
            echo "failed" > "$state_file"
        fi
    }

### Audit Trail
    hook_pre_execute() {
        echo "[$(date -u +'%Y-%m-%dT%H:%M:%SZ')] AUDIT: $USER executing: $*" | \
            tee -a /var/log/cothink_audit.log
    }

## Troubleshooting

### Hooks not loading
- Check file exists and is readable: `ls -la ~/.cothink_hooks`
- Check for bash syntax errors: `bash -n ~/.cothink_hooks`
- Verify hook function names match pattern: `hook_<name>`

### Hooks not executing
- Check if sourcing is disabled: `shopt | grep sourcepath`
- Verify hooks are defined before cothink-shell runs
- Check exit codes: hooks that return non-zero may stop execution

### Performance issues
- Move slow operations to background: `expensive_operation &`
- Use timeouts: `timeout 5 slow_command || true`
- Profile with `time`: `( time hook_function ) 2>&1 | grep real`

═════════════════════════════════════════════════════════════════════════════════

For more information: cothink-shell --help
EOF
