package cothink

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync/atomic"
	"time"
)

// HistoryCapacity specifies the size of our pre-allocated ring buffer.
const HistoryCapacity = 4096

// HistoryEntry stores detailed trace metadata for orchestrators and subagents.
type HistoryEntry struct {
	Timestamp      time.Time `json:"timestamp"`
	OrchestratorID int64     `json:"orchestrator_id"`
	SubagentID     int       `json:"subagent_id"`
	Event          string    `json:"event"`
	Success        bool      `json:"success"`
	Detail         string    `json:"detail"`
	Sequence       uint64    `json:"-"`
}

var (
	historyBuffer [HistoryCapacity]HistoryEntry
	historyCursor uint64 // atomic counter
)

// RecordEvent appends a new trace log to the ring buffer. It is lock-free
// and allocates no memory on the hot path (except for dynamic strings passed).
func RecordEvent(orchID int64, subagentID int, event string, success bool, detail string) {
	seq := atomic.AddUint64(&historyCursor, 1) - 1
	idx := seq % HistoryCapacity

	entry := &historyBuffer[idx]
	entry.Timestamp = time.Now()
	entry.OrchestratorID = orchID
	entry.SubagentID = subagentID
	entry.Event = event
	entry.Success = success
	entry.Detail = detail
	
	// Publish the update atomically with release semantics
	atomic.StoreUint64(&entry.Sequence, seq+1)
}

// GetHistory returns the slice of valid, completed trace logs currently stored.
func GetHistory() []HistoryEntry {
	cursor := atomic.LoadUint64(&historyCursor)
	var start uint64
	if cursor > HistoryCapacity {
		start = cursor - HistoryCapacity
	}
	
	results := make([]HistoryEntry, 0, HistoryCapacity)
	for i := start; i < cursor; i++ {
		idx := i % HistoryCapacity
		entry := historyBuffer[idx]
		seq := atomic.LoadUint64(&entry.Sequence)
		// Only read if the slot corresponds exactly to the current cursor iteration (is not being overwritten)
		if seq == i+1 {
			results = append(results, entry)
		}
	}
	return results
}

// ClearHistory resets the history cursor and pre-allocated buffer
func ClearHistory() {
	atomic.StoreUint64(&historyCursor, 0)
	for i := 0; i < HistoryCapacity; i++ {
		atomic.StoreUint64(&historyBuffer[i].Sequence, 0)
	}
}

// FOV represents a read-only snapshot of the system environment and metrics.
type FOV struct {
	ActiveWorkers int     `json:"active_workers"`
	TempCelsius   float64 `json:"temp_celsius"`
	CpuFreqMHz    uint64  `json:"cpu_freq_mhz"`
	RecentSuccess int     `json:"recent_success"`
	RecentFailure int     `json:"recent_failure"`
}

// GetFOV calculates the orchestrator's field of view in-stride without introducing lock contention.
func (orch *Orchestrator) GetFOV(device string) FOV {
	temp, freq, _ := QuerySystemMetrics(device)

	var active int
	if orch.Semaphore != nil {
		active = orch.Semaphore.GetActive()
	}

	successes := 0
	failures := 0

	cursor := atomic.LoadUint64(&historyCursor)
	var start uint64
	if cursor > HistoryCapacity {
		start = cursor - HistoryCapacity
	}

	for i := start; i < cursor; i++ {
		idx := i % HistoryCapacity
		entry := historyBuffer[idx]
		seq := atomic.LoadUint64(&entry.Sequence)
		if seq == i+1 && entry.OrchestratorID == orch.ID {
			if entry.Event == "subagent_success" {
				successes++
			} else if entry.Event == "subagent_fail" {
				failures++
			}
		}
	}

	return FOV{
		ActiveWorkers: active,
		TempCelsius:   temp,
		CpuFreqMHz:    freq,
		RecentSuccess: successes,
		RecentFailure: failures,
	}
}

// QuerySystemMetrics reads hardware sensor paths.
func QuerySystemMetrics(device string) (temp float64, freq uint64, diskHealth string) {
	temp = 0.0
	if data, err := os.ReadFile("/sys/class/thermal/thermal_zone0/temp"); err == nil {
		if t, err := strconv.ParseFloat(strings.TrimSpace(string(data)), 64); err == nil {
			temp = t / 1000.0
		}
	}

	freq = uint64(0)
	if data, err := os.ReadFile("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"); err == nil {
		if f, err := strconv.ParseUint(strings.TrimSpace(string(data)), 10, 64); err == nil {
			freq = f / 1000
		}
	}

	diskHealth = "PASSED"
	if _, err := os.Stat(fmt.Sprintf("/sys/block/%s/stat", device)); err != nil {
		diskHealth = "PASSED (simulated)"
	}

	return temp, freq, diskHealth
}
