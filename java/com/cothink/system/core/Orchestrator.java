package com.cothink.system.core;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

/**
 * Thinkspace orchestrator — ports Rust concurrent subagent dispatch and Go ScalarSpawn
 * into a pure-Java runtime suitable for Android (no external perl/lua/python binaries).
 */
public final class Orchestrator {

    public interface Listener {
        void onLog(String line);

        void onAgentComplete(int agentId, boolean success, String detail);

        void onFinished(RunResult result);
    }

    public static final class Config {
        public final int maxChildren;
        public final int semaphoreLimit;
        public final long baseDelayMs;
        public final int spiralTasks;
        public final int spiralDepth;
        public final boolean useSpiralOrder;

        public Config(
                int maxChildren,
                int semaphoreLimit,
                long baseDelayMs,
                int spiralTasks,
                int spiralDepth,
                boolean useSpiralOrder) {
            this.maxChildren = maxChildren;
            this.semaphoreLimit = semaphoreLimit;
            this.baseDelayMs = baseDelayMs;
            this.spiralTasks = spiralTasks;
            this.spiralDepth = spiralDepth;
            this.useSpiralOrder = useSpiralOrder;
        }

        public static Config defaults() {
            return new Config(8, 4, 80L, 8, 2, true);
        }
    }

    public static final class RunResult {
        public final long orchestratorId;
        public final List<Integer> completed;
        public final List<Integer> failed;
        public final HistoryTracker.FOV fov;
        public final String historyText;
        public final long durationMs;

        public RunResult(
                long orchestratorId,
                List<Integer> completed,
                List<Integer> failed,
                HistoryTracker.FOV fov,
                String historyText,
                long durationMs) {
            this.orchestratorId = orchestratorId;
            this.completed = completed;
            this.failed = failed;
            this.fov = fov;
            this.historyText = historyText;
            this.durationMs = durationMs;
        }
    }

    private final Config config;
    private final HistoryTracker history;
    private final DynamicSemaphore semaphore;
    private final AtomicBoolean cancelled = new AtomicBoolean(false);
    private ExecutorService pool;

    public Orchestrator(Config config) {
        this.config = config == null ? Config.defaults() : config;
        this.history = HistoryTracker.getInstance();
        this.semaphore = new DynamicSemaphore(this.config.semaphoreLimit);
    }

    public void cancel() {
        cancelled.set(true);
        if (pool != null) {
            pool.shutdownNow();
        }
    }

    public RunResult run(Listener listener) {
        cancelled.set(false);
        history.clear();
        long orchId = HistoryTracker.nextOrchestratorId();
        long started = System.currentTimeMillis();
        AgentNode root = new AgentNode(0, 0);

        log(listener, "cothink-system is booting...");
        log(listener, "Thinkspace Orchestrator #" + orchId + " on Android runtime");

        history.record(
                orchId,
                0,
                "orch_start",
                true,
                "allocation=" + config.maxChildren + " sem=" + config.semaphoreLimit);

        List<Integer> taskIds;
        if (config.useSpiralOrder) {
            taskIds = SpiralAllocator.spiralOrder(config.spiralTasks, config.spiralDepth);
            log(listener, "spiral allocation: " + taskIds);
        } else {
            taskIds = new ArrayList<>();
            for (int i = 1; i <= config.maxChildren; i++) {
                taskIds.add(i);
            }
            log(listener, "linear allocation: " + taskIds);
        }

        int workers = Math.max(1, Math.min(config.semaphoreLimit, taskIds.size()));
        pool = Executors.newFixedThreadPool(workers);

        List<Future<Integer>> futures = new ArrayList<>();
        for (int i = 0; i < taskIds.size(); i++) {
            final int taskId = taskIds.get(i);
            final int spawnIndex = i + 1;
            futures.add(pool.submit(() -> runSubagent(orchId, taskId, spawnIndex, root, listener)));
        }

        List<Integer> completed = new ArrayList<>();
        List<Integer> failed = new ArrayList<>();
        for (int i = 0; i < futures.size(); i++) {
            int taskId = taskIds.get(i);
            try {
                Integer ok = futures.get(i).get();
                if (ok != null) {
                    completed.add(ok);
                } else {
                    failed.add(taskId);
                }
            } catch (Exception e) {
                failed.add(taskId);
                String msg = e.getMessage() == null ? e.toString() : e.getMessage();
                history.record(orchId, taskId, "subagent_fail", false, msg);
                log(listener, "Subagent task failed: " + msg);
                if (listener != null) {
                    listener.onAgentComplete(taskId, false, msg);
                }
            }
        }

        pool.shutdown();
        try {
            pool.awaitTermination(5, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }

        history.record(
                orchId,
                0,
                "orch_finish",
                true,
                "completed=" + completed.size() + " failed=" + failed.size());

        SystemMetrics.Snapshot metrics = SystemMetrics.query("sda");
        HistoryTracker.FOV fov = history.getFov(orchId, metrics);
        String historyText = history.formatHistory();
        long duration = System.currentTimeMillis() - started;

        log(listener, "completed subagents: " + completed);
        log(listener, "FOV: " + fov);
        log(listener, String.format(Locale.US, "run finished in %d ms", duration));
        log(listener, historyText);

        RunResult result = new RunResult(orchId, completed, failed, fov, historyText, duration);
        if (listener != null) {
            listener.onFinished(result);
        }
        return result;
    }

    private Integer runSubagent(
            long orchId, int taskId, int spawnIndex, AgentNode root, Listener listener)
            throws Exception {
        if (cancelled.get() || Thread.currentThread().isInterrupted()) {
            history.record(orchId, taskId, "subagent_cancel", false, "cancelled");
            return null;
        }

        // Stagger spawn like Go ScalarSpawn (id * BaseDelay)
        long delay = Math.max(0, spawnIndex) * config.baseDelayMs;
        if (delay > 0) {
            Thread.sleep(delay);
        }

        semaphore.acquire();
        try {
            if (cancelled.get() || Thread.currentThread().isInterrupted()) {
                history.record(orchId, taskId, "subagent_cancel", false, "cancelled after acquire");
                return null;
            }

            history.record(orchId, taskId, "subagent_start", true, "");
            log(listener, "[Agent " + taskId + "] start");

            // 1) Mock "preprocessor" stage (replaces perl agent.pl on device)
            history.record(orchId, taskId, "subagent_perl_start", true, "");
            String pre = preprocess(taskId);
            history.record(orchId, taskId, "subagent_perl_success", true, "bytes=" + pre.length());

            // 2) Classifier stage (replaces lua classifier / Rust classifier binary)
            history.record(orchId, taskId, "subagent_lua_start", true, "");
            Classifier.ClassificationResult cls = Classifier.classifyFrame(taskId);
            history.record(
                    orchId,
                    taskId,
                    "subagent_lua_success",
                    true,
                    "output len: " + cls.summary.length());

            String output = pre + " | " + cls.summary;
            history.record(orchId, taskId, "subagent_success", true, output);

            AgentNode child = new AgentNode(taskId, root.depth + 1);
            child.metadata.put("output", output);
            child.metadata.put("anomaly", String.valueOf(cls.score.score));
            root.addChild(child);

            log(listener, "[Agent " + taskId + "] " + output);
            if (listener != null) {
                listener.onAgentComplete(taskId, true, output);
            }
            return taskId;
        } catch (Exception e) {
            String err = e.getMessage() == null ? e.toString() : e.getMessage();
            history.record(orchId, taskId, "subagent_fail", false, err);
            String diag = SystemMetrics.failureDiagnostics("sda");
            history.record(orchId, taskId, "subagent_fail_query", false, diag);
            log(listener, "[Agent " + taskId + "] FAIL: " + err + " | " + diag);
            if (listener != null) {
                listener.onAgentComplete(taskId, false, err);
            }
            return null;
        } finally {
            semaphore.release();
        }
    }

    private static String preprocess(int taskId) {
        // Lightweight deterministic stand-in for agent.pl
        int hash = Integer.hashCode(taskId * 31 + 7);
        return String.format(Locale.US, "pre[task=%d hash=%08x]", taskId, hash);
    }

    private static void log(Listener listener, String line) {
        if (listener != null) {
            listener.onLog(line);
        }
    }

    /** Convenience runner used by unit tests / CLI-style invocation. */
    public static RunResult quickRun(Consumer<String> logSink) {
        Orchestrator orch = new Orchestrator(Config.defaults());
        return orch.run(
                new Listener() {
                    @Override
                    public void onLog(String line) {
                        if (logSink != null) {
                            logSink.accept(line);
                        }
                    }

                    @Override
                    public void onAgentComplete(int agentId, boolean success, String detail) {}

                    @Override
                    public void onFinished(RunResult result) {}
                });
    }
}
