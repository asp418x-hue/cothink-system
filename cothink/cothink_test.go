package cothink

import (
	"context"
	"testing"
	"time"
)

func TestZeroResiduals(t *testing.T) {
	// Ensure this executes without crashing
	ZeroResiduals()
}

func TestScalarSpawn(t *testing.T) {
	orch := &Orchestrator{
		MaxChildren: 2,
		BaseDelay:   10 * time.Millisecond,
		Semaphore:   NewDynamicSemaphore(2),
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	root := &AgentNode{ID: 0, Depth: 0}

	// Note: We need './classifier' binary to exist in the current working directory during test.
	// We'll verify this by running the test from the parent directory /root/cothink-system
	orch.ScalarSpawn(ctx, root)

	if len(root.Children) != 2 {
		t.Errorf("Expected 2 child nodes, got %d", len(root.Children))
	}

	for _, child := range root.Children {
		if child.Depth != 1 {
			t.Errorf("Expected child depth 1, got %d", child.Depth)
		}
	}
}
