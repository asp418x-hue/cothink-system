import 'package:flutter/material.dart';
import '../services/runtime_manager.dart';
import '../services/api_client.dart';
import 'dart:async';

class TaskManagerScreen extends StatefulWidget {
  const TaskManagerScreen({super.key});

  @override
  State<TaskManagerScreen> createState() => _TaskManagerScreenState();
}

class _TaskManagerScreenState extends State<TaskManagerScreen> {
  final List<String> _logs = [];
  final ScrollController _scrollController = ScrollController();

  Timer? _pollingTimer;
  final Set<String> _seenOutputs = {};

  @override
  void initState() {
    super.initState();
    _startPolling();
  }

  void _startPolling() {
    _pollingTimer = Timer.periodic(const Duration(milliseconds: 500), (_) async {
      try {
        final status = await ApiClient.getStatus();
        if (!mounted || status['status'] == 'error') return;
        
        final agents = status['agents'] ?? [];
        bool addedNew = false;
        
        setState(() {
          for (var agent in agents) {
            final metadata = agent['metadata'] as Map<String, dynamic>? ?? {};
            final String output = metadata['output'] ?? '';
            
            if (output.trim().isNotEmpty) {
              // Create a unique key for this agent's output based on its ID and output
              final int id = agent['id'];
              final String uniqueKey = '${id}_$output';
              
              if (!_seenOutputs.contains(uniqueKey)) {
                _seenOutputs.add(uniqueKey);
                _logs.addAll(output.split('\n').where((s) => s.trim().isNotEmpty));
                addedNew = true;
              }
            }
          }
          if (_logs.length > 500) _logs.removeRange(0, _logs.length - 500);
        });

        if (addedNew) {
          Future.delayed(const Duration(milliseconds: 100), () {
            if (_scrollController.hasClients) {
              _scrollController.animateTo(
                _scrollController.position.maxScrollExtent,
                duration: const Duration(milliseconds: 200),
                curve: Curves.easeOut,
              );
            }
          });
        }
      } catch (e) {
        setState(() {
          final errorMsg = '[CRITICAL] Task Manager polling failed: $e';
          if (!_seenOutputs.contains(errorMsg)) {
             _seenOutputs.add(errorMsg);
             _logs.add(errorMsg);
             if (_logs.length > 500) _logs.removeRange(0, _logs.length - 500);
          }
        });
      }
    });
  }

  @override
  void dispose() {
    _pollingTimer?.cancel();
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Task Manager'),
      ),
      body: Container(
        color: const Color(0xFF1E1E1E), // Darker grey for Task Manager
        padding: const EdgeInsets.all(16.0),
        child: ListView.builder(
          controller: _scrollController,
          itemCount: _logs.length,
          itemBuilder: (context, index) {
            final log = _logs[index];
            Color textColor = Colors.cyanAccent;
            if (log.contains('[INFO')) textColor = Colors.greenAccent;
            if (log.contains('[CRITICAL')) textColor = Colors.redAccent;
            if (log.contains('[DIAGNOSTIC')) textColor = Colors.orangeAccent;
            if (log.contains('Agent')) textColor = Colors.amberAccent;
            
            return Padding(
              padding: const EdgeInsets.symmetric(vertical: 2.0),
              child: Text(
                log,
                style: TextStyle(
                  fontFamily: 'monospace',
                  color: textColor,
                  fontSize: 14,
                ),
              ),
            );
          },
        ),
      ),
    );
  }
}
