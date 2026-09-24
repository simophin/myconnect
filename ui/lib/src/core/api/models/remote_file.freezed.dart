// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'remote_file.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$RemoteFile {

 String get name; String get path;@JsonKey(unknownEnumValue: FileKind.unknown) FileKind get kind; int? get size; int? get modifiedAt;
/// Create a copy of RemoteFile
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RemoteFileCopyWith<RemoteFile> get copyWith => _$RemoteFileCopyWithImpl<RemoteFile>(this as RemoteFile, _$identity);

  /// Serializes this RemoteFile to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as RemoteFile;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is RemoteFile&&(identical(other.name, _this.name) || other.name == _this.name)&&(identical(other.path, _this.path) || other.path == _this.path)&&(identical(other.kind, _this.kind) || other.kind == _this.kind)&&(identical(other.size, _this.size) || other.size == _this.size)&&(identical(other.modifiedAt, _this.modifiedAt) || other.modifiedAt == _this.modifiedAt));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as RemoteFile;
  return Object.hash(runtimeType,_this.name,_this.path,_this.kind,_this.size,_this.modifiedAt);
}

@override
String toString() {
  final _this = this as RemoteFile;
  return 'RemoteFile(name: ${_this.name}, path: ${_this.path}, kind: ${_this.kind}, size: ${_this.size}, modifiedAt: ${_this.modifiedAt})';
}


}

/// @nodoc
abstract mixin class $RemoteFileCopyWith<$Res>  {
  factory $RemoteFileCopyWith(RemoteFile value, $Res Function(RemoteFile) _then) = _$RemoteFileCopyWithImpl;
@useResult
$Res call({
 String name, String path,@JsonKey(unknownEnumValue: FileKind.unknown) FileKind kind, int? size, int? modifiedAt
});




}
/// @nodoc
class _$RemoteFileCopyWithImpl<$Res>
    implements $RemoteFileCopyWith<$Res> {
  _$RemoteFileCopyWithImpl(this._self, this._then);

  final RemoteFile _self;
  final $Res Function(RemoteFile) _then;

/// Create a copy of RemoteFile
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? name = null,Object? path = null,Object? kind = null,Object? size = freezed,Object? modifiedAt = freezed,}) {
  return _then(RemoteFile(
name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as FileKind,size: freezed == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as int?,modifiedAt: freezed == modifiedAt ? _self.modifiedAt : modifiedAt // ignore: cast_nullable_to_non_nullable
as int?,
  ));
}

}


/// Adds pattern-matching-related methods to [RemoteFile].
extension RemoteFilePatterns on RemoteFile {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _RemoteFile value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _RemoteFile() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _RemoteFile value)  $default,){
final _that = this;
switch (_that) {
case _RemoteFile():
return $default(_that);case _:
  throw StateError('Unexpected subclass');

}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _RemoteFile value)?  $default,){
final _that = this;
switch (_that) {
case _RemoteFile() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( String name,  String path, @JsonKey(unknownEnumValue: FileKind.unknown)  FileKind kind,  int? size,  int? modifiedAt)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _RemoteFile() when $default != null:
return $default(_that.name,_that.path,_that.kind,_that.size,_that.modifiedAt);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( String name,  String path, @JsonKey(unknownEnumValue: FileKind.unknown)  FileKind kind,  int? size,  int? modifiedAt)  $default,) {final _that = this;
switch (_that) {
case _RemoteFile():
return $default(_that.name,_that.path,_that.kind,_that.size,_that.modifiedAt);case _:
  throw StateError('Unexpected subclass');

}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( String name,  String path, @JsonKey(unknownEnumValue: FileKind.unknown)  FileKind kind,  int? size,  int? modifiedAt)?  $default,) {final _that = this;
switch (_that) {
case _RemoteFile() when $default != null:
return $default(_that.name,_that.path,_that.kind,_that.size,_that.modifiedAt);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _RemoteFile extends RemoteFile {
  const _RemoteFile({required this.name, required this.path, @JsonKey(unknownEnumValue: FileKind.unknown) required this.kind, this.size, this.modifiedAt}): super._();
  factory _RemoteFile.fromJson(Map<String, dynamic> json) => _$RemoteFileFromJson(json);

@override final  String name;
@override final  String path;
@override@JsonKey(unknownEnumValue: FileKind.unknown) final  FileKind kind;
@override final  int? size;
@override final  int? modifiedAt;

/// Create a copy of RemoteFile
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$RemoteFileCopyWith<_RemoteFile> get copyWith => __$RemoteFileCopyWithImpl<_RemoteFile>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$RemoteFileToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _RemoteFile&&(identical(other.name, name) || other.name == name)&&(identical(other.path, path) || other.path == path)&&(identical(other.kind, kind) || other.kind == kind)&&(identical(other.size, size) || other.size == size)&&(identical(other.modifiedAt, modifiedAt) || other.modifiedAt == modifiedAt));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,name,path,kind,size,modifiedAt);
}

@override
String toString() {
    return 'RemoteFile(name: $name, path: $path, kind: $kind, size: $size, modifiedAt: $modifiedAt)';
}


}

/// @nodoc
abstract mixin class _$RemoteFileCopyWith<$Res> implements $RemoteFileCopyWith<$Res> {
  factory _$RemoteFileCopyWith(_RemoteFile value, $Res Function(_RemoteFile) _then) = __$RemoteFileCopyWithImpl;
@override @useResult
$Res call({
 String name, String path,@JsonKey(unknownEnumValue: FileKind.unknown) FileKind kind, int? size, int? modifiedAt
});




}
/// @nodoc
class __$RemoteFileCopyWithImpl<$Res>
    implements _$RemoteFileCopyWith<$Res> {
  __$RemoteFileCopyWithImpl(this._self, this._then);

  final _RemoteFile _self;
  final $Res Function(_RemoteFile) _then;

/// Create a copy of RemoteFile
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? name = null,Object? path = null,Object? kind = null,Object? size = freezed,Object? modifiedAt = freezed,}) {
  return _then(_RemoteFile(
name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as FileKind,size: freezed == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as int?,modifiedAt: freezed == modifiedAt ? _self.modifiedAt : modifiedAt // ignore: cast_nullable_to_non_nullable
as int?,
  ));
}


}


/// @nodoc
mixin _$DirectoryListing {

 List<RemoteFile> get entries; String? get path;
/// Create a copy of DirectoryListing
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$DirectoryListingCopyWith<DirectoryListing> get copyWith => _$DirectoryListingCopyWithImpl<DirectoryListing>(this as DirectoryListing, _$identity);

  /// Serializes this DirectoryListing to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as DirectoryListing;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is DirectoryListing&&const DeepCollectionEquality().equals(other.entries, _this.entries)&&(identical(other.path, _this.path) || other.path == _this.path));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as DirectoryListing;
  return Object.hash(runtimeType,const DeepCollectionEquality().hash(_this.entries),_this.path);
}

@override
String toString() {
  final _this = this as DirectoryListing;
  return 'DirectoryListing(entries: ${_this.entries}, path: ${_this.path})';
}


}

/// @nodoc
abstract mixin class $DirectoryListingCopyWith<$Res>  {
  factory $DirectoryListingCopyWith(DirectoryListing value, $Res Function(DirectoryListing) _then) = _$DirectoryListingCopyWithImpl;
@useResult
$Res call({
 List<RemoteFile> entries, String? path
});




}
/// @nodoc
class _$DirectoryListingCopyWithImpl<$Res>
    implements $DirectoryListingCopyWith<$Res> {
  _$DirectoryListingCopyWithImpl(this._self, this._then);

  final DirectoryListing _self;
  final $Res Function(DirectoryListing) _then;

/// Create a copy of DirectoryListing
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? entries = null,Object? path = freezed,}) {
  return _then(DirectoryListing(
entries: null == entries ? _self.entries : entries // ignore: cast_nullable_to_non_nullable
as List<RemoteFile>,path: freezed == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}

}


/// Adds pattern-matching-related methods to [DirectoryListing].
extension DirectoryListingPatterns on DirectoryListing {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _DirectoryListing value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _DirectoryListing() when $default != null:
return $default(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _DirectoryListing value)  $default,){
final _that = this;
switch (_that) {
case _DirectoryListing():
return $default(_that);case _:
  throw StateError('Unexpected subclass');

}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _DirectoryListing value)?  $default,){
final _that = this;
switch (_that) {
case _DirectoryListing() when $default != null:
return $default(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( List<RemoteFile> entries,  String? path)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _DirectoryListing() when $default != null:
return $default(_that.entries,_that.path);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( List<RemoteFile> entries,  String? path)  $default,) {final _that = this;
switch (_that) {
case _DirectoryListing():
return $default(_that.entries,_that.path);case _:
  throw StateError('Unexpected subclass');

}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( List<RemoteFile> entries,  String? path)?  $default,) {final _that = this;
switch (_that) {
case _DirectoryListing() when $default != null:
return $default(_that.entries,_that.path);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _DirectoryListing implements DirectoryListing {
  const _DirectoryListing({required  List<RemoteFile> entries, this.path}): _entries = entries;
  factory _DirectoryListing.fromJson(Map<String, dynamic> json) => _$DirectoryListingFromJson(json);

 final  List<RemoteFile> _entries;
@override List<RemoteFile> get entries {
  if (_entries is EqualUnmodifiableListView) return _entries;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_entries);
}

@override final  String? path;

/// Create a copy of DirectoryListing
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$DirectoryListingCopyWith<_DirectoryListing> get copyWith => __$DirectoryListingCopyWithImpl<_DirectoryListing>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$DirectoryListingToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _DirectoryListing&&const DeepCollectionEquality().equals(other.entries, _entries)&&(identical(other.path, path) || other.path == path));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,const DeepCollectionEquality().hash(_entries),path);
}

@override
String toString() {
    return 'DirectoryListing(entries: $entries, path: $path)';
}


}

/// @nodoc
abstract mixin class _$DirectoryListingCopyWith<$Res> implements $DirectoryListingCopyWith<$Res> {
  factory _$DirectoryListingCopyWith(_DirectoryListing value, $Res Function(_DirectoryListing) _then) = __$DirectoryListingCopyWithImpl;
@override @useResult
$Res call({
 List<RemoteFile> entries, String? path
});




}
/// @nodoc
class __$DirectoryListingCopyWithImpl<$Res>
    implements _$DirectoryListingCopyWith<$Res> {
  __$DirectoryListingCopyWithImpl(this._self, this._then);

  final _DirectoryListing _self;
  final $Res Function(_DirectoryListing) _then;

/// Create a copy of DirectoryListing
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? entries = null,Object? path = freezed,}) {
  return _then(_DirectoryListing(
entries: null == entries ? _self._entries : entries // ignore: cast_nullable_to_non_nullable
as List<RemoteFile>,path: freezed == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

// dart format on
