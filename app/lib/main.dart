import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:path_provider/path_provider.dart';

import 'src/app.dart';
import 'src/rust/frb_generated.dart';
import 'src/rust_service.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final dir = await getApplicationSupportDirectory();
  final service = RustDupeLensService.open(
    '${dir.path}${Platform.pathSeparator}dupelens.db',
  );
  runApp(DupeLensApp(service: service));
}
