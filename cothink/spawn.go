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
				return
			default:
			}

			// 1. Run Perl preprocessor binary
			perlCmd := exec.CommandContext(ctx, "perl", "agent.pl", strconv.Itoa(id))
			perlOut, err := perlCmd.Output()
			if err != nil {
				fmt.Printf("[Orchestrator] Perl agent %d failed to execute: %v\n", id, err)
				return
			}

			// 2. Run Lua classifier binary
			luaCmd := exec.CommandContext(ctx, "/usr/bin/rpmlua", "agent.lua")
			
			// Pipe Perl's output to Lua's stdin
			stdinPipe, err := luaCmd.StdinPipe()
			if err != nil {
				fmt.Printf("[Orchestrator] Stdin pipe failed: %v\n", err)
				return
			}
			
			var luaOutBuf strings.Builder
			luaCmd.Stdout = &luaOutBuf
			
			if err := luaCmd.Start(); err != nil {
				fmt.Printf("[Orchestrator] Lua agent failed to start: %v\n", err)
				return
			}
			
			// Write Perl output to Lua and close the pipe
			stdinPipe.Write(perlOut)
			stdinPipe.Close()
			
			if err := luaCmd.Wait(); err != nil {
				fmt.Printf("[Orchestrator] Lua agent failed to finish: %v\n", err)
				return
			}

			output := luaOutBuf.String()

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
}
