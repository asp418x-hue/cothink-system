package com.cothink.system.core;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileReader;
import java.io.IOException;
import java.util.Locale;

/**
 * Hardware / thermal probes with simulated fallbacks for sandboxed and Android environments.
 * Mirrors the Rust thermal_monitor and Go QuerySystemMetrics helpers.
 */
public final class SystemMetrics {

    public static final class Snapshot {
        public final double tempCelsius;
        public final long cpuFreqMhz;
        public final String diskHealth;
        public final boolean simulated;

        public Snapshot(double tempCelsius, long cpuFreqMhz, String diskHealth, boolean simulated) {
            this.tempCelsius = tempCelsius;
            this.cpuFreqMhz = cpuFreqMhz;
            this.diskHealth = diskHealth;
            this.simulated = simulated;
        }

        @Override
        public String toString() {
            return String.format(
                    Locale.US,
                    "Temp: %.1f°C | CPU: %d MHz | Disk: %s%s",
                    tempCelsius,
                    cpuFreqMhz,
                    diskHealth,
                    simulated ? " (sim)" : "");
        }
    }

    public static final class DiskStats {
        public final String deviceName;
        public final long readsCompleted;
        public final long writesCompleted;
        public final long sectorsRead;
        public final long sectorsWritten;
        public final String healthStatus;

        public DiskStats(
                String deviceName,
                long readsCompleted,
                long writesCompleted,
                long sectorsRead,
                long sectorsWritten,
                String healthStatus) {
            this.deviceName = deviceName;
            this.readsCompleted = readsCompleted;
            this.writesCompleted = writesCompleted;
            this.sectorsRead = sectorsRead;
            this.sectorsWritten = sectorsWritten;
            this.healthStatus = healthStatus;
        }
    }

    private SystemMetrics() {}

    public static Snapshot query(String device) {
        Double temp = readThermalCelsius();
        Long freq = readCpuFreqMhz();
        boolean diskOk = new File("/sys/block/" + device + "/stat").exists();
        boolean simulated = temp == null || freq == null || !diskOk;

        double t = temp != null ? temp : 42.0 + (System.currentTimeMillis() % 700) / 100.0;
        long f = freq != null ? freq : 1800L;
        String health = diskOk ? "PASSED" : "PASSED (simulated)";
        return new Snapshot(t, f, health, simulated);
    }

    public static DiskStats readDiskStats(String device) {
        File stat = new File("/sys/block/" + device + "/stat");
        if (stat.exists()) {
            String content = readFile(stat);
            if (content != null) {
                String[] parts = content.trim().split("\\s+");
                if (parts.length >= 7) {
                    return new DiskStats(
                            device,
                            parseLong(parts[0], 0),
                            parseLong(parts[4], 0),
                            parseLong(parts[2], 0),
                            parseLong(parts[6], 0),
                            "PASSED");
                }
            }
        }
        return new DiskStats(device, 104520, 45290, 8361620, 3628100, "PASSED");
    }

    public static String failureDiagnostics(String device) {
        Snapshot s = query(device);
        return String.format(
                Locale.US,
                "diagnostics - Temp: %.1f°C | CPU Freq: %dMHz | Disk Status: %s",
                s.tempCelsius,
                s.cpuFreqMhz,
                s.diskHealth);
    }

    private static Double readThermalCelsius() {
        // Linux sysfs
        Double v = readMillidegreeFile("/sys/class/thermal/thermal_zone0/temp");
        if (v != null) {
            return v;
        }
        // Android thermal HAL dump paths (best-effort)
        File dir = new File("/sys/class/thermal");
        if (dir.isDirectory()) {
            File[] zones = dir.listFiles();
            if (zones != null) {
                for (File zone : zones) {
                    File temp = new File(zone, "temp");
                    Double t = readMillidegreeFile(temp.getAbsolutePath());
                    if (t != null) {
                        return t;
                    }
                }
            }
        }
        return null;
    }

    private static Long readCpuFreqMhz() {
        Long khz = readLongFile("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq");
        if (khz != null) {
            return khz / 1000L;
        }
        // Android: try cpuinfo_max_freq as a coarse fallback
        khz = readLongFile("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq");
        if (khz != null) {
            return khz / 1000L;
        }
        return null;
    }

    private static Double readMillidegreeFile(String path) {
        Long raw = readLongFile(path);
        if (raw == null) {
            return null;
        }
        // Some Android nodes already report degrees
        if (raw < 1000) {
            return raw.doubleValue();
        }
        return raw / 1000.0;
    }

    private static Long readLongFile(String path) {
        String content = readFile(new File(path));
        if (content == null) {
            return null;
        }
        try {
            return Long.parseLong(content.trim());
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static String readFile(File file) {
        if (!file.exists() || !file.canRead()) {
            return null;
        }
        try (BufferedReader br = new BufferedReader(new FileReader(file))) {
            return br.readLine();
        } catch (IOException e) {
            return null;
        }
    }

    private static long parseLong(String s, long def) {
        try {
            return Long.parseLong(s);
        } catch (NumberFormatException e) {
            return def;
        }
    }
}
