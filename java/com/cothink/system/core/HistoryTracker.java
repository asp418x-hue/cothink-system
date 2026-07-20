package com.cothink.system.core;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Lock-friendly ring-buffer history tracker ported from the Rust/Go cothink core.
 * Records orchestrator and subagent lifecycle events for FOV diagnostics.
 */
public final class HistoryTracker {

    public static final int HISTORY_CAPACITY = 4096;

    public static final class HistoryEntry {
        public final long timestampMs;
        public final long orchestratorId;
        public final int subagentId;
        public final String event;
        public final boolean success;
        public final String detail;

        public HistoryEntry(
                long timestampMs,
                long orchestratorId,
                int subagentId,
                String event,
                boolean success,
                String detail) {
            this.timestampMs = timestampMs;
            this.orchestratorId = orchestratorId;
            this.subagentId = subagentId;
            this.event = event;
            this.success = success;
            this.detail = detail == null ? "" : detail;
        }

        @Override
        public String toString() {
            String status = success ? "SUCCESS" : "FAIL";
            return String.format(
                    "[%d] Orch:%d Agent:%d Event:%s Status:%s Detail:%s",
                    timestampMs % 86_400_000L,
                    orchestratorId,
                    subagentId,
                    event,
                    status,
                    detail);
        }
    }

    public static final class FOV {
        public final int activeWorkers;
        public final double tempCelsius;
        public final long cpuFreqMhz;
        public final int recentSuccess;
        public final int recentFailure;

        public FOV(
                int activeWorkers,
                double tempCelsius,
                long cpuFreqMhz,
                int recentSuccess,
                int recentFailure) {
            this.activeWorkers = activeWorkers;
            this.tempCelsius = tempCelsius;
            this.cpuFreqMhz = cpuFreqMhz;
            this.recentSuccess = recentSuccess;
            this.recentFailure = recentFailure;
        }

        @Override
        public String toString() {
            return String.format(
                    "FOV{active=%d temp=%.1f°C freq=%dMHz ok=%d fail=%d}",
                    activeWorkers, tempCelsius, cpuFreqMhz, recentSuccess, recentFailure);
        }
    }

    private final Object[] locks = new Object[HISTORY_CAPACITY];
    private final HistoryEntry[] buffer = new HistoryEntry[HISTORY_CAPACITY];
    private final AtomicInteger cursor = new AtomicInteger(0);
    private final AtomicInteger activeWorkers = new AtomicInteger(0);
    private static final AtomicLong NEXT_ORCH_ID = new AtomicLong(1);
    private static final HistoryTracker INSTANCE = new HistoryTracker();

    private HistoryTracker() {
        for (int i = 0; i < HISTORY_CAPACITY; i++) {
            locks[i] = new Object();
        }
    }

    public static HistoryTracker getInstance() {
        return INSTANCE;
    }

    public static long nextOrchestratorId() {
        return NEXT_ORCH_ID.getAndIncrement();
    }

    public void record(long orchId, int subagentId, String event, boolean success, String detail) {
        if ("subagent_start".equals(event)) {
            activeWorkers.incrementAndGet();
        } else if ("subagent_success".equals(event)
                || "subagent_fail".equals(event)
                || "subagent_cancel".equals(event)) {
            activeWorkers.decrementAndGet();
        }

        int seq = cursor.getAndIncrement();
        int idx = Math.floorMod(seq, HISTORY_CAPACITY);
        HistoryEntry entry =
                new HistoryEntry(
                        System.currentTimeMillis(), orchId, subagentId, event, success, detail);
        synchronized (locks[idx]) {
            buffer[idx] = entry;
        }
    }

    public List<HistoryEntry> getHistory() {
        int cur = cursor.get();
        int start = cur > HISTORY_CAPACITY ? cur - HISTORY_CAPACITY : 0;
        List<HistoryEntry> results = new ArrayList<>();
        for (int i = start; i < cur; i++) {
            int idx = Math.floorMod(i, HISTORY_CAPACITY);
            synchronized (locks[idx]) {
                if (buffer[idx] != null) {
                    results.add(buffer[idx]);
                }
            }
        }
        return results;
    }

    public void clear() {
        cursor.set(0);
        activeWorkers.set(0);
        for (int i = 0; i < HISTORY_CAPACITY; i++) {
            synchronized (locks[i]) {
                buffer[i] = null;
            }
        }
    }

    public int getActiveWorkers() {
        return Math.max(0, activeWorkers.get());
    }

    public FOV getFov(long orchId, SystemMetrics.Snapshot metrics) {
        int successes = 0;
        int failures = 0;
        for (HistoryEntry entry : getHistory()) {
            if (entry.orchestratorId != orchId) {
                continue;
            }
            if ("subagent_success".equals(entry.event)) {
                successes++;
            } else if ("subagent_fail".equals(entry.event)
                    || "subagent_perl_fail".equals(entry.event)
                    || "subagent_lua_fail".equals(entry.event)) {
                failures++;
            }
        }
        return new FOV(
                getActiveWorkers(),
                metrics.tempCelsius,
                metrics.cpuFreqMhz,
                successes,
                failures);
    }

    public String formatHistory() {
        StringBuilder sb = new StringBuilder();
        sb.append("--- Execution History ---\n");
        for (HistoryEntry entry : getHistory()) {
            sb.append("  ").append(entry).append('\n');
        }
        return sb.toString();
    }
}
