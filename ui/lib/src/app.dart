import 'dart:ui';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_gate.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/core/routing/router.dart';
import 'package:myconnect_ui/src/features/pairing/incoming_pairing_prompt.dart';

class MyConnectApp extends ConsumerStatefulWidget {
  const new({super.key});

  @override
  ConsumerState<MyConnectApp> createState() => _MyConnectAppState();
}

class _MyConnectAppState extends ConsumerState<MyConnectApp> {
  late final AppLifecycleListener _lifecycle;

  @override
  void initState() {
    super.initState();
    // Give the embedded daemon a chance to close connections and clean up
    // partial transfers before the process exits. (Disposing the
    // ProviderScope also stops it, via daemonEndpointProvider.)
    _lifecycle = AppLifecycleListener(
      onExitRequested: () async {
        await ref.read(daemonHostProvider).stop();
        return AppExitResponse.exit;
      },
    );
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    const seed = Colors.indigo;
    return MaterialApp.router(
      title: 'MyConnect',
      theme: ThemeData(colorSchemeSeed: seed),
      darkTheme: ThemeData(colorSchemeSeed: seed, brightness: Brightness.dark),
      routerConfig: ref.watch(routerProvider),
      builder: (context, child) =>
          DaemonGate(child: IncomingPairingPrompt(child: child!)),
    );
  }
}
