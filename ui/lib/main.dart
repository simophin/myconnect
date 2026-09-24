import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/app.dart';

void main() {
  Logger.root.level = kDebugMode ? Level.FINE : Level.INFO;
  Logger.root.onRecord.listen(
    (record) => debugPrint(
      '${record.level.name} ${record.loggerName}: ${record.message}',
    ),
  );
  runApp(
    const ProviderScope(
      // Failures surface in the UI with an explicit Retry instead of being
      // retried silently (Riverpod 3 retries failing providers by default).
      retry: _noRetry,
      child: MyConnectApp(),
    ),
  );
}

Duration? _noRetry(int retryCount, Object error) => null;
