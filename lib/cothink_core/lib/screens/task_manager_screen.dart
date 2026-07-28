import 'package:flutter/material.dart';
import '../services/runtime_manager.dart';

class TaskManagerScreen extends StatefulWidget {
  const TaskManagerScreen({super.key});

  @override
  State<TaskManagerScreen> createState() => _TaskManagerScreenState();
}

class _TaskManagerScreenState extends State<TaskManagerScreen> {
  final List<String> _logs = [];
  final ScrollController _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    RuntimeManager().classifierStream.stream.listen((log) {
      if (!mounted) return;
      setState(() {
        _logs.addAll(log.split('\n').where((s) => s.trim().isNotEmpty));
        if (_logs.length > 500) _logs.removeRange(0, _logs.length - 500);
      });
      Future.delayed(const Duration(milliseconds: 100), () {
        if (_scrollController.hasClients) {
          _scrollController.animateTo(
            _scrollController.position.maxScrollExtent,
            duration: const Duration(milliseconds: 200),
            curve: Curves.easeOut,
          );
        }
      });
    });
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
            if (log.contains('Agent')) textColor = Colors.amberAccent;
            if (log.contains('Threshold crossed: true')) textColor = Colors.redAccent;
            
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
