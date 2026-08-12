package main

import (
	"encoding/json"
	"fmt"
	"net"
	"os"
)

func RunClient(port int) {
	conn, err := net.Dial("tcp", fmt.Sprintf("localhost:%d", port))
	if err != nil {
		fmt.Printf("Error connecting to server: %v\n", err)
		os.Exit(1)
	}
	defer conn.Close()

	request := map[string]string{"action": "get_status"}
	json.NewEncoder(conn).Encode(request)

	var status OrchestratorStatus
	if err := json.NewDecoder(conn).Decode(&status); err != nil {
		fmt.Printf("Error decoding status: %v\n", err)
		return
	}

	fmt.Printf("Orchestrator Status:\n")
	fmt.Printf("  Active Workers: %d\n", status.ActiveWorkers)
	fmt.Printf("  Max Children:   %d\n", status.MaxChildren)
	fmt.Printf("  Base Delay:     %d ms\n", status.BaseDelayMs)
	fmt.Printf("  Total Agents:   %d\n", len(status.Agents))
	for _, agent := range status.Agents {
		fmt.Printf("    - Agent %d (Depth %d)\n", agent.ID, agent.Depth)
	}
	if len(status.History) > 0 {
		fmt.Printf("  Execution History Trace:\n")
		for _, entry := range status.History {
			successStr := "SUCCESS"
			if !entry.Success {
				successStr = "FAIL"
			}
			fmt.Printf("    [%s] Orch ID: %d | Agent ID: %d | Event: %-22s | Result: %-7s | Detail: %s\n",
				entry.Timestamp.Format("15:04:05.000"),
				entry.OrchestratorID,
				entry.SubagentID,
				entry.Event,
				successStr,
				entry.Detail,
			)
		}
	}
}
