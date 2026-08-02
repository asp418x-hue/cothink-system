import 'package:flutter/material.dart';
import 'dart:io';

class ProcessMonitorCard extends StatelessWidget {
  const ProcessMonitorCard({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: InkWell(
        borderRadius: BorderRadius.circular(16),
        onTap: () {
          Process.run('bash', ['/home/asp418x/cothink-system/pip_btop.sh', 'start']);
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
              const Text('btop snap-in', style: TextStyle(color: Colors.grey, fontSize: 12)),
            ],
          ),
        ),
      ),
    );
  }
}
