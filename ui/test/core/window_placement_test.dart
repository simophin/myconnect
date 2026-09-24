import 'dart:io';
import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:myconnect_ui/src/core/desktop/window_placement.dart';

void main() {
  late Directory dir;
  late WindowPlacementStore store;

  setUp(() async {
    dir = await Directory.systemTemp.createTemp('window-placement-');
    store = WindowPlacementStore(File('${dir.path}/state/window.json'));
  });

  tearDown(() => dir.delete(recursive: true));

  test('nothing is saved at first', () async {
    expect(await store.load(), isNull);
  });

  test('a saved placement reads back the same', () async {
    const placement = WindowPlacement(
      visible: false,
      bounds: Rect.fromLTWH(120, 80, 900, 600),
      maximized: true,
    );
    await store.save(placement);
    expect(await store.load(), placement);

    const shown = WindowPlacement(visible: true);
    await store.save(shown);
    expect(await store.load(), shown);
  });

  test('an unreadable file counts as nothing saved', () async {
    await store.file.parent.create(recursive: true);
    await store.file.writeAsString('{"visible": "yes"');
    expect(await store.load(), isNull);
    await store.file.writeAsString('{"visible": true, "bounds": [1, 2, 0, 4]}');
    expect(await store.load(), const WindowPlacement(visible: true));
  });

  group('fitsOnScreen', () {
    const left = Rect.fromLTWH(0, 0, 1920, 1080);
    const right = Rect.fromLTWH(1920, 0, 2560, 1440);

    test('on either screen', () {
      expect(
        fitsOnScreen(const Rect.fromLTWH(100, 100, 800, 600), [left]),
        isTrue,
      );
      expect(
        fitsOnScreen(const Rect.fromLTWH(2500, 300, 800, 600), [left, right]),
        isTrue,
      );
    });

    test('not on a screen that was unplugged', () {
      expect(
        fitsOnScreen(const Rect.fromLTWH(2500, 300, 800, 600), [left]),
        isFalse,
      );
    });

    test('not with only a sliver showing', () {
      expect(
        fitsOnScreen(const Rect.fromLTWH(1880, 100, 800, 600), [left]),
        isFalse,
      );
    });
  });
}
