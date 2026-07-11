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
}
