// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'remote_file.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_RemoteFile _$RemoteFileFromJson(Map<String, dynamic> json) => _RemoteFile(
  name: json['name'] as String,
  path: json['path'] as String,
  kind: $enumDecode(
    _$FileKindEnumMap,
    json['kind'],
    unknownValue: FileKind.unknown,
  ),
  size: (json['size'] as num?)?.toInt(),
  modifiedAt: (json['modifiedAt'] as num?)?.toInt(),
);

Map<String, dynamic> _$RemoteFileToJson(_RemoteFile instance) =>
    <String, dynamic>{
      'name': instance.name,
      'path': instance.path,
      'kind': _$FileKindEnumMap[instance.kind]!,
      'size': instance.size,
      'modifiedAt': instance.modifiedAt,
    };

const _$FileKindEnumMap = {
  FileKind.file: 'file',
  FileKind.directory: 'directory',
  FileKind.symlink: 'symlink',
  FileKind.other: 'other',
  FileKind.unknown: 'unknown',
};

_DirectoryListing _$DirectoryListingFromJson(Map<String, dynamic> json) =>
    _DirectoryListing(
      entries: (json['entries'] as List<dynamic>)
          .map((e) => RemoteFile.fromJson(e as Map<String, dynamic>))
          .toList(),
      path: json['path'] as String?,
    );

Map<String, dynamic> _$DirectoryListingToJson(_DirectoryListing instance) =>
    <String, dynamic>{'entries': instance.entries, 'path': instance.path};
