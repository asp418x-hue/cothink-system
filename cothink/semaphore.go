package cothink

import (
	"sync"
)

// DynamicSemaphore implements a thread-safe, resizable concurrency choke-point
type DynamicSemaphore struct {
	mu     sync.Mutex
	limit  int
	active int
	cond   *sync.Cond
}

// NewDynamicSemaphore initializes a semaphore with a specific limit
func NewDynamicSemaphore(initialLimit int) *DynamicSemaphore {
	ds := &DynamicSemaphore{limit: initialLimit}
	ds.cond = sync.NewCond(&ds.mu)
	return ds
}

// Acquire blocks until a concurrency slot becomes available
func (ds *DynamicSemaphore) Acquire() {
	ds.mu.Lock()
	defer ds.mu.Unlock()
	for ds.active >= ds.limit {
		ds.cond.Wait()
	}
	ds.active++
}

// Release frees a concurrency slot and notifies waiting acquire calls
func (ds *DynamicSemaphore) Release() {
	ds.mu.Lock()
	defer ds.mu.Unlock()
	ds.active--
	ds.cond.Broadcast()
}

// SetLimit dynamically resizes the maximum concurrency ceiling
func (ds *DynamicSemaphore) SetLimit(newLimit int) {
	ds.mu.Lock()
	defer ds.mu.Unlock()
	ds.limit = newLimit
	// Broadcast in case the limit has increased, letting waiting workers proceed
	ds.cond.Broadcast()
}

// GetActive returns the current number of active workers running in the choke-point
func (ds *DynamicSemaphore) GetActive() int {
	ds.mu.Lock()
	defer ds.mu.Unlock()
	return ds.active
}
