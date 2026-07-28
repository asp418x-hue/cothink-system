import 'package:flutter/material.dart';
import '../services/runtime_manager.dart';

class ThermalScreen extends StatefulWidget {
  const ThermalScreen({super.key});

  @override
  State<ThermalScreen> createState() => _ThermalScreenState();
}

class _ThermalScreenState extends State<ThermalScreen> {
  final List<String> _logs = [];
  final ScrollController _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    RuntimeManager().thermalStream.stream.listen((log) {
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
        title: const Text('Thermal Monitor'),
      ),
      body: Container(
        color: Colors.black87,
        padding: const EdgeInsets.all(16.0),
        child: ListView.builder(
          controller: _scrollController,
          itemCount: _logs.length,
          itemBuilder: (context, index) {
            final log = _logs[index];
            Color textColor = Colors.green;
            if (log.contains('Warning') || log.contains('Throttling')) textColor = Colors.orange;
            if (log.contains('Critical') || log.contains('Exceeded')) textColor = Colors.red;
            
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
