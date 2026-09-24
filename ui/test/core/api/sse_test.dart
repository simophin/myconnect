import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:myconnect_ui/src/core/api/sse.dart';

Future<List<String>> parse(List<String> chunks) =>
    Stream.fromIterable(chunks.map(utf8.encode))
        .transform(sseDataTransformer<Uint8List>())
        .toList();

void main() {
  test('yields the data of each frame and skips comments and fields', () async {
    expect(
      await parse([
        ':keepalive\n\n',
        'id: 1\nevent: device.updated\ndata: {"a":1}\n\n',
        'data:{"b":2}\n\n',
      ]),
      ['{"a":1}', '{"b":2}'],
    );
  });

  test(
    'reassembles frames split across chunks and CRLF line endings',
    () async {
      expect(
        await parse(['data: {"a"', ':1}\r\n', '\r\n', 'data: x\n', '\n']),
        ['{"a":1}', 'x'],
      );
    },
  );

  test('joins multi-line data with newlines', () async {
    expect(await parse(['data: one\ndata: two\n\n']), ['one\ntwo']);
  });

  test('drops an unterminated trailing frame', () async {
    expect(await parse(['data: partial\n']), isEmpty);
  });
}
