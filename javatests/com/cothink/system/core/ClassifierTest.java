package com.cothink.system.core;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

@RunWith(JUnit4.class)
public class ClassifierTest {

    @Test
    public void anomalyScoreMath() {
        Classifier.SensorFrame frame = Classifier.loadSensorData(1);
        assertEquals(1, frame.frameId);
        assertEquals(0.07, frame.anomalyScore.score, 1e-6);
        assertFalse(frame.anomalyScore.thresholdCrossed);

        Classifier.SensorFrame high = Classifier.loadSensorData(8);
        assertTrue(high.anomalyScore.thresholdCrossed);
    }

    @Test
    public void sharedRawDataLength() {
        Classifier.SensorFrame frame = Classifier.loadSensorData(1);
        assertEquals(5, frame.rawData.length);
        assertEquals((byte) 0x1A, frame.rawData[0]);
    }

    @Test
    public void classifyWithoutSleep() {
        Classifier.SensorFrame frame = Classifier.loadSensorData(3);
        Classifier.ClassificationResult r = Classifier.classify(frame, false);
        assertEquals(3, r.frameId);
        assertTrue(r.summary.contains("frame=3"));
    }
}
