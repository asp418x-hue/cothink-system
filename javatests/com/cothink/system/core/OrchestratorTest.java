package com.cothink.system.core;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import java.util.HashSet;
import java.util.Set;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

@RunWith(JUnit4.class)
public class OrchestratorTest {

    @Test
    public void concurrentDispatchCompletesAgents() {
        Orchestrator.Config cfg =
                new Orchestrator.Config(
                        /* maxChildren */ 6,
                        /* semaphoreLimit */ 3,
                        /* baseDelayMs */ 5L,
                        /* spiralTasks */ 6,
                        /* spiralDepth */ 3,
                        /* useSpiralOrder */ true);
        Orchestrator orch = new Orchestrator(cfg);
        Orchestrator.RunResult result = orch.run(null);

        assertEquals(6, result.completed.size());
        assertTrue(result.failed.isEmpty());
        Set<Integer> seen = new HashSet<>(result.completed);
        assertEquals(6, seen.size());
        assertTrue(result.durationMs >= 0);
        assertTrue(result.historyText.contains("Execution History"));
    }

    @Test
    public void historyRecordsOrchestratorLifecycle() {
        Orchestrator.Config cfg =
                new Orchestrator.Config(4, 2, 1L, 4, 2, true);
        Orchestrator orch = new Orchestrator(cfg);
        Orchestrator.RunResult result = orch.run(null);
        assertTrue(result.historyText.contains("orch_start"));
        assertTrue(result.historyText.contains("orch_finish"));
        assertEquals(4, result.fov.recentSuccess);
    }
}
