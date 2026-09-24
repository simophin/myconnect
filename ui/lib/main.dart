import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/app.dart';

void main() {
  Logger.root.level = kDebugMode ? Level.FINE : Level.INFO;
  Logger.root.onRecord.listen(
    (record) => debugPrint(
      '${record.level.name} ${record.loggerName}: ${record.message}',
    ),
  );
  runApp(const MyConnectRoot());
}
