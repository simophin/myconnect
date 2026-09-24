import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_gate.dart';
import 'package:myconnect_ui/src/core/routing/router.dart';
import 'package:myconnect_ui/src/features/background/background_host.dart';
import 'package:myconnect_ui/src/features/pairing/incoming_pairing_prompt.dart';

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
