package com.cothink.system.core;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

@RunWith(JUnit4.class)
public class SpiralAllocatorTest {

    @Test
    public void spiralOrderCoversAllTasksOnce() {
        List<Integer> order = SpiralAllocator.spiralOrder(8, 3);
        assertEquals(8, order.size());
        Set<Integer> seen = new HashSet<>();
        for (int idx : order) {
            assertTrue("duplicate index " + idx, seen.add(idx));
            assertTrue(idx >= 0 && idx < 8);
        }
    }

    @Test
    public void radialLayerUsesLogPhiSpacing() {
        assertEquals(0, SpiralAllocator.radialLayer(0));
        assertEquals(1, SpiralAllocator.radialLayer(1));
        assertEquals(1, SpiralAllocator.radialLayer(2));
        assertEquals(2, SpiralAllocator.radialLayer(3));
        assertEquals(2, SpiralAllocator.radialLayer(4));
        assertEquals(3, SpiralAllocator.radialLayer(5));
    }

    @Test
    public void emptyInputsReturnEmpty() {
        assertTrue(SpiralAllocator.spiralOrder(0, 3).isEmpty());
        assertTrue(SpiralAllocator.spiralOrder(8, 0).isEmpty());
    }
}
