package cothink

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"sync"
	"sync/atomic"
	"time"
)

// TrustVariables holds the Rusty-Py-Coffee assertions
type TrustVariables struct {
	Nonce          string
	RustySignature []byte
	PolkitToken    string
	IsVerified     bool
}

// AgentNode represents a single "Fractal" leaf in the loom
type AgentNode struct {
	ID         int
	Depth      int
	ParentTTY  *os.File
	Children   []*AgentNode
	Trust      TrustVariables
	Metadata   map[string]string
	CancelFunc context.CancelFunc
}

// Orchestrator manages the Logarithmic Async Scalar
type Orchestrator struct {
	ID            int64 // Unique identifier for tracing concurrent orchestrators
	RootTTY       *os.File
	MaxChildren   int
	BaseDelay     time.Duration
	ActiveWorkers sync.WaitGroup
	Semaphore     *DynamicSemaphore
}

var globalOrchestratorSeq int64

// ensureID assigns a unique orchestrator ID atomically if one is not set.
func (orch *Orchestrator) ensureID() {
	if atomic.LoadInt64(&orch.ID) == 0 {
		atomic.CompareAndSwapInt64(&orch.ID, 0, atomic.AddInt64(&globalOrchestratorSeq, 1))
	}
}

// ZeroResiduals performs the "sync -f / && hash -r" rhythm
func ZeroResiduals() {
	fmt.Println("[Zeroing] Flushing buffers and clearing hash table...")
	exec.Command("sync", "-f", "/").Run()
	exec.Command("hash", "-r").Run()
}
