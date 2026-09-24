import 'package:freezed_annotation/freezed_annotation.dart';

part 'remote_file.freezed.dart';
part 'remote_file.g.dart';

@JsonEnum(fieldRename: FieldRename.snake)
enum FileKind { file, directory, symlink, other, unknown }

/// Mirror of the daemon's `FileEntry`: one file or directory on a peer.
/// [path] is absolute on the peer; [modifiedAt] is Unix milliseconds.
@freezed
abstract class RemoteFile with _$RemoteFile {
  const factory({
    required String name,
    required String path,
    @JsonKey(unknownEnumValue: FileKind.unknown) required FileKind kind,
    int? size,
    int? modifiedAt,
  }) = _RemoteFile;

  const new _();

  factory fromJson(Map<String, Object?> json) => _$RemoteFileFromJson(json);

  bool get isDirectory => kind == FileKind.directory;
}

/// Mirror of the daemon's `DirectoryListing`: a directory's entries, or,
/// with no [path], the storage roots the peer shares.
@freezed
abstract class DirectoryListing with _$DirectoryListing {
  const factory({required List<RemoteFile> entries, String? path}) =
      _DirectoryListing;

  factory fromJson(Map<String, Object?> json) =>
      _$DirectoryListingFromJson(json);
}
