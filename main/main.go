package main

import (
	"fmt"
	"time"
	"cothink-system/cothink"
)

func main() {
	fmt.Println("Starting Thinkspace Orchestrator with JSON RPC Server...")

	// Run Orchestrator
	orch := &cothink.Orchestrator{
		MaxChildren: 16,
		BaseDelay:   120 * time.Millisecond,
		Semaphore:   cothink.NewDynamicSemaphore(6),
	}

	root := &cothink.AgentNode{ID: 0, Depth: 0}

	GlobalOrchestrator = orch
	GlobalRootNode = root

	// Initialize status
	UpdateStatus(OrchestratorStatus{
		MaxChildren: 16,
		BaseDelayMs: 120,
	})

	// Start TCP server in background on port 5000
	go StartJSONServer(5000)

	// Keep updating status dynamically
	go func() {
		for {
			time.Sleep(200 * time.Millisecond)
			agents := make([]Agent, 0)
			for _, child := range root.Children {
				agents = append(agents, Agent{
					ID:       child.ID,
					Depth:    child.Depth,
					Metadata: child.Metadata,
				})
			}
			UpdateStatus(OrchestratorStatus{
				ActiveWorkers: orch.Semaphore.GetActive(),
				MaxChildren:   orch.MaxChildren,
				BaseDelayMs:   120,
				Agents:        agents,
			})
		}
	}()

	fmt.Println("System initialized. JSON Server listening on port 5000. Ready for dashboard connections.")
	// Keep main alive
	select {}
}
