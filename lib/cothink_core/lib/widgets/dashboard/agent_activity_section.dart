import 'package:flutter/material.dart';

class AgentActivitySection extends StatelessWidget {
  final List<dynamic> agents;
  final int activeWorkers;

  const AgentActivitySection({
    super.key,
    required this.agents,
    required this.activeWorkers,
  });

  @override
  Widget build(BuildContext context) {
    if (agents.isEmpty) return const SizedBox.shrink();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 32),
        Row(
          children: [
            const Text(
              'Subagent Activity',
              style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold, letterSpacing: -0.5),
            ),
            const Spacer(),
            if (activeWorkers > 0)
              Chip(
                label: Text('$activeWorkers Active', style: const TextStyle(fontSize: 12)),
                backgroundColor: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                side: BorderSide.none,
              ),
          ],
        ),
        const SizedBox(height: 16),
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: agents.map((agent) {
            final int id = agent['id'];
            final metadata = agent['metadata'] as Map<String, dynamic>? ?? {};
            final String output = metadata['output'] ?? '';
            
            bool isDone = output.isNotEmpty;
            bool isAnomaly = output.contains('Threshold crossed: true');
            
            Color cardColor;
            if (!isDone) {
              cardColor = Colors.grey.withOpacity(0.1);
            } else if (isAnomaly) {
              cardColor = Colors.red.withOpacity(0.1);
            } else {
              cardColor = Colors.green.withOpacity(0.1);
            }
            
            Color iconColor;
            if (!isDone) {
              iconColor = Colors.grey;
            } else if (isAnomaly) {
              iconColor = Colors.red;
            } else {
              iconColor = Colors.green;
            }

            return Container(
              width: 160,
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: cardColor,
                borderRadius: BorderRadius.circular(16),
                border: Border.all(
                  color: isDone ? iconColor.withOpacity(0.3) : Colors.transparent,
                ),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(isDone ? (isAnomaly ? Icons.warning : Icons.check_circle) : Icons.pending, color: iconColor, size: 18),
                      const SizedBox(width: 8),
                      Text('Agent $id', style: const TextStyle(fontWeight: FontWeight.bold)),
                    ],
                  ),
                  const SizedBox(height: 12),
                  Text(
                    isDone ? (isAnomaly ? 'Anomaly Detected' : 'Normal') : 'Analyzing...',
                    style: TextStyle(fontSize: 12, color: isDone ? iconColor : Colors.grey, fontWeight: FontWeight.w600),
                  ),
                ],
              ),
            );
          }).toList(),
        ),
      ],
    );
  }
}
