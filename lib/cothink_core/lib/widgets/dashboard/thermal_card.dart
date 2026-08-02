import 'package:flutter/material.dart';
import '../screens/thermal_screen.dart';

class ThermalMonitorCard extends StatelessWidget {
  const ThermalMonitorCard({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: InkWell(
        borderRadius: BorderRadius.circular(16),
        onTap: () {
          Navigator.push(
            context,
            MaterialPageRoute(builder: (context) => const ThermalScreen()),
          );
        },
        child: Padding(
          padding: const EdgeInsets.all(24.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.thermostat, color: Theme.of(context).colorScheme.primary, size: 32),
              const SizedBox(height: 16),
              const Text('Thermal Monitor', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
              const SizedBox(height: 4),
              const Text('System temps & fans', style: TextStyle(color: Colors.grey, fontSize: 12)),
            ],
          ),
        ),
      ),
    );
  }
}
