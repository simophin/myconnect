// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'transfer.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$Transfer {

 String get id; String get deviceId; String get deviceName;@JsonKey(unknownEnumValue: TransferDirection.unknown) TransferDirection get direction;@JsonKey(unknownEnumValue: TransferStatus.unknown) TransferStatus get status; String get fileName; int get totalBytes; int get transferredBytes; int get createdAt; int get updatedAt; String? get errorCode;/// Where a completed incoming file was saved.
 String? get savedPath;
/// Create a copy of Transfer
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TransferCopyWith<Transfer> get copyWith => _$TransferCopyWithImpl<Transfer>(this as Transfer, _$identity);

  /// Serializes this Transfer to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as Transfer;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Transfer&&(identical(other.id, _this.id) || other.id == _this.id)&&(identical(other.deviceId, _this.deviceId) || other.deviceId == _this.deviceId)&&(identical(other.deviceName, _this.deviceName) || other.deviceName == _this.deviceName)&&(identical(other.direction, _this.direction) || other.direction == _this.direction)&&(identical(other.status, _this.status) || other.status == _this.status)&&(identical(other.fileName, _this.fileName) || other.fileName == _this.fileName)&&(identical(other.totalBytes, _this.totalBytes) || other.totalBytes == _this.totalBytes)&&(identical(other.transferredBytes, _this.transferredBytes) || other.transferredBytes == _this.transferredBytes)&&(identical(other.createdAt, _this.createdAt) || other.createdAt == _this.createdAt)&&(identical(other.updatedAt, _this.updatedAt) || other.updatedAt == _this.updatedAt)&&(identical(other.errorCode, _this.errorCode) || other.errorCode == _this.errorCode)&&(identical(other.savedPath, _this.savedPath) || other.savedPath == _this.savedPath));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as Transfer;
  return Object.hash(runtimeType,_this.id,_this.deviceId,_this.deviceName,_this.direction,_this.status,_this.fileName,_this.totalBytes,_this.transferredBytes,_this.createdAt,_this.updatedAt,_this.errorCode,_this.savedPath);
}

@override
String toString() {
  final _this = this as Transfer;
  return 'Transfer(id: ${_this.id}, deviceId: ${_this.deviceId}, deviceName: ${_this.deviceName}, direction: ${_this.direction}, status: ${_this.status}, fileName: ${_this.fileName}, totalBytes: ${_this.totalBytes}, transferredBytes: ${_this.transferredBytes}, createdAt: ${_this.createdAt}, updatedAt: ${_this.updatedAt}, errorCode: ${_this.errorCode}, savedPath: ${_this.savedPath})';
}


}

/// @nodoc
abstract mixin class $TransferCopyWith<$Res>  {
  factory $TransferCopyWith(Transfer value, $Res Function(Transfer) _then) = _$TransferCopyWithImpl;
@useResult
$Res call({
 String id, String deviceId, String deviceName,@JsonKey(unknownEnumValue: TransferDirection.unknown) TransferDirection direction,@JsonKey(unknownEnumValue: TransferStatus.unknown) TransferStatus status, String fileName, int totalBytes, int transferredBytes, int createdAt, int updatedAt, String? errorCode, String? savedPath
});




}
/// @nodoc
class _$TransferCopyWithImpl<$Res>
    implements $TransferCopyWith<$Res> {
  _$TransferCopyWithImpl(this._self, this._then);

  final Transfer _self;
  final $Res Function(Transfer) _then;

/// Create a copy of Transfer
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? id = null,Object? deviceId = null,Object? deviceName = null,Object? direction = null,Object? status = null,Object? fileName = null,Object? totalBytes = null,Object? transferredBytes = null,Object? createdAt = null,Object? updatedAt = null,Object? errorCode = freezed,Object? savedPath = freezed,}) {
  return _then(Transfer(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,direction: null == direction ? _self.direction : direction // ignore: cast_nullable_to_non_nullable
as TransferDirection,status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as TransferStatus,fileName: null == fileName ? _self.fileName : fileName // ignore: cast_nullable_to_non_nullable
as String,totalBytes: null == totalBytes ? _self.totalBytes : totalBytes // ignore: cast_nullable_to_non_nullable
as int,transferredBytes: null == transferredBytes ? _self.transferredBytes : transferredBytes // ignore: cast_nullable_to_non_nullable
as int,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as int,updatedAt: null == updatedAt ? _self.updatedAt : updatedAt // ignore: cast_nullable_to_non_nullable
as int,errorCode: freezed == errorCode ? _self.errorCode : errorCode // ignore: cast_nullable_to_non_nullable
as String?,savedPath: freezed == savedPath ? _self.savedPath : savedPath // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}

}


/// Adds pattern-matching-related methods to [Transfer].
extension TransferPatterns on Transfer {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _Transfer value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _Transfer() when $default != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _Transfer value)  $default,){
final _that = this;
switch (_that) {
case _Transfer():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _Transfer value)?  $default,){
final _that = this;
switch (_that) {
case _Transfer() when $default != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( String id,  String deviceId,  String deviceName, @JsonKey(unknownEnumValue: TransferDirection.unknown)  TransferDirection direction, @JsonKey(unknownEnumValue: TransferStatus.unknown)  TransferStatus status,  String fileName,  int totalBytes,  int transferredBytes,  int createdAt,  int updatedAt,  String? errorCode,  String? savedPath)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _Transfer() when $default != null:
return $default(_that.id,_that.deviceId,_that.deviceName,_that.direction,_that.status,_that.fileName,_that.totalBytes,_that.transferredBytes,_that.createdAt,_that.updatedAt,_that.errorCode,_that.savedPath);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( String id,  String deviceId,  String deviceName, @JsonKey(unknownEnumValue: TransferDirection.unknown)  TransferDirection direction, @JsonKey(unknownEnumValue: TransferStatus.unknown)  TransferStatus status,  String fileName,  int totalBytes,  int transferredBytes,  int createdAt,  int updatedAt,  String? errorCode,  String? savedPath)  $default,) {final _that = this;
switch (_that) {
case _Transfer():
return $default(_that.id,_that.deviceId,_that.deviceName,_that.direction,_that.status,_that.fileName,_that.totalBytes,_that.transferredBytes,_that.createdAt,_that.updatedAt,_that.errorCode,_that.savedPath);case _:
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( String id,  String deviceId,  String deviceName, @JsonKey(unknownEnumValue: TransferDirection.unknown)  TransferDirection direction, @JsonKey(unknownEnumValue: TransferStatus.unknown)  TransferStatus status,  String fileName,  int totalBytes,  int transferredBytes,  int createdAt,  int updatedAt,  String? errorCode,  String? savedPath)?  $default,) {final _that = this;
switch (_that) {
case _Transfer() when $default != null:
return $default(_that.id,_that.deviceId,_that.deviceName,_that.direction,_that.status,_that.fileName,_that.totalBytes,_that.transferredBytes,_that.createdAt,_that.updatedAt,_that.errorCode,_that.savedPath);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _Transfer extends Transfer {
  const _Transfer({required this.id, required this.deviceId, required this.deviceName, @JsonKey(unknownEnumValue: TransferDirection.unknown) required this.direction, @JsonKey(unknownEnumValue: TransferStatus.unknown) required this.status, required this.fileName, required this.totalBytes, required this.transferredBytes, required this.createdAt, required this.updatedAt, this.errorCode, this.savedPath}): super._();
  factory _Transfer.fromJson(Map<String, dynamic> json) => _$TransferFromJson(json);

@override final  String id;
@override final  String deviceId;
@override final  String deviceName;
@override@JsonKey(unknownEnumValue: TransferDirection.unknown) final  TransferDirection direction;
@override@JsonKey(unknownEnumValue: TransferStatus.unknown) final  TransferStatus status;
@override final  String fileName;
@override final  int totalBytes;
@override final  int transferredBytes;
@override final  int createdAt;
@override final  int updatedAt;
@override final  String? errorCode;
/// Where a completed incoming file was saved.
@override final  String? savedPath;

/// Create a copy of Transfer
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$TransferCopyWith<_Transfer> get copyWith => __$TransferCopyWithImpl<_Transfer>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$TransferToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _Transfer&&(identical(other.id, id) || other.id == id)&&(identical(other.deviceId, deviceId) || other.deviceId == deviceId)&&(identical(other.deviceName, deviceName) || other.deviceName == deviceName)&&(identical(other.direction, direction) || other.direction == direction)&&(identical(other.status, status) || other.status == status)&&(identical(other.fileName, fileName) || other.fileName == fileName)&&(identical(other.totalBytes, totalBytes) || other.totalBytes == totalBytes)&&(identical(other.transferredBytes, transferredBytes) || other.transferredBytes == transferredBytes)&&(identical(other.createdAt, createdAt) || other.createdAt == createdAt)&&(identical(other.updatedAt, updatedAt) || other.updatedAt == updatedAt)&&(identical(other.errorCode, errorCode) || other.errorCode == errorCode)&&(identical(other.savedPath, savedPath) || other.savedPath == savedPath));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,id,deviceId,deviceName,direction,status,fileName,totalBytes,transferredBytes,createdAt,updatedAt,errorCode,savedPath);
}

@override
String toString() {
    return 'Transfer(id: $id, deviceId: $deviceId, deviceName: $deviceName, direction: $direction, status: $status, fileName: $fileName, totalBytes: $totalBytes, transferredBytes: $transferredBytes, createdAt: $createdAt, updatedAt: $updatedAt, errorCode: $errorCode, savedPath: $savedPath)';
}


}

/// @nodoc
abstract mixin class _$TransferCopyWith<$Res> implements $TransferCopyWith<$Res> {
  factory _$TransferCopyWith(_Transfer value, $Res Function(_Transfer) _then) = __$TransferCopyWithImpl;
@override @useResult
$Res call({
 String id, String deviceId, String deviceName,@JsonKey(unknownEnumValue: TransferDirection.unknown) TransferDirection direction,@JsonKey(unknownEnumValue: TransferStatus.unknown) TransferStatus status, String fileName, int totalBytes, int transferredBytes, int createdAt, int updatedAt, String? errorCode, String? savedPath
});




}
/// @nodoc
class __$TransferCopyWithImpl<$Res>
    implements _$TransferCopyWith<$Res> {
  __$TransferCopyWithImpl(this._self, this._then);

  final _Transfer _self;
  final $Res Function(_Transfer) _then;

/// Create a copy of Transfer
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? id = null,Object? deviceId = null,Object? deviceName = null,Object? direction = null,Object? status = null,Object? fileName = null,Object? totalBytes = null,Object? transferredBytes = null,Object? createdAt = null,Object? updatedAt = null,Object? errorCode = freezed,Object? savedPath = freezed,}) {
  return _then(_Transfer(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,direction: null == direction ? _self.direction : direction // ignore: cast_nullable_to_non_nullable
as TransferDirection,status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as TransferStatus,fileName: null == fileName ? _self.fileName : fileName // ignore: cast_nullable_to_non_nullable
as String,totalBytes: null == totalBytes ? _self.totalBytes : totalBytes // ignore: cast_nullable_to_non_nullable
as int,transferredBytes: null == transferredBytes ? _self.transferredBytes : transferredBytes // ignore: cast_nullable_to_non_nullable
as int,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as int,updatedAt: null == updatedAt ? _self.updatedAt : updatedAt // ignore: cast_nullable_to_non_nullable
as int,errorCode: freezed == errorCode ? _self.errorCode : errorCode // ignore: cast_nullable_to_non_nullable
as String?,savedPath: freezed == savedPath ? _self.savedPath : savedPath // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

// dart format on
