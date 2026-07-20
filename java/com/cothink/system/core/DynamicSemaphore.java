package com.cothink.system.core;

/**
 * Thread-safe, resizable concurrency choke-point ported from Go cothink.DynamicSemaphore.
 */
public final class DynamicSemaphore {

    private final Object lock = new Object();
    private int limit;
    private int active;

    public DynamicSemaphore(int initialLimit) {
        if (initialLimit < 1) {
            throw new IllegalArgumentException("initialLimit must be >= 1");
        }
        this.limit = initialLimit;
    }

    /** Blocks until a concurrency slot becomes available. */
    public void acquire() throws InterruptedException {
        synchronized (lock) {
            while (active >= limit) {
                lock.wait();
            }
            active++;
        }
    }

    /** Frees a concurrency slot and wakes waiters. */
    public void release() {
        synchronized (lock) {
            active = Math.max(0, active - 1);
            lock.notifyAll();
        }
    }

    /** Dynamically resizes the maximum concurrency ceiling. */
    public void setLimit(int newLimit) {
        if (newLimit < 1) {
            throw new IllegalArgumentException("newLimit must be >= 1");
        }
        synchronized (lock) {
            this.limit = newLimit;
            lock.notifyAll();
        }
    }

    public int getActive() {
        synchronized (lock) {
            return active;
        }
    }

    public int getLimit() {
        synchronized (lock) {
            return limit;
        }
    }
}
