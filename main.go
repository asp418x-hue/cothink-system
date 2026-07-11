package main

import (
	"context"
	"fmt"
	"time"
	"cothink-system/cothink"
)

func main() {
	fmt.Println("Thinkspace Orchestrator Initialized on 5000M Bus")
	
	orch := &cothink.Orchestrator{
		MaxChildren: 16, // Spawning 16 agents to show complete ramp up/down cycle
		BaseDelay:   120 * time.Millisecond,
		Semaphore:   cothink.NewDynamicSemaphore(6), // Starts with a performance limit of 6
	}
	
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	defer cothink.ZeroResiduals() // Ensure cleanup on exit

	root := &cothink.AgentNode{ID: 0, Depth: 0}
	
	orch.ScalarSpawn(ctx, root)
}
