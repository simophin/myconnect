// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'status.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$LocalDevice {

 String get deviceId; String get deviceName;
/// Create a copy of LocalDevice
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$LocalDeviceCopyWith<LocalDevice> get copyWith => _$LocalDeviceCopyWithImpl<LocalDevice>(this as LocalDevice, _$identity);

  /// Serializes this LocalDevice to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as LocalDevice;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is LocalDevice&&(identical(other.deviceId, _this.deviceId) || other.deviceId == _this.deviceId)&&(identical(other.deviceName, _this.deviceName) || other.deviceName == _this.deviceName));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as LocalDevice;
  return Object.hash(runtimeType,_this.deviceId,_this.deviceName);
}

@override
String toString() {
  final _this = this as LocalDevice;
  return 'LocalDevice(deviceId: ${_this.deviceId}, deviceName: ${_this.deviceName})';
}


}

/// @nodoc
abstract mixin class $LocalDeviceCopyWith<$Res>  {
  factory $LocalDeviceCopyWith(LocalDevice value, $Res Function(LocalDevice) _then) = _$LocalDeviceCopyWithImpl;
@useResult
$Res call({
 String deviceId, String deviceName
});




}
/// @nodoc
class _$LocalDeviceCopyWithImpl<$Res>
    implements $LocalDeviceCopyWith<$Res> {
  _$LocalDeviceCopyWithImpl(this._self, this._then);

  final LocalDevice _self;
  final $Res Function(LocalDevice) _then;

/// Create a copy of LocalDevice
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? deviceId = null,Object? deviceName = null,}) {
  return _then(LocalDevice(
deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,
  ));
}

}


/// Adds pattern-matching-related methods to [LocalDevice].
extension LocalDevicePatterns on LocalDevice {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _LocalDevice value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _LocalDevice() when $default != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _LocalDevice value)  $default,){
final _that = this;
switch (_that) {
case _LocalDevice():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _LocalDevice value)?  $default,){
final _that = this;
switch (_that) {
case _LocalDevice() when $default != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( String deviceId,  String deviceName)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _LocalDevice() when $default != null:
return $default(_that.deviceId,_that.deviceName);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( String deviceId,  String deviceName)  $default,) {final _that = this;
switch (_that) {
case _LocalDevice():
return $default(_that.deviceId,_that.deviceName);case _:
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( String deviceId,  String deviceName)?  $default,) {final _that = this;
switch (_that) {
case _LocalDevice() when $default != null:
return $default(_that.deviceId,_that.deviceName);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _LocalDevice implements LocalDevice {
  const _LocalDevice({required this.deviceId, required this.deviceName});
  factory _LocalDevice.fromJson(Map<String, dynamic> json) => _$LocalDeviceFromJson(json);

@override final  String deviceId;
@override final  String deviceName;

/// Create a copy of LocalDevice
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$LocalDeviceCopyWith<_LocalDevice> get copyWith => __$LocalDeviceCopyWithImpl<_LocalDevice>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$LocalDeviceToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _LocalDevice&&(identical(other.deviceId, deviceId) || other.deviceId == deviceId)&&(identical(other.deviceName, deviceName) || other.deviceName == deviceName));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,deviceId,deviceName);
}

@override
String toString() {
    return 'LocalDevice(deviceId: $deviceId, deviceName: $deviceName)';
}


}

/// @nodoc
abstract mixin class _$LocalDeviceCopyWith<$Res> implements $LocalDeviceCopyWith<$Res> {
  factory _$LocalDeviceCopyWith(_LocalDevice value, $Res Function(_LocalDevice) _then) = __$LocalDeviceCopyWithImpl;
@override @useResult
$Res call({
 String deviceId, String deviceName
});




}
/// @nodoc
class __$LocalDeviceCopyWithImpl<$Res>
    implements _$LocalDeviceCopyWith<$Res> {
  __$LocalDeviceCopyWithImpl(this._self, this._then);

  final _LocalDevice _self;
  final $Res Function(_LocalDevice) _then;

/// Create a copy of LocalDevice
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? deviceId = null,Object? deviceName = null,}) {
  return _then(_LocalDevice(
deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}


/// @nodoc
mixin _$DaemonStatus {

 String get version; int get uptimeSeconds; LocalDevice get localDevice; int get protocolVersion;
/// Create a copy of DaemonStatus
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$DaemonStatusCopyWith<DaemonStatus> get copyWith => _$DaemonStatusCopyWithImpl<DaemonStatus>(this as DaemonStatus, _$identity);

  /// Serializes this DaemonStatus to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as DaemonStatus;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is DaemonStatus&&(identical(other.version, _this.version) || other.version == _this.version)&&(identical(other.uptimeSeconds, _this.uptimeSeconds) || other.uptimeSeconds == _this.uptimeSeconds)&&(identical(other.localDevice, _this.localDevice) || other.localDevice == _this.localDevice)&&(identical(other.protocolVersion, _this.protocolVersion) || other.protocolVersion == _this.protocolVersion));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as DaemonStatus;
  return Object.hash(runtimeType,_this.version,_this.uptimeSeconds,_this.localDevice,_this.protocolVersion);
}

@override
String toString() {
  final _this = this as DaemonStatus;
  return 'DaemonStatus(version: ${_this.version}, uptimeSeconds: ${_this.uptimeSeconds}, localDevice: ${_this.localDevice}, protocolVersion: ${_this.protocolVersion})';
}


}

/// @nodoc
abstract mixin class $DaemonStatusCopyWith<$Res>  {
  factory $DaemonStatusCopyWith(DaemonStatus value, $Res Function(DaemonStatus) _then) = _$DaemonStatusCopyWithImpl;
@useResult
$Res call({
 String version, int uptimeSeconds, LocalDevice localDevice, int protocolVersion
});


$LocalDeviceCopyWith<$Res> get localDevice;

}
/// @nodoc
class _$DaemonStatusCopyWithImpl<$Res>
    implements $DaemonStatusCopyWith<$Res> {
  _$DaemonStatusCopyWithImpl(this._self, this._then);

  final DaemonStatus _self;
  final $Res Function(DaemonStatus) _then;

/// Create a copy of DaemonStatus
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? version = null,Object? uptimeSeconds = null,Object? localDevice = null,Object? protocolVersion = null,}) {
  return _then(DaemonStatus(
version: null == version ? _self.version : version // ignore: cast_nullable_to_non_nullable
as String,uptimeSeconds: null == uptimeSeconds ? _self.uptimeSeconds : uptimeSeconds // ignore: cast_nullable_to_non_nullable
as int,localDevice: null == localDevice ? _self.localDevice : localDevice // ignore: cast_nullable_to_non_nullable
as LocalDevice,protocolVersion: null == protocolVersion ? _self.protocolVersion : protocolVersion // ignore: cast_nullable_to_non_nullable
as int,
  ));
}
/// Create a copy of DaemonStatus
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$LocalDeviceCopyWith<$Res> get localDevice {
  
  return $LocalDeviceCopyWith<$Res>(_self.localDevice, (value) {
    return _then(_self.copyWith(localDevice: value));
  });
}
}


/// Adds pattern-matching-related methods to [DaemonStatus].
extension DaemonStatusPatterns on DaemonStatus {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _DaemonStatus value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _DaemonStatus() when $default != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _DaemonStatus value)  $default,){
final _that = this;
switch (_that) {
case _DaemonStatus():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _DaemonStatus value)?  $default,){
final _that = this;
switch (_that) {
case _DaemonStatus() when $default != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( String version,  int uptimeSeconds,  LocalDevice localDevice,  int protocolVersion)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _DaemonStatus() when $default != null:
return $default(_that.version,_that.uptimeSeconds,_that.localDevice,_that.protocolVersion);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( String version,  int uptimeSeconds,  LocalDevice localDevice,  int protocolVersion)  $default,) {final _that = this;
switch (_that) {
case _DaemonStatus():
return $default(_that.version,_that.uptimeSeconds,_that.localDevice,_that.protocolVersion);case _:
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( String version,  int uptimeSeconds,  LocalDevice localDevice,  int protocolVersion)?  $default,) {final _that = this;
switch (_that) {
case _DaemonStatus() when $default != null:
return $default(_that.version,_that.uptimeSeconds,_that.localDevice,_that.protocolVersion);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _DaemonStatus implements DaemonStatus {
  const _DaemonStatus({required this.version, required this.uptimeSeconds, required this.localDevice, required this.protocolVersion});
  factory _DaemonStatus.fromJson(Map<String, dynamic> json) => _$DaemonStatusFromJson(json);

@override final  String version;
@override final  int uptimeSeconds;
@override final  LocalDevice localDevice;
@override final  int protocolVersion;

/// Create a copy of DaemonStatus
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$DaemonStatusCopyWith<_DaemonStatus> get copyWith => __$DaemonStatusCopyWithImpl<_DaemonStatus>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$DaemonStatusToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _DaemonStatus&&(identical(other.version, version) || other.version == version)&&(identical(other.uptimeSeconds, uptimeSeconds) || other.uptimeSeconds == uptimeSeconds)&&(identical(other.localDevice, localDevice) || other.localDevice == localDevice)&&(identical(other.protocolVersion, protocolVersion) || other.protocolVersion == protocolVersion));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,version,uptimeSeconds,localDevice,protocolVersion);
}

@override
String toString() {
    return 'DaemonStatus(version: $version, uptimeSeconds: $uptimeSeconds, localDevice: $localDevice, protocolVersion: $protocolVersion)';
}


}

/// @nodoc
abstract mixin class _$DaemonStatusCopyWith<$Res> implements $DaemonStatusCopyWith<$Res> {
  factory _$DaemonStatusCopyWith(_DaemonStatus value, $Res Function(_DaemonStatus) _then) = __$DaemonStatusCopyWithImpl;
@override @useResult
$Res call({
 String version, int uptimeSeconds, LocalDevice localDevice, int protocolVersion
});


@override $LocalDeviceCopyWith<$Res> get localDevice;

}
/// @nodoc
class __$DaemonStatusCopyWithImpl<$Res>
    implements _$DaemonStatusCopyWith<$Res> {
  __$DaemonStatusCopyWithImpl(this._self, this._then);

  final _DaemonStatus _self;
  final $Res Function(_DaemonStatus) _then;

/// Create a copy of DaemonStatus
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? version = null,Object? uptimeSeconds = null,Object? localDevice = null,Object? protocolVersion = null,}) {
  return _then(_DaemonStatus(
version: null == version ? _self.version : version // ignore: cast_nullable_to_non_nullable
as String,uptimeSeconds: null == uptimeSeconds ? _self.uptimeSeconds : uptimeSeconds // ignore: cast_nullable_to_non_nullable
as int,localDevice: null == localDevice ? _self.localDevice : localDevice // ignore: cast_nullable_to_non_nullable
as LocalDevice,protocolVersion: null == protocolVersion ? _self.protocolVersion : protocolVersion // ignore: cast_nullable_to_non_nullable
as int,
  ));
}

/// Create a copy of DaemonStatus
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$LocalDeviceCopyWith<$Res> get localDevice {
  
  return $LocalDeviceCopyWith<$Res>(_self.localDevice, (value) {
    return _then(_self.copyWith(localDevice: value));
  });
}
}

// dart format on
