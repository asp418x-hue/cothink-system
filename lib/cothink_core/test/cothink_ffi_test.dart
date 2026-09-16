import 'package:flutter_test/flutter_test.dart';
import 'package:cothink_core/ffi/cothink_native.dart';

void main() {
  group('Cothink Native FFI Tests', () {
    final native = CothinkNative();

    test('Native library initializes or reports availability', () {
      print('Native FFI available: ${native.isAvailable}');
      if (!native.isAvailable) {
        print('Skipping live FFI assertions (native library not yet in search path).');
        return;
      }
      expect(native.isAvailable, isTrue);
    });

    test('FOV telemetry query', () {
      if (!native.isAvailable) return;

      final fov = native.getFov(orchId: 1);
      expect(fov, isNotNull);
      print('FOV Result: $fov');
      expect(fov!.activeWorkers, greaterThanOrEqualTo(0));
      expect(fov.tempCelsius, greaterThanOrEqualTo(0.0));
      expect(fov.cpuFreqMhz, greaterThanOrEqualTo(0));
    });

    test('Event recording into native history', () {
      if (!native.isAvailable) return;

      final ok = native.recordEvent(
        orchId: 1,
        subagentId: 42,
        event: 'test_ffi_event',
        success: true,
        detail: 'dart_ffi_verification',
      );
      expect(ok, isTrue);
    });

    test('Thermal zone sample query', () {
      if (!native.isAvailable) return;

      final sample = native.getThermalSample(
        zoneId: 0,
        simulatedTemp: 55.0,
        criticalTemp: 75.0,
      );
      expect(sample, isNotNull);
      print('Thermal Sample: $sample');
      expect(sample!.tempCelsius, greaterThan(0.0));
      expect(sample.cpuFreqMhz, greaterThan(0));
    });

    test('Frame classification', () {
      if (!native.isAvailable) return;

      final res1 = native.classify(1);
      expect(res1, isNotNull);
      expect(res1!.thresholdCrossed, isFalse);

      final res10 = native.classify(10);
      expect(res10, isNotNull);
      expect(res10!.thresholdCrossed, isTrue);
      print('Classify Result (frame 10): $res10');
    });

    test('Payload executor', () {
      if (!native.isAvailable) return;

      final res = native.executePayload('{"swarm_id": 99}');
      expect(res, isNotNull);
      print('Payload Execution Result: $res');
      expect(res!['status'], equals('success'));
      expect(res.containsKey('score'), isTrue);
      expect(res.containsKey('load_1m'), isTrue);
    });
  });
}
