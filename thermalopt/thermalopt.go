package thermalopt

import (
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// DiskStats represents S.M.A.R.T.-like block device stats
type DiskStats struct {
	DeviceName       string
	ReadsCompleted   uint64
	WritesCompleted  uint64
	SectorsRead      uint64
	SectorsWritten   uint64
	SMARTHealthStatus string
}

// ReadDiskStats queries system stats or falls back to mock stats if unavailable
func ReadDiskStats(device string) DiskStats {
	statPath := fmt.Sprintf("/sys/block/%s/stat", device)
	if data, err := ioutil.ReadFile(statPath); err == nil {
		fields := strings.Fields(string(data))
		if len(fields) >= 7 {
			rComp, _ := strconv.ParseUint(fields[0], 10, 64)
			wComp, _ := strconv.ParseUint(fields[4], 10, 64)
			sRead, _ := strconv.ParseUint(fields[2], 10, 64)
			sWrite, _ := strconv.ParseUint(fields[6], 10, 64)
			return DiskStats{
				DeviceName:        device,
				ReadsCompleted:    rComp,
				WritesCompleted:   wComp,
				SectorsRead:       sRead,
				SectorsWritten:    sWrite,
				SMARTHealthStatus: "PASSED",
			}
		}
	}
	// Simulated fallback stats
	return DiskStats{
		DeviceName:        device,
		ReadsCompleted:    98124,
		WritesCompleted:   34182,
		SectorsRead:       7289100,
		SectorsWritten:    2918230,
		SMARTHealthStatus: "PASSED",
	}
}

// ThermalZone monitors device temperatures
type ThermalZone struct {
	ZoneID int
	Path   string
}

// GetTemperature fetches the temperature in Celsius or uses the simulation value
func (tz *ThermalZone) GetTemperature(simulatedTemp float64) float64 {
	if data, err := ioutil.ReadFile(tz.Path); err == nil {
		if tempVal, err := strconv.ParseFloat(strings.TrimSpace(string(data)), 64); err == nil {
			return tempVal / 1000.0 // sysfs uses millidegrees Celsius
		}
	}
	return simulatedTemp
}

// CpuGovernor manages CPU clock frequency limits
type CpuGovernor struct {
	Path string
}

// GetCurrentFrequency returns the current scaling frequency in MHz
func (cg *CpuGovernor) GetCurrentFrequency(simulatedFreq uint64) uint64 {
	if data, err := ioutil.ReadFile(cg.Path); err == nil {
		if freqVal, err := strconv.ParseUint(strings.TrimSpace(string(data)), 10, 64); err == nil {
			return freqVal / 1000
		}
	}
	return simulatedFreq
}

// SetFrequencyLimit writes the new maximum scaling frequency or simulates it
func (cg *CpuGovernor) SetFrequencyLimit(maxFreqMHz uint64) error {
	fmt.Printf("[Go Governor] Adjusting CPU clock limit to %d MHz...\n", maxFreqMHz)
	maxPath := strings.Replace(cg.Path, "scaling_cur_freq", "scaling_max_freq", 1)
	
	// Try writing if file exists
	if _, err := os.Stat(maxPath); err == nil {
		freqStr := fmt.Sprintf("%d", maxFreqMHz*1000)
		return ioutil.WriteFile(maxPath, []byte(freqStr), 0644)
	}
	
	// Fallback simulated success
	fmt.Printf("[Go Governor Simulation] Simulated write to %s: %d MHz\n", filepath.Base(maxPath), maxFreqMHz)
	return nil
}

// ThermalManager orchestrates the thermal optimization loop
type ThermalManager struct {
	Zones               []ThermalZone
	Governor            CpuGovernor
	TargetDisk          string
	CriticalTempCelsius float64
}

// Optimize monitors inputs and decides optimization strategy
func (tm *ThermalManager) Optimize(cycleStep int, simulatedTemp float64) {
	fmt.Printf("\n--- [Go Opt Cycle #%d] ---\n", cycleStep)

	// 1. Read Disk SMART Stats
	disk := ReadDiskStats(tm.TargetDisk)
	fmt.Printf("[Disk Manager] Disk: %s | SMART Status: %s | Total Sectors Read: %d\n",
		disk.DeviceName, disk.SMARTHealthStatus, disk.SectorsRead)

	// 2. Read temperature zones
	for _, zone := range tm.Zones {
		temp := zone.GetTemperature(simulatedTemp)
		fmt.Printf("[Thermal Zone %d] Temperature: %.1f°C\n", zone.ZoneID, temp)

		// 3. Optimize Clocks & Power based on thresholds
		if temp >= tm.CriticalTempCelsius {
			fmt.Printf("[Throttling Trigger] Critical Limit %.1f°C exceeded!\n", tm.CriticalTempCelsius)
			_ = tm.Governor.SetFrequencyLimit(1200) // Lower CPU power limit
		} else {
			fmt.Printf("[Cool Zone] Optimizing for peak clock frequency.\n")
			_ = tm.Governor.SetFrequencyLimit(2400) // Boost CPU performance limit
		}
	}

	simulatedClock := uint64(2400)
	if simulatedTemp >= tm.CriticalTempCelsius {
		simulatedClock = 1200
	}
	currentClock := tm.Governor.GetCurrentFrequency(simulatedClock)
	fmt.Printf("[Clock Status] CPU Clock Speed: %d MHz\n", currentClock)
}
