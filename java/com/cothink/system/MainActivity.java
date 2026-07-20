package com.cothink.system;

import android.app.Activity;
import android.graphics.Typeface;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.text.method.ScrollingMovementMethod;
import android.view.View;
import android.widget.Button;
import android.widget.ProgressBar;
import android.widget.SeekBar;
import android.widget.TextView;
import android.widget.Toast;

import com.cothink.system.core.Orchestrator;
import com.cothink.system.core.SystemMetrics;

import java.util.Locale;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Main Android entry point for the Cothink System orchestrator.
 * Runs spiral-allocated concurrent agents and streams FOV diagnostics to the UI.
 */
public class MainActivity extends Activity {

    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private final ExecutorService bg = Executors.newSingleThreadExecutor();
    private final AtomicReference<Orchestrator> running = new AtomicReference<>(null);

    private TextView logView;
    private TextView statusView;
    private TextView fovView;
    private TextView agentsLabel;
    private SeekBar agentsSeek;
    private ProgressBar progress;
    private Button runButton;
    private Button cancelButton;
    private Button metricsButton;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.main_activity);

        logView = findViewById(R.id.log_view);
        statusView = findViewById(R.id.status_view);
        fovView = findViewById(R.id.fov_view);
        agentsLabel = findViewById(R.id.agents_label);
        agentsSeek = findViewById(R.id.agents_seek);
        progress = findViewById(R.id.progress);
        runButton = findViewById(R.id.button_run);
        cancelButton = findViewById(R.id.button_cancel);
        metricsButton = findViewById(R.id.button_metrics);

        logView.setTypeface(Typeface.MONOSPACE);
        logView.setMovementMethod(new ScrollingMovementMethod());
        fovView.setTypeface(Typeface.MONOSPACE);

        agentsSeek.setMax(15); // 1..16
        agentsSeek.setProgress(7); // default 8
        updateAgentsLabel(8);

        agentsSeek.setOnSeekBarChangeListener(
                new SeekBar.OnSeekBarChangeListener() {
                    @Override
                    public void onProgressChanged(SeekBar seekBar, int progressValue, boolean fromUser) {
                        updateAgentsLabel(progressValue + 1);
                    }

                    @Override
                    public void onStartTrackingTouch(SeekBar seekBar) {}

                    @Override
                    public void onStopTrackingTouch(SeekBar seekBar) {}
                });

        runButton.setOnClickListener(v -> startRun());
        cancelButton.setOnClickListener(v -> cancelRun());
        metricsButton.setOnClickListener(v -> refreshMetrics());

        cancelButton.setEnabled(false);
        refreshMetrics();
        appendLog("Cothink System ready. Tap RUN to spawn spiral-ordered agents.\n");
    }

    private void updateAgentsLabel(int n) {
        agentsLabel.setText(getString(R.string.agents_count, n));
    }

    private int selectedAgents() {
        return agentsSeek.getProgress() + 1;
    }

    private void startRun() {
        if (running.get() != null) {
            Toast.makeText(this, R.string.already_running, Toast.LENGTH_SHORT).show();
            return;
        }

        final int agents = selectedAgents();
        final int sem = Math.max(2, Math.min(6, agents / 2));
        Orchestrator.Config cfg =
                new Orchestrator.Config(
                        /* maxChildren */ agents,
                        /* semaphoreLimit */ sem,
                        /* baseDelayMs */ 60L,
                        /* spiralTasks */ agents,
                        /* spiralDepth */ 3,
                        /* useSpiralOrder */ true);

        setRunningUi(true);
        logView.setText("");
        statusView.setText(getString(R.string.status_running, agents));
        appendLog(String.format(Locale.US, "Starting run: agents=%d semaphore=%d\n", agents, sem));

        bg.execute(
                () -> {
                    Orchestrator orch = new Orchestrator(cfg);
                    running.set(orch);
                    try {
                        Orchestrator.RunResult result =
                                orch.run(
                                        new Orchestrator.Listener() {
                                            @Override
                                            public void onLog(String line) {
                                                postLog(line);
                                            }

                                            @Override
                                            public void onAgentComplete(
                                                    int agentId, boolean success, String detail) {
                                                // progress is indeterminate; status updates via logs
                                            }

                                            @Override
                                            public void onFinished(Orchestrator.RunResult r) {
                                                // handled after run() returns
                                            }
                                        });

                        mainHandler.post(() -> showResult(result));
                    } catch (Exception e) {
                        mainHandler.post(
                                () -> {
                                    appendLog("ERROR: " + e.getMessage() + "\n");
                                    statusView.setText(R.string.status_error);
                                    setRunningUi(false);
                                });
                    } finally {
                        running.set(null);
                        mainHandler.post(() -> setRunningUi(false));
                    }
                });
    }

    private void cancelRun() {
        Orchestrator orch = running.get();
        if (orch != null) {
            orch.cancel();
            appendLog("Cancel requested…\n");
            statusView.setText(R.string.status_cancelling);
        }
    }

    private void refreshMetrics() {
        bg.execute(
                () -> {
                    SystemMetrics.Snapshot snap = SystemMetrics.query("sda");
                    mainHandler.post(
                            () ->
                                    fovView.setText(
                                            getString(
                                                    R.string.metrics_line,
                                                    snap.tempCelsius,
                                                    snap.cpuFreqMhz,
                                                    snap.diskHealth,
                                                    snap.simulated
                                                            ? getString(R.string.sim_tag)
                                                            : "")));
                });
    }

    private void showResult(Orchestrator.RunResult result) {
        statusView.setText(
                getString(
                        R.string.status_done,
                        result.completed.size(),
                        result.failed.size(),
                        result.durationMs));
        if (result.fov != null) {
            fovView.setText(
                    getString(
                            R.string.fov_line,
                            result.fov.activeWorkers,
                            result.fov.tempCelsius,
                            result.fov.cpuFreqMhz,
                            result.fov.recentSuccess,
                            result.fov.recentFailure));
        }
        Toast.makeText(
                        this,
                        getString(
                                R.string.toast_done,
                                result.completed.size(),
                                result.failed.size()),
                        Toast.LENGTH_SHORT)
                .show();
    }

    private void setRunningUi(boolean runningNow) {
        runButton.setEnabled(!runningNow);
        cancelButton.setEnabled(runningNow);
        agentsSeek.setEnabled(!runningNow);
        progress.setVisibility(runningNow ? View.VISIBLE : View.GONE);
        if (!runningNow && running.get() == null) {
            // keep last status
        }
    }

    private void postLog(String line) {
        mainHandler.post(() -> appendLog(line + "\n"));
    }

    private void appendLog(String text) {
        logView.append(text);
        // Auto-scroll
        final int scrollAmount =
                logView.getLayout() == null
                        ? 0
                        : logView.getLayout().getLineTop(logView.getLineCount())
                                - logView.getHeight();
        if (scrollAmount > 0) {
            logView.scrollTo(0, scrollAmount);
        }
    }

    @Override
    protected void onDestroy() {
        Orchestrator orch = running.getAndSet(null);
        if (orch != null) {
            orch.cancel();
        }
        bg.shutdownNow();
        super.onDestroy();
    }
}
