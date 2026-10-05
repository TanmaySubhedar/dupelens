import 'dart:async';

import 'package:flutter/material.dart';

import '../service.dart';
import 'results_screen.dart';
import 'welcome_screen.dart';

/// Runs a scan with live progress, then hands over to the results.
class ScanScreen extends StatefulWidget {
  const ScanScreen({super.key, required this.service});

  final DupeLensService service;

  @override
  State<ScanScreen> createState() => _ScanScreenState();
}

class _ScanScreenState extends State<ScanScreen> {
  StreamSubscription<ScanEvent>? _sub;
  ScanProgress? _progress;
  String? _error;

  @override
  void initState() {
    super.initState();
    _start();
  }

  void _start() {
    setState(() {
      _error = null;
      _progress = null;
    });
    _sub = widget.service.scan().listen(
      (event) async {
        switch (event) {
          case ScanProgressEvent(:final progress):
            setState(() => _progress = progress);
          case ScanDoneEvent(:final summary):
            final groups = await widget.service.groups();
            if (!mounted) return;
            Navigator.of(context).pushReplacement(MaterialPageRoute(
              builder: (_) => ResultsScreen(
                service: widget.service,
                groups: groups,
                summary: summary,
              ),
            ));
        }
      },
      onError: (Object e) => setState(() => _error = '$e'),
    );
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  String get _label {
    final p = _progress;
    if (p == null || p.phase == ScanPhase.discovering) return 'Looking for photos…';
    if (p.phase == ScanPhase.saving) return 'Finishing up…';
    if (p.total == 0) return 'Everything is up to date';
    return 'Checking photo ${p.done} of ${p.total}';
  }

  @override
  Widget build(BuildContext context) {
    final p = _progress;
    final fraction = (p != null && p.phase == ScanPhase.analyzing && p.total > 0)
        ? p.done / p.total
        : null;
    return Scaffold(
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: _error != null
                ? Column(mainAxisSize: MainAxisSize.min, children: [
                    Text('The scan stopped: $_error', textAlign: TextAlign.center),
                    const SizedBox(height: 16),
                    FilledButton(onPressed: _start, child: const Text('Try again')),
                    TextButton(
                      onPressed: () => Navigator.of(context).push(MaterialPageRoute(
                        builder: (_) =>
                            WelcomeScreen(service: widget.service, firstLaunch: false),
                      )),
                      child: const Text('Change folders'),
                    ),
                  ])
                : Column(mainAxisSize: MainAxisSize.min, children: [
                    LinearProgressIndicator(value: fraction),
                    const SizedBox(height: 16),
                    Text(_label, style: Theme.of(context).textTheme.titleMedium),
                    const SizedBox(height: 8),
                    Text('Your photos stay on this device.',
                        style: Theme.of(context).textTheme.bodySmall),
                  ]),
          ),
        ),
      ),
    );
  }
}
