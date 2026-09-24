import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:myconnect_ui/src/features/devices/add_device_page.dart';
import 'package:myconnect_ui/src/features/devices/device_detail_page.dart';
import 'package:myconnect_ui/src/features/devices/devices_page.dart';
import 'package:myconnect_ui/src/features/pairing/pairing_page.dart';

/// Route tree. Paths nest the way screens stack, so the app bar's back
/// button walks up it.
final routerProvider = Provider<GoRouter>((ref) {
  final router = GoRouter(
    routes: [
      GoRoute(
        path: '/',
        builder: (context, state) => const DevicesPage(),
        routes: [
          GoRoute(
            path: 'devices/:deviceId',
            builder: (context, state) =>
                DeviceDetailPage(deviceId: state.pathParameters['deviceId']!),
          ),
          GoRoute(
            path: 'add',
            builder: (context, state) => const AddDevicePage(),
            routes: [
              GoRoute(
                path: 'pairing/:pairingId',
                builder: (context, state) =>
                    PairingPage(pairingId: state.pathParameters['pairingId']!),
              ),
            ],
          ),
        ],
      ),
    ],
  );
  ref.onDispose(router.dispose);
  return router;
});
