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
          seedColor: const Color(0xFFD32F2F), // Crimson / Blood Red accent
          brightness: Brightness.dark,
          surface: const Color(0xFF141414), // Deep charcoal surface
        ),
        scaffoldBackgroundColor: const Color(0xFF0D0D0D),
        appBarTheme: const AppBarTheme(
          backgroundColor: Colors.transparent,
          elevation: 0,
          centerTitle: true,
          foregroundColor: Color(0xFFE0E0E0),
        ),
        cardTheme: CardThemeData(
          color: const Color(0xFF1A1A1A), // Slightly lighter than background
          elevation: 8,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(16),
            side: const BorderSide(color: Color(0xFF2A2A2A), width: 1), // Subtle border
          ),
        ),
        useMaterial3: true,
      ),
      home: const CoreDashboard(),
    );
  }
}
