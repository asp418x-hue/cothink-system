package cothink

import (
	"context"
	"sync"
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

func TestHistoryTracking(t *testing.T) {
	ClearHistory()

	// Perform parallel writes using multiple goroutines
	const numGoroutines = 50
	const writesPerGoroutine = 100
	var wg sync.WaitGroup

	for g := 0; g < numGoroutines; g++ {
		wg.Add(1)
		go func(gId int) {
			defer wg.Done()
			for i := 0; i < writesPerGoroutine; i++ {
				RecordEvent(int64(gId), i, "test_event", true, "detail_message")
			}
		}(g)
	}

	wg.Wait()

	history := GetHistory()
	expectedCount := numGoroutines * writesPerGoroutine
	if expectedCount > HistoryCapacity {
		expectedCount = HistoryCapacity
	}

	if len(history) != expectedCount {
		t.Errorf("Expected %d history entries, got %d", expectedCount, len(history))
	}

	// Verify all returned entries are valid and complete
	for _, entry := range history {
		if entry.Event != "test_event" || entry.Detail != "detail_message" {
			t.Errorf("Found corrupt history entry: %+v", entry)
		}
	}
}
