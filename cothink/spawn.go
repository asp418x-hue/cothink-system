package cothink

import (
	"context"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"sync"
	"time"
)

// ScalarSpawn spawns agents concurrently up to MaxChildren, using the semaphore and base delay.
// It executes a pipeline of two separate binaries (perl and rpmlua).
func (orch *Orchestrator) ScalarSpawn(ctx context.Context, root *AgentNode) {
	orch.ensureID()
	RecordEvent(orch.ID, 0, "orch_start", true, "MaxChildren="+strconv.Itoa(orch.MaxChildren))

	var wg sync.WaitGroup
	var mu sync.Mutex

	for i := 1; i <= orch.MaxChildren; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()

			// Delay spawn based on ID and base delay to simulate ramp up
			time.Sleep(time.Duration(id) * orch.BaseDelay)

			// Acquire slot from Semaphore if present
			if orch.Semaphore != nil {
				orch.Semaphore.Acquire()
				defer orch.Semaphore.Release()
			}

			// Check context cancellation before running command
			select {
			case <-ctx.Done():
				RecordEvent(orch.ID, id, "subagent_cancel", false, "Context cancelled before execution")
				return
			default:
			}

			RecordEvent(orch.ID, id, "subagent_start", true, "")

			// 1. Run Perl preprocessor binary
			RecordEvent(orch.ID, id, "subagent_perl_start", true, "")
			perlCmd := exec.CommandContext(ctx, "perl", "agent.pl", strconv.Itoa(id))
			perlOut, err := perlCmd.Output()
			if err != nil {
				fmt.Printf("[Orchestrator] Perl agent %d failed to execute: %v\n", id, err)
				RecordEvent(orch.ID, id, "subagent_perl_fail", false, err.Error())
				diag := queryFailureDiagnostics("sda")
				RecordEvent(orch.ID, id, "subagent_fail_query", false, diag)
				RecordEvent(orch.ID, id, "subagent_fail", false, "Perl preprocessor failed: "+err.Error())
				return
			}
			RecordEvent(orch.ID, id, "subagent_perl_success", true, "bytes: "+strconv.Itoa(len(perlOut)))

			// 2. Run Lua classifier binary
			RecordEvent(orch.ID, id, "subagent_lua_start", true, "")
			luaCmd := exec.CommandContext(ctx, "/usr/bin/rpmlua", "agent.lua")
			
			// Pipe Perl's output to Lua's stdin
			stdinPipe, err := luaCmd.StdinPipe()
			if err != nil {
				fmt.Printf("[Orchestrator] Stdin pipe failed: %v\n", err)
				RecordEvent(orch.ID, id, "subagent_lua_fail", false, "stdin pipe: "+err.Error())
				diag := queryFailureDiagnostics("sda")
				RecordEvent(orch.ID, id, "subagent_fail_query", false, diag)
				RecordEvent(orch.ID, id, "subagent_fail", false, "Lua stdin pipe failed")
				return
			}
			
			var luaOutBuf strings.Builder
			luaCmd.Stdout = &luaOutBuf
			
			if err := luaCmd.Start(); err != nil {
				fmt.Printf("[Orchestrator] Lua agent failed to start: %v\n", err)
				RecordEvent(orch.ID, id, "subagent_lua_fail", false, "start: "+err.Error())
				diag := queryFailureDiagnostics("sda")
				RecordEvent(orch.ID, id, "subagent_fail_query", false, diag)
				RecordEvent(orch.ID, id, "subagent_fail", false, "Lua start failed: "+err.Error())
				return
			}
			
			// Write Perl output to Lua and close the pipe
			stdinPipe.Write(perlOut)
			stdinPipe.Close()
			
			if err := luaCmd.Wait(); err != nil {
				fmt.Printf("[Orchestrator] Lua agent failed to finish: %v\n", err)
				RecordEvent(orch.ID, id, "subagent_lua_fail", false, "wait: "+err.Error())
				diag := queryFailureDiagnostics("sda")
				RecordEvent(orch.ID, id, "subagent_fail_query", false, diag)
				RecordEvent(orch.ID, id, "subagent_fail", false, "Lua execution failed: "+err.Error())
				return
			}

			output := luaOutBuf.String()
			RecordEvent(orch.ID, id, "subagent_lua_success", true, "output len: "+strconv.Itoa(len(output)))
			RecordEvent(orch.ID, id, "subagent_success", true, "Completed successfully")

			// Create child node
			child := &AgentNode{
				ID:    id,
				Depth: root.Depth + 1,
				Metadata: map[string]string{
					"output": output,
				},
			}

			// Append to root's Children slice safely
			mu.Lock()
			root.Children = append(root.Children, child)
			mu.Unlock()
		}(i)
	}

	wg.Wait()
	RecordEvent(orch.ID, 0, "orch_finish", true, "Spawned children count: "+strconv.Itoa(len(root.Children)))
}

// queryFailureDiagnostics gathers system metrics on failure in-stride, without throttling first.
func queryFailureDiagnostics(device string) string {
	temp, freq, diskHealth := QuerySystemMetrics(device)
	return fmt.Sprintf("diagnostics - Temp: %.1fC | CPU Freq: %dMHz | Disk Status: %s", temp, freq, diskHealth)
}
