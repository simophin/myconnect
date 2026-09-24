import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_gate.dart';
import 'package:myconnect_ui/src/core/routing/router.dart';
import 'package:myconnect_ui/src/features/background/background_host.dart';
import 'package:myconnect_ui/src/features/pairing/incoming_pairing_prompt.dart';

/// The app inside its `ProviderScope`, as `main` runs it.
class MyConnectRoot extends StatelessWidget {
  const new({this.overrides = const [], super.key});

  /// Providers to replace, e.g. the daemon host in integration tests.
  final List<Override> overrides;

  @override
  Widget build(BuildContext context) => ProviderScope(
    // Failures surface in the UI with an explicit Retry instead of being
    // retried silently (Riverpod 3 retries failing providers by default).
    retry: _noRetry,
    overrides: overrides,
    child: const MyConnectApp(),
  );
}

Duration? _noRetry(int retryCount, Object error) => null;

class MyConnectApp extends ConsumerWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    const seed = Colors.indigo;
    return MaterialApp.router(
      title: 'MyConnect',
      theme: ThemeData(colorSchemeSeed: seed),
      darkTheme: ThemeData(colorSchemeSeed: seed, brightness: Brightness.dark),
      routerConfig: ref.watch(routerProvider),
      builder: (context, child) => BackgroundHost(
        child: DaemonGate(child: IncomingPairingPrompt(child: child!)),
      ),
    );
  }
}
