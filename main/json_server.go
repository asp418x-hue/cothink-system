package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"strconv"
	"sync"
	"cothink-system/cothink"
)

type OrchestratorStatus struct {
	ActiveWorkers int      `json:"active_workers"`
	MaxChildren   int      `json:"max_children"`
	BaseDelayMs   int64    `json:"base_delay_ms"`
	Agents        []Agent  `json:"agents"`
}

type Agent struct {
	ID       int               `json:"id"`
	Depth    int               `json:"depth"`
	Metadata map[string]string `json:"metadata"`
}

var (
	statusMutex        sync.RWMutex
	currentStatus      OrchestratorStatus
	GlobalOrchestrator *cothink.Orchestrator
	GlobalRootNode     *cothink.AgentNode
)

func StartJSONServer(port int) {
	listener, err := net.Listen("tcp", fmt.Sprintf(":%d", port))
	if err != nil {
		fmt.Printf("[JSON Server] Failed to listen on port %d: %v\n", port, err)
		return
	}
	defer listener.Close()
	fmt.Printf("[JSON Server] Listening on port %d...\n", port)

	for {
		conn, err := listener.Accept()
		if err != nil {
			continue
		}
		go handleConnection(conn)
	}
}

func handleConnection(conn net.Conn) {
	defer conn.Close()
	decoder := json.NewDecoder(conn)
	encoder := json.NewEncoder(conn)

	var request map[string]interface{}
	if err := decoder.Decode(&request); err != nil {
		return
	}

	action, ok := request["action"].(string)
	if !ok {
		return
	}

	response := map[string]string{"status": "success"}

	switch action {
	case "get_status":
		statusMutex.RLock()
		encoder.Encode(currentStatus)
		statusMutex.RUnlock()
		return

	case "set_concurrency":
		if limitVal, ok := request["limit"]; ok {
			var limit int
			switch v := limitVal.(type) {
			case float64:
				limit = int(v)
			case string:
				limit, _ = strconv.Atoi(v)
			}
			if limit > 0 && GlobalOrchestrator != nil {
				GlobalOrchestrator.Semaphore.SetLimit(limit)
				fmt.Printf("[JSON Server] Semaphore limit updated to %d\n", limit)
			}
		}

	case "zero_residuals":
		cothink.ZeroResiduals()

	case "trigger_sweep":
		if GlobalOrchestrator != nil && GlobalRootNode != nil {
			fmt.Println("[JSON Server] Triggering async optimization sweep...")
			// Reset children for new sweep
			GlobalRootNode.Children = make([]*cothink.AgentNode, 0)
			go func() {
				ctx, cancel := context.WithCancel(context.Background())
				defer cancel()
				GlobalOrchestrator.ScalarSpawn(ctx, GlobalRootNode)
			}()
		}

	default:
		response["status"] = "error"
		response["error"] = "Unknown action"
	}

	encoder.Encode(response)
}

func UpdateStatus(status OrchestratorStatus) {
	statusMutex.Lock()
	currentStatus = status
	statusMutex.Unlock()
}
