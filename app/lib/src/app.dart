import 'package:flutter/material.dart';

import 'screens/scan_screen.dart';
import 'screens/welcome_screen.dart';
import 'service.dart';

class DupeLensApp extends StatelessWidget {
  const DupeLensApp({super.key, required this.service});

  final DupeLensService service;

  @override
  Widget build(BuildContext context) {
    ThemeData theme(Brightness b) => ThemeData(
          colorSchemeSeed: const Color(0xFF3B6FD8),
          brightness: b,
          useMaterial3: true,
        );
    return MaterialApp(
      title: 'DupeLens',
      theme: theme(Brightness.light),
      darkTheme: theme(Brightness.dark),
      home: _StartGate(service: service),
    );
  }
}

/// First launch (no folders yet) goes to the welcome screen; later launches go straight
/// to a quick incremental rescan of the remembered folders.
class _StartGate extends StatelessWidget {
  const _StartGate({required this.service});

  final DupeLensService service;

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<String>>(
      future: service.folders(),
      builder: (context, snap) {
        if (!snap.hasData) {
          return const Scaffold(body: Center(child: CircularProgressIndicator()));
        }
        return snap.data!.isEmpty
            ? WelcomeScreen(service: service)
            : ScanScreen(service: service);
      },
    );
  }
}
