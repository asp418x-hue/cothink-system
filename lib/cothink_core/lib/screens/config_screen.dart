import 'package:flutter/material.dart';
import 'dart:io';
import '../services/path_utils.dart';

class ConfigScreen extends StatelessWidget {
  const ConfigScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Configuration', style: TextStyle(fontWeight: FontWeight.w600)),
      ),
      body: ListView(
        padding: const EdgeInsets.all(24.0),
        children: [
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text('Theme Preferences', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                  const SizedBox(height: 16),
                  SwitchListTile(
                    title: const Text('Enable Dark Blood Theme'),
                    subtitle: const Text('Uses MX Linux inspired colors'),
                    value: true,
                    onChanged: (val) {},
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text('System Settings', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                  const SizedBox(height: 16),
                  ListTile(
                    leading: const Icon(Icons.memory),
                    title: const Text('Resource Monitor'),
                    subtitle: const Text('Floating btop window'),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () {
                      if (!Platform.isAndroid) {
                        Process.run('bash', [PathUtils.scriptPath('pip_btop.sh'), 'start']);
                      }
                    },
                  ),
                  ListTile(
                    leading: const Icon(Icons.analytics_outlined),
                    title: const Text('Task Manager'),
                    subtitle: const Text('Live agent log and anomaly stream'),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () {
                      Navigator.pushNamed(context, '/tasks');
                    },
                  ),
                  ListTile(
                    leading: const Icon(Icons.terminal),
                    title: const Text('Default Editor'),
                    subtitle: const Text('Neovim'),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () {},
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
