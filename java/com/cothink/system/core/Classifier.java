package com.cothink.system.core;

import java.util.Arrays;
import java.util.Locale;

/**
 * Immutable sensor-frame classifier ported from Rust src/bin/classifier.rs.
 * Agents operate on read-only snapshots — no mutation, no decoherence.
 */
public final class Classifier {

    public static final class AnomalyScore {
        public final double score;
        public final boolean thresholdCrossed;

        public AnomalyScore(double score, boolean thresholdCrossed) {
            this.score = score;
            this.thresholdCrossed = thresholdCrossed;
        }

        @Override
        public String toString() {
            return String.format(
                    Locale.US, "score=%.2f crossed=%s", score, thresholdCrossed);
        }
    }

    public static final class SensorFrame {
        public final long timestampMs;
        public final long frameId;
        public final byte[] rawData;
        public final AnomalyScore anomalyScore;

        public SensorFrame(long timestampMs, long frameId, byte[] rawData, AnomalyScore anomalyScore) {
            this.timestampMs = timestampMs;
            this.frameId = frameId;
            this.rawData = rawData == null ? new byte[0] : Arrays.copyOf(rawData, rawData.length);
            this.anomalyScore = anomalyScore;
        }
    }

    public static final class ClassificationResult {
        public final long frameId;
        public final int payloadBytes;
        public final AnomalyScore score;
        public final String summary;

        public ClassificationResult(
                long frameId, int payloadBytes, AnomalyScore score, String summary) {
            this.frameId = frameId;
            this.payloadBytes = payloadBytes;
            this.score = score;
            this.summary = summary;
        }

        @Override
        public String toString() {
            return summary;
        }
    }

    private Classifier() {}

    /** Load simulated sensor data for a frame id. */
    public static SensorFrame loadSensorData(long id) {
        byte[] mock = new byte[] {(byte) 0x1A, (byte) 0x2B, (byte) 0x3C, (byte) 0x4D, (byte) 0x5E};
        double score = Math.min(1.0, id * 0.07);
        boolean crossed = score > 0.5;
        return new SensorFrame(
                System.currentTimeMillis(), id, mock, new AnomalyScore(score, crossed));
    }

    /** Classify a frame (read-only). Optionally sleeps to simulate work. */
    public static ClassificationResult classify(SensorFrame frame, boolean simulateWork) {
        if (simulateWork) {
            try {
                Thread.sleep(40);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        }
        String summary =
                String.format(
                        Locale.US,
                        "[Classifier] frame=%d bytes=%d anomaly=%.2f threshold=%s",
                        frame.frameId,
                        frame.rawData.length,
                        frame.anomalyScore.score,
                        frame.anomalyScore.thresholdCrossed);
        return new ClassificationResult(
                frame.frameId, frame.rawData.length, frame.anomalyScore, summary);
    }

    public static ClassificationResult classifyFrame(long frameId) {
        return classify(loadSensorData(frameId), true);
    }
}
