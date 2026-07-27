import 'package:flutter/material.dart';
import 'screens/dashboard.dart';

void main() {
  runApp(const CothinkApp());
}

class CothinkApp extends StatelessWidget {
  const CothinkApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Cothink System',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.deepPurple,
          brightness: Brightness.dark,
        ),
        useMaterial3: true,
      ),
      home: const CoreDashboard(),
    );
  }
}
