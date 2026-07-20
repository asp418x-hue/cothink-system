package com.cothink.system.core;

import java.util.ArrayList;
import java.util.List;

/**
 * Logarithmic φ-spiral task allocator ported from Rust src/main.rs.
 * Produces a deterministic spawn order over a square grid of task slots.
 */
public final class SpiralAllocator {

    private static final double PHI = 1.618033988749895;

    private SpiralAllocator() {}

    /** Radial layer derived from log_φ(index). */
    public static int radialLayer(int index) {
        if (index == 0) {
            return 0;
        }
        int layer = (int) Math.floor(Math.log(index) / Math.log(PHI));
        return Math.max(1, layer);
    }

    /**
     * Returns a spiral-ordered list of task indices covering up to {@code taskCount}
     * slots, truncated to {@code truncationDepth} spiral layers.
     */
    public static List<Integer> spiralOrder(int taskCount, int truncationDepth) {
        List<Integer> order = new ArrayList<>();
        if (taskCount <= 0 || truncationDepth <= 0) {
            return order;
        }

        int size = (int) Math.ceil(Math.sqrt(taskCount));
        int layers = Math.min(truncationDepth, (size + 1) / 2);

        for (int layer = 0; layer < layers; layer++) {
            int top = layer;
            int bottom = Math.max(0, size - 1 - layer);
            int left = layer;
            int right = Math.max(0, size - 1 - layer);

            if (top > bottom || left > right) {
                break;
            }

            int stride = Math.max(1, radialLayer(layer + 1));

            for (int col = left; col <= right; col++) {
                if (order.size() == taskCount) {
                    return order;
                }
                if ((col + top) % stride == 0 || layer == 0) {
                    int index = top * size + col;
                    if (index < taskCount) {
                        order.add(index);
                    }
                }
            }

            for (int row = top + 1; row <= bottom; row++) {
                if (order.size() == taskCount) {
                    return order;
                }
                if ((row + right) % stride == 0 || layer == 0) {
                    int index = row * size + right;
                    if (index < taskCount) {
                        order.add(index);
                    }
                }
            }

            if (top < bottom && left < right) {
                for (int col = right - 1; col >= left; col--) {
                    if (order.size() == taskCount) {
                        return order;
                    }
                    if ((col + bottom) % stride == 0 || layer == 0) {
                        int index = bottom * size + col;
                        if (index < taskCount) {
                            order.add(index);
                        }
                    }
                }

                for (int row = bottom - 1; row > top; row--) {
                    if (order.size() == taskCount) {
                        return order;
                    }
                    if ((row + left) % stride == 0 || layer == 0) {
                        int index = row * size + left;
                        if (index < taskCount) {
                            order.add(index);
                        }
                    }
                }
            }
        }
        return order;
    }
}
