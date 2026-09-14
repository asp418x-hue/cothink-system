import 'package:flutter/material.dart';
import 'dart:io';
import '../../services/path_utils.dart';

class ProcessMonitorCard extends StatelessWidget {
  const ProcessMonitorCard({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: InkWell(
        borderRadius: BorderRadius.circular(16),
        onTap: () {
          if (!Platform.isAndroid) {
            Process.run('bash', [PathUtils.scriptPath('pip_btop.sh'), 'start']);
          }
          Navigator.pushNamed(context, '/tasks');
        },
        child: Padding(
          padding: const EdgeInsets.all(24.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.memory, color: Theme.of(context).colorScheme.primary, size: 32),
              const SizedBox(height: 16),
              const Text('Process Monitor', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
              const SizedBox(height: 4),
              const Text('Task Manager & btop', style: TextStyle(color: Colors.grey, fontSize: 12)),
            ],
          ),
        ),
      ),
    );
  }
}
