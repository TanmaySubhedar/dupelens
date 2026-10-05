import 'package:flutter/widgets.dart';

import 'src/app.dart';
import 'src/demo_service.dart';

/// Runs the UI on made-up data, without the Rust engine: `flutter run -t lib/main_demo.dart`.
void main() => runApp(DupeLensApp(service: DemoDupeLensService()));
