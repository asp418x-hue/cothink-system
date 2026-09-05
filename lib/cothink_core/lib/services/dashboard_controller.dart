import 'package:flutter/material.dart';
import 'dart:async';
import 'api_client.dart';
import 'runtime_manager.dart';

class DashboardController extends ChangeNotifier {
  Timer? _statusTimer;
  List<dynamic> agents = [];
  int activeWorkers = 0;
  bool showEditor = false;

  DashboardController({bool autoStart = true}) {
    if (autoStart) {
      start();
    }
  }

  void start() {
    RuntimeManager().startRuntimes();
    _statusTimer ??= Timer.periodic(const Duration(milliseconds: 500), (_) => fetchStatus());
  }

  @override
  void dispose() {
    _statusTimer?.cancel();
    super.dispose();
  }

  Future<void> fetchStatus() async {
    try {
      final status = await ApiClient.getStatus();
      if (status['status'] != 'error') {
        agents = status['agents'] ?? [];
        activeWorkers = status['active_workers'] ?? 0;
        notifyListeners();
      }
    } catch (e) {
      // Ignore polling errors
    }
  }

  void toggleEditor() {
    showEditor = !showEditor;
    notifyListeners();
  }
}
