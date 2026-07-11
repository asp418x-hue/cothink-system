package main

import (
	"fmt"
	"time"
	"cothink-system/thermalopt"
)

func main() {
	fmt.Println("Initializing Go Thermal & Clock Speed Optimizer...")

	zones := []thermalopt.ThermalZone{
		{ZoneID: 0, Path: "/sys/class/thermal/thermal_zone0/temp"},
		{ZoneID: 1, Path: "/sys/class/thermal/thermal_zone1/temp"},
	}

	governor := thermalopt.CpuGovernor{
		Path: "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
	}

	manager := &thermalopt.ThermalManager{
		Zones:               zones,
		Governor:            governor,
		TargetDisk:          "sda",
		CriticalTempCelsius: 75.0,
	}

	// Run simulated temperature sweeps
	temperatures := []float64{48.2, 62.5, 80.1, 74.3, 55.0}

	for i, temp := range temperatures {
		manager.Optimize(i+1, temp)
		time.Sleep(500 * time.Millisecond)
	}

	fmt.Println("\nSimulation sweeps finished.")
}
