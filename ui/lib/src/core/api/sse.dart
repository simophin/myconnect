import 'dart:async';
import 'dart:convert';

/// Splits a Server-Sent Events byte stream into the `data` payload of each
/// event. Comment lines (the daemon's `:keepalive`), `event:` and `id:` fields
/// are dropped: the JSON payload already names its own type and sequence.
StreamTransformer<B, String> sseDataTransformer<B extends List<int>>() =>
    StreamTransformer.fromBind(
      (bytes) => bytes
          .cast<List<int>>()
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .transform(_SseFrameTransformer()),
    );

class _SseFrameTransformer extends StreamTransformerBase<String, String> {
  @override
  Stream<String> bind(Stream<String> lines) async* {
    final data = <String>[];
    await for (final line in lines) {
      if (line.isEmpty) {
        if (data.isNotEmpty) yield data.join('\n');
        data.clear();
      } else if (line.startsWith('data:')) {
        final value = line.substring(5);
        data.add(value.startsWith(' ') ? value.substring(1) : value);
      }
    }
  }
}
