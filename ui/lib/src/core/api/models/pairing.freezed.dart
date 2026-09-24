// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'pairing.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$Pairing {

 String get id; String get deviceId; String get deviceName;@JsonKey(unknownEnumValue: PairingDirection.unknown) PairingDirection get direction;@JsonKey(unknownEnumValue: PairingStatus.unknown) PairingStatus get status; int get createdAt; int get expiresAt; String? get verificationCode; String? get errorCode;
/// Create a copy of Pairing
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PairingCopyWith<Pairing> get copyWith => _$PairingCopyWithImpl<Pairing>(this as Pairing, _$identity);

  /// Serializes this Pairing to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as Pairing;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Pairing&&(identical(other.id, _this.id) || other.id == _this.id)&&(identical(other.deviceId, _this.deviceId) || other.deviceId == _this.deviceId)&&(identical(other.deviceName, _this.deviceName) || other.deviceName == _this.deviceName)&&(identical(other.direction, _this.direction) || other.direction == _this.direction)&&(identical(other.status, _this.status) || other.status == _this.status)&&(identical(other.createdAt, _this.createdAt) || other.createdAt == _this.createdAt)&&(identical(other.expiresAt, _this.expiresAt) || other.expiresAt == _this.expiresAt)&&(identical(other.verificationCode, _this.verificationCode) || other.verificationCode == _this.verificationCode)&&(identical(other.errorCode, _this.errorCode) || other.errorCode == _this.errorCode));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as Pairing;
  return Object.hash(runtimeType,_this.id,_this.deviceId,_this.deviceName,_this.direction,_this.status,_this.createdAt,_this.expiresAt,_this.verificationCode,_this.errorCode);
}

@override
String toString() {
  final _this = this as Pairing;
  return 'Pairing(id: ${_this.id}, deviceId: ${_this.deviceId}, deviceName: ${_this.deviceName}, direction: ${_this.direction}, status: ${_this.status}, createdAt: ${_this.createdAt}, expiresAt: ${_this.expiresAt}, verificationCode: ${_this.verificationCode}, errorCode: ${_this.errorCode})';
}


}

/// @nodoc
abstract mixin class $PairingCopyWith<$Res>  {
  factory $PairingCopyWith(Pairing value, $Res Function(Pairing) _then) = _$PairingCopyWithImpl;
@useResult
$Res call({
 String id, String deviceId, String deviceName,@JsonKey(unknownEnumValue: PairingDirection.unknown) PairingDirection direction,@JsonKey(unknownEnumValue: PairingStatus.unknown) PairingStatus status, int createdAt, int expiresAt, String? verificationCode, String? errorCode
});




}
/// @nodoc
class _$PairingCopyWithImpl<$Res>
    implements $PairingCopyWith<$Res> {
  _$PairingCopyWithImpl(this._self, this._then);

  final Pairing _self;
  final $Res Function(Pairing) _then;

/// Create a copy of Pairing
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? id = null,Object? deviceId = null,Object? deviceName = null,Object? direction = null,Object? status = null,Object? createdAt = null,Object? expiresAt = null,Object? verificationCode = freezed,Object? errorCode = freezed,}) {
  return _then(Pairing(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,direction: null == direction ? _self.direction : direction // ignore: cast_nullable_to_non_nullable
as PairingDirection,status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as PairingStatus,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as int,expiresAt: null == expiresAt ? _self.expiresAt : expiresAt // ignore: cast_nullable_to_non_nullable
as int,verificationCode: freezed == verificationCode ? _self.verificationCode : verificationCode // ignore: cast_nullable_to_non_nullable
as String?,errorCode: freezed == errorCode ? _self.errorCode : errorCode // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}

}


/// Adds pattern-matching-related methods to [Pairing].
extension PairingPatterns on Pairing {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _Pairing value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _Pairing() when $default != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _Pairing value)  $default,){
final _that = this;
switch (_that) {
case _Pairing():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _Pairing value)?  $default,){
final _that = this;
switch (_that) {
case _Pairing() when $default != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( String id,  String deviceId,  String deviceName, @JsonKey(unknownEnumValue: PairingDirection.unknown)  PairingDirection direction, @JsonKey(unknownEnumValue: PairingStatus.unknown)  PairingStatus status,  int createdAt,  int expiresAt,  String? verificationCode,  String? errorCode)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _Pairing() when $default != null:
return $default(_that.id,_that.deviceId,_that.deviceName,_that.direction,_that.status,_that.createdAt,_that.expiresAt,_that.verificationCode,_that.errorCode);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( String id,  String deviceId,  String deviceName, @JsonKey(unknownEnumValue: PairingDirection.unknown)  PairingDirection direction, @JsonKey(unknownEnumValue: PairingStatus.unknown)  PairingStatus status,  int createdAt,  int expiresAt,  String? verificationCode,  String? errorCode)  $default,) {final _that = this;
switch (_that) {
case _Pairing():
return $default(_that.id,_that.deviceId,_that.deviceName,_that.direction,_that.status,_that.createdAt,_that.expiresAt,_that.verificationCode,_that.errorCode);case _:
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( String id,  String deviceId,  String deviceName, @JsonKey(unknownEnumValue: PairingDirection.unknown)  PairingDirection direction, @JsonKey(unknownEnumValue: PairingStatus.unknown)  PairingStatus status,  int createdAt,  int expiresAt,  String? verificationCode,  String? errorCode)?  $default,) {final _that = this;
switch (_that) {
case _Pairing() when $default != null:
return $default(_that.id,_that.deviceId,_that.deviceName,_that.direction,_that.status,_that.createdAt,_that.expiresAt,_that.verificationCode,_that.errorCode);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _Pairing extends Pairing {
  const _Pairing({required this.id, required this.deviceId, required this.deviceName, @JsonKey(unknownEnumValue: PairingDirection.unknown) required this.direction, @JsonKey(unknownEnumValue: PairingStatus.unknown) required this.status, required this.createdAt, required this.expiresAt, this.verificationCode, this.errorCode}): super._();
  factory _Pairing.fromJson(Map<String, dynamic> json) => _$PairingFromJson(json);

@override final  String id;
@override final  String deviceId;
@override final  String deviceName;
@override@JsonKey(unknownEnumValue: PairingDirection.unknown) final  PairingDirection direction;
@override@JsonKey(unknownEnumValue: PairingStatus.unknown) final  PairingStatus status;
@override final  int createdAt;
@override final  int expiresAt;
@override final  String? verificationCode;
@override final  String? errorCode;

/// Create a copy of Pairing
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$PairingCopyWith<_Pairing> get copyWith => __$PairingCopyWithImpl<_Pairing>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$PairingToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _Pairing&&(identical(other.id, id) || other.id == id)&&(identical(other.deviceId, deviceId) || other.deviceId == deviceId)&&(identical(other.deviceName, deviceName) || other.deviceName == deviceName)&&(identical(other.direction, direction) || other.direction == direction)&&(identical(other.status, status) || other.status == status)&&(identical(other.createdAt, createdAt) || other.createdAt == createdAt)&&(identical(other.expiresAt, expiresAt) || other.expiresAt == expiresAt)&&(identical(other.verificationCode, verificationCode) || other.verificationCode == verificationCode)&&(identical(other.errorCode, errorCode) || other.errorCode == errorCode));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,id,deviceId,deviceName,direction,status,createdAt,expiresAt,verificationCode,errorCode);
}

@override
String toString() {
    return 'Pairing(id: $id, deviceId: $deviceId, deviceName: $deviceName, direction: $direction, status: $status, createdAt: $createdAt, expiresAt: $expiresAt, verificationCode: $verificationCode, errorCode: $errorCode)';
}


}

/// @nodoc
abstract mixin class _$PairingCopyWith<$Res> implements $PairingCopyWith<$Res> {
  factory _$PairingCopyWith(_Pairing value, $Res Function(_Pairing) _then) = __$PairingCopyWithImpl;
@override @useResult
$Res call({
 String id, String deviceId, String deviceName,@JsonKey(unknownEnumValue: PairingDirection.unknown) PairingDirection direction,@JsonKey(unknownEnumValue: PairingStatus.unknown) PairingStatus status, int createdAt, int expiresAt, String? verificationCode, String? errorCode
});




}
/// @nodoc
class __$PairingCopyWithImpl<$Res>
    implements _$PairingCopyWith<$Res> {
  __$PairingCopyWithImpl(this._self, this._then);

  final _Pairing _self;
  final $Res Function(_Pairing) _then;

/// Create a copy of Pairing
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? id = null,Object? deviceId = null,Object? deviceName = null,Object? direction = null,Object? status = null,Object? createdAt = null,Object? expiresAt = null,Object? verificationCode = freezed,Object? errorCode = freezed,}) {
  return _then(_Pairing(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,direction: null == direction ? _self.direction : direction // ignore: cast_nullable_to_non_nullable
as PairingDirection,status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as PairingStatus,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as int,expiresAt: null == expiresAt ? _self.expiresAt : expiresAt // ignore: cast_nullable_to_non_nullable
as int,verificationCode: freezed == verificationCode ? _self.verificationCode : verificationCode // ignore: cast_nullable_to_non_nullable
as String?,errorCode: freezed == errorCode ? _self.errorCode : errorCode // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

// dart format on
