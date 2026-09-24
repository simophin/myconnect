// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'device.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$BatteryStatus {

 int get charge; bool get charging;
/// Create a copy of BatteryStatus
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$BatteryStatusCopyWith<BatteryStatus> get copyWith => _$BatteryStatusCopyWithImpl<BatteryStatus>(this as BatteryStatus, _$identity);

  /// Serializes this BatteryStatus to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as BatteryStatus;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is BatteryStatus&&(identical(other.charge, _this.charge) || other.charge == _this.charge)&&(identical(other.charging, _this.charging) || other.charging == _this.charging));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as BatteryStatus;
  return Object.hash(runtimeType,_this.charge,_this.charging);
}

@override
String toString() {
  final _this = this as BatteryStatus;
  return 'BatteryStatus(charge: ${_this.charge}, charging: ${_this.charging})';
}


}

/// @nodoc
abstract mixin class $BatteryStatusCopyWith<$Res>  {
  factory $BatteryStatusCopyWith(BatteryStatus value, $Res Function(BatteryStatus) _then) = _$BatteryStatusCopyWithImpl;
@useResult
$Res call({
 int charge, bool charging
});




}
/// @nodoc
class _$BatteryStatusCopyWithImpl<$Res>
    implements $BatteryStatusCopyWith<$Res> {
  _$BatteryStatusCopyWithImpl(this._self, this._then);

  final BatteryStatus _self;
  final $Res Function(BatteryStatus) _then;

/// Create a copy of BatteryStatus
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? charge = null,Object? charging = null,}) {
  return _then(BatteryStatus(
charge: null == charge ? _self.charge : charge // ignore: cast_nullable_to_non_nullable
as int,charging: null == charging ? _self.charging : charging // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}

}


/// Adds pattern-matching-related methods to [BatteryStatus].
extension BatteryStatusPatterns on BatteryStatus {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _BatteryStatus value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _BatteryStatus() when $default != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _BatteryStatus value)  $default,){
final _that = this;
switch (_that) {
case _BatteryStatus():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _BatteryStatus value)?  $default,){
final _that = this;
switch (_that) {
case _BatteryStatus() when $default != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( int charge,  bool charging)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _BatteryStatus() when $default != null:
return $default(_that.charge,_that.charging);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( int charge,  bool charging)  $default,) {final _that = this;
switch (_that) {
case _BatteryStatus():
return $default(_that.charge,_that.charging);case _:
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( int charge,  bool charging)?  $default,) {final _that = this;
switch (_that) {
case _BatteryStatus() when $default != null:
return $default(_that.charge,_that.charging);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _BatteryStatus implements BatteryStatus {
  const _BatteryStatus({required this.charge, required this.charging});
  factory _BatteryStatus.fromJson(Map<String, dynamic> json) => _$BatteryStatusFromJson(json);

@override final  int charge;
@override final  bool charging;

/// Create a copy of BatteryStatus
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$BatteryStatusCopyWith<_BatteryStatus> get copyWith => __$BatteryStatusCopyWithImpl<_BatteryStatus>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$BatteryStatusToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _BatteryStatus&&(identical(other.charge, charge) || other.charge == charge)&&(identical(other.charging, charging) || other.charging == charging));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,charge,charging);
}

@override
String toString() {
    return 'BatteryStatus(charge: $charge, charging: $charging)';
}


}

/// @nodoc
abstract mixin class _$BatteryStatusCopyWith<$Res> implements $BatteryStatusCopyWith<$Res> {
  factory _$BatteryStatusCopyWith(_BatteryStatus value, $Res Function(_BatteryStatus) _then) = __$BatteryStatusCopyWithImpl;
@override @useResult
$Res call({
 int charge, bool charging
});




}
/// @nodoc
class __$BatteryStatusCopyWithImpl<$Res>
    implements _$BatteryStatusCopyWith<$Res> {
  __$BatteryStatusCopyWithImpl(this._self, this._then);

  final _BatteryStatus _self;
  final $Res Function(_BatteryStatus) _then;

/// Create a copy of BatteryStatus
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? charge = null,Object? charging = null,}) {
  return _then(_BatteryStatus(
charge: null == charge ? _self.charge : charge // ignore: cast_nullable_to_non_nullable
as int,charging: null == charging ? _self.charging : charging // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}


/// @nodoc
mixin _$Device {

 String get deviceId; String get deviceName;@JsonKey(unknownEnumValue: DeviceType.unknown) DeviceType get deviceType; int get protocolVersion; List<String> get incomingCapabilities; List<String> get outgoingCapabilities;@JsonKey(unknownEnumValue: DeviceReachability.unknown) DeviceReachability get reachability; bool get paired; bool get pairing; int get lastSeenAt;/// Known only while the device is paired and connected, once it has
/// reported it.
 BatteryStatus? get battery;
/// Create a copy of Device
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$DeviceCopyWith<Device> get copyWith => _$DeviceCopyWithImpl<Device>(this as Device, _$identity);

  /// Serializes this Device to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as Device;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is Device&&(identical(other.deviceId, _this.deviceId) || other.deviceId == _this.deviceId)&&(identical(other.deviceName, _this.deviceName) || other.deviceName == _this.deviceName)&&(identical(other.deviceType, _this.deviceType) || other.deviceType == _this.deviceType)&&(identical(other.protocolVersion, _this.protocolVersion) || other.protocolVersion == _this.protocolVersion)&&const DeepCollectionEquality().equals(other.incomingCapabilities, _this.incomingCapabilities)&&const DeepCollectionEquality().equals(other.outgoingCapabilities, _this.outgoingCapabilities)&&(identical(other.reachability, _this.reachability) || other.reachability == _this.reachability)&&(identical(other.paired, _this.paired) || other.paired == _this.paired)&&(identical(other.pairing, _this.pairing) || other.pairing == _this.pairing)&&(identical(other.lastSeenAt, _this.lastSeenAt) || other.lastSeenAt == _this.lastSeenAt)&&(identical(other.battery, _this.battery) || other.battery == _this.battery));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as Device;
  return Object.hash(runtimeType,_this.deviceId,_this.deviceName,_this.deviceType,_this.protocolVersion,const DeepCollectionEquality().hash(_this.incomingCapabilities),const DeepCollectionEquality().hash(_this.outgoingCapabilities),_this.reachability,_this.paired,_this.pairing,_this.lastSeenAt,_this.battery);
}

@override
String toString() {
  final _this = this as Device;
  return 'Device(deviceId: ${_this.deviceId}, deviceName: ${_this.deviceName}, deviceType: ${_this.deviceType}, protocolVersion: ${_this.protocolVersion}, incomingCapabilities: ${_this.incomingCapabilities}, outgoingCapabilities: ${_this.outgoingCapabilities}, reachability: ${_this.reachability}, paired: ${_this.paired}, pairing: ${_this.pairing}, lastSeenAt: ${_this.lastSeenAt}, battery: ${_this.battery})';
}


}

/// @nodoc
abstract mixin class $DeviceCopyWith<$Res>  {
  factory $DeviceCopyWith(Device value, $Res Function(Device) _then) = _$DeviceCopyWithImpl;
@useResult
$Res call({
 String deviceId, String deviceName,@JsonKey(unknownEnumValue: DeviceType.unknown) DeviceType deviceType, int protocolVersion, List<String> incomingCapabilities, List<String> outgoingCapabilities,@JsonKey(unknownEnumValue: DeviceReachability.unknown) DeviceReachability reachability, bool paired, bool pairing, int lastSeenAt, BatteryStatus? battery
});


$BatteryStatusCopyWith<$Res>? get battery;

}
/// @nodoc
class _$DeviceCopyWithImpl<$Res>
    implements $DeviceCopyWith<$Res> {
  _$DeviceCopyWithImpl(this._self, this._then);

  final Device _self;
  final $Res Function(Device) _then;

/// Create a copy of Device
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? deviceId = null,Object? deviceName = null,Object? deviceType = null,Object? protocolVersion = null,Object? incomingCapabilities = null,Object? outgoingCapabilities = null,Object? reachability = null,Object? paired = null,Object? pairing = null,Object? lastSeenAt = null,Object? battery = freezed,}) {
  return _then(Device(
deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,deviceType: null == deviceType ? _self.deviceType : deviceType // ignore: cast_nullable_to_non_nullable
as DeviceType,protocolVersion: null == protocolVersion ? _self.protocolVersion : protocolVersion // ignore: cast_nullable_to_non_nullable
as int,incomingCapabilities: null == incomingCapabilities ? _self.incomingCapabilities : incomingCapabilities // ignore: cast_nullable_to_non_nullable
as List<String>,outgoingCapabilities: null == outgoingCapabilities ? _self.outgoingCapabilities : outgoingCapabilities // ignore: cast_nullable_to_non_nullable
as List<String>,reachability: null == reachability ? _self.reachability : reachability // ignore: cast_nullable_to_non_nullable
as DeviceReachability,paired: null == paired ? _self.paired : paired // ignore: cast_nullable_to_non_nullable
as bool,pairing: null == pairing ? _self.pairing : pairing // ignore: cast_nullable_to_non_nullable
as bool,lastSeenAt: null == lastSeenAt ? _self.lastSeenAt : lastSeenAt // ignore: cast_nullable_to_non_nullable
as int,battery: freezed == battery ? _self.battery : battery // ignore: cast_nullable_to_non_nullable
as BatteryStatus?,
  ));
}
/// Create a copy of Device
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$BatteryStatusCopyWith<$Res>? get battery {
    if (_self.battery == null) {
    return null;
  }

  return $BatteryStatusCopyWith<$Res>(_self.battery!, (value) {
    return _then(_self.copyWith(battery: value));
  });
}
}


/// Adds pattern-matching-related methods to [Device].
extension DevicePatterns on Device {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _Device value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _Device() when $default != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _Device value)  $default,){
final _that = this;
switch (_that) {
case _Device():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _Device value)?  $default,){
final _that = this;
switch (_that) {
case _Device() when $default != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( String deviceId,  String deviceName, @JsonKey(unknownEnumValue: DeviceType.unknown)  DeviceType deviceType,  int protocolVersion,  List<String> incomingCapabilities,  List<String> outgoingCapabilities, @JsonKey(unknownEnumValue: DeviceReachability.unknown)  DeviceReachability reachability,  bool paired,  bool pairing,  int lastSeenAt,  BatteryStatus? battery)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _Device() when $default != null:
return $default(_that.deviceId,_that.deviceName,_that.deviceType,_that.protocolVersion,_that.incomingCapabilities,_that.outgoingCapabilities,_that.reachability,_that.paired,_that.pairing,_that.lastSeenAt,_that.battery);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( String deviceId,  String deviceName, @JsonKey(unknownEnumValue: DeviceType.unknown)  DeviceType deviceType,  int protocolVersion,  List<String> incomingCapabilities,  List<String> outgoingCapabilities, @JsonKey(unknownEnumValue: DeviceReachability.unknown)  DeviceReachability reachability,  bool paired,  bool pairing,  int lastSeenAt,  BatteryStatus? battery)  $default,) {final _that = this;
switch (_that) {
case _Device():
return $default(_that.deviceId,_that.deviceName,_that.deviceType,_that.protocolVersion,_that.incomingCapabilities,_that.outgoingCapabilities,_that.reachability,_that.paired,_that.pairing,_that.lastSeenAt,_that.battery);case _:
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( String deviceId,  String deviceName, @JsonKey(unknownEnumValue: DeviceType.unknown)  DeviceType deviceType,  int protocolVersion,  List<String> incomingCapabilities,  List<String> outgoingCapabilities, @JsonKey(unknownEnumValue: DeviceReachability.unknown)  DeviceReachability reachability,  bool paired,  bool pairing,  int lastSeenAt,  BatteryStatus? battery)?  $default,) {final _that = this;
switch (_that) {
case _Device() when $default != null:
return $default(_that.deviceId,_that.deviceName,_that.deviceType,_that.protocolVersion,_that.incomingCapabilities,_that.outgoingCapabilities,_that.reachability,_that.paired,_that.pairing,_that.lastSeenAt,_that.battery);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _Device extends Device {
  const _Device({required this.deviceId, required this.deviceName, @JsonKey(unknownEnumValue: DeviceType.unknown) required this.deviceType, required this.protocolVersion, required  List<String> incomingCapabilities, required  List<String> outgoingCapabilities, @JsonKey(unknownEnumValue: DeviceReachability.unknown) required this.reachability, required this.paired, required this.pairing, required this.lastSeenAt, this.battery}): _incomingCapabilities = incomingCapabilities,_outgoingCapabilities = outgoingCapabilities,super._();
  factory _Device.fromJson(Map<String, dynamic> json) => _$DeviceFromJson(json);

@override final  String deviceId;
@override final  String deviceName;
@override@JsonKey(unknownEnumValue: DeviceType.unknown) final  DeviceType deviceType;
@override final  int protocolVersion;
 final  List<String> _incomingCapabilities;
@override List<String> get incomingCapabilities {
  if (_incomingCapabilities is EqualUnmodifiableListView) return _incomingCapabilities;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_incomingCapabilities);
}

 final  List<String> _outgoingCapabilities;
@override List<String> get outgoingCapabilities {
  if (_outgoingCapabilities is EqualUnmodifiableListView) return _outgoingCapabilities;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_outgoingCapabilities);
}

@override@JsonKey(unknownEnumValue: DeviceReachability.unknown) final  DeviceReachability reachability;
@override final  bool paired;
@override final  bool pairing;
@override final  int lastSeenAt;
/// Known only while the device is paired and connected, once it has
/// reported it.
@override final  BatteryStatus? battery;

/// Create a copy of Device
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$DeviceCopyWith<_Device> get copyWith => __$DeviceCopyWithImpl<_Device>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$DeviceToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _Device&&(identical(other.deviceId, deviceId) || other.deviceId == deviceId)&&(identical(other.deviceName, deviceName) || other.deviceName == deviceName)&&(identical(other.deviceType, deviceType) || other.deviceType == deviceType)&&(identical(other.protocolVersion, protocolVersion) || other.protocolVersion == protocolVersion)&&const DeepCollectionEquality().equals(other.incomingCapabilities, _incomingCapabilities)&&const DeepCollectionEquality().equals(other.outgoingCapabilities, _outgoingCapabilities)&&(identical(other.reachability, reachability) || other.reachability == reachability)&&(identical(other.paired, paired) || other.paired == paired)&&(identical(other.pairing, pairing) || other.pairing == pairing)&&(identical(other.lastSeenAt, lastSeenAt) || other.lastSeenAt == lastSeenAt)&&(identical(other.battery, battery) || other.battery == battery));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,deviceId,deviceName,deviceType,protocolVersion,const DeepCollectionEquality().hash(_incomingCapabilities),const DeepCollectionEquality().hash(_outgoingCapabilities),reachability,paired,pairing,lastSeenAt,battery);
}

@override
String toString() {
    return 'Device(deviceId: $deviceId, deviceName: $deviceName, deviceType: $deviceType, protocolVersion: $protocolVersion, incomingCapabilities: $incomingCapabilities, outgoingCapabilities: $outgoingCapabilities, reachability: $reachability, paired: $paired, pairing: $pairing, lastSeenAt: $lastSeenAt, battery: $battery)';
}


}

/// @nodoc
abstract mixin class _$DeviceCopyWith<$Res> implements $DeviceCopyWith<$Res> {
  factory _$DeviceCopyWith(_Device value, $Res Function(_Device) _then) = __$DeviceCopyWithImpl;
@override @useResult
$Res call({
 String deviceId, String deviceName,@JsonKey(unknownEnumValue: DeviceType.unknown) DeviceType deviceType, int protocolVersion, List<String> incomingCapabilities, List<String> outgoingCapabilities,@JsonKey(unknownEnumValue: DeviceReachability.unknown) DeviceReachability reachability, bool paired, bool pairing, int lastSeenAt, BatteryStatus? battery
});


@override $BatteryStatusCopyWith<$Res>? get battery;

}
/// @nodoc
class __$DeviceCopyWithImpl<$Res>
    implements _$DeviceCopyWith<$Res> {
  __$DeviceCopyWithImpl(this._self, this._then);

  final _Device _self;
  final $Res Function(_Device) _then;

/// Create a copy of Device
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? deviceId = null,Object? deviceName = null,Object? deviceType = null,Object? protocolVersion = null,Object? incomingCapabilities = null,Object? outgoingCapabilities = null,Object? reachability = null,Object? paired = null,Object? pairing = null,Object? lastSeenAt = null,Object? battery = freezed,}) {
  return _then(_Device(
deviceId: null == deviceId ? _self.deviceId : deviceId // ignore: cast_nullable_to_non_nullable
as String,deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,deviceType: null == deviceType ? _self.deviceType : deviceType // ignore: cast_nullable_to_non_nullable
as DeviceType,protocolVersion: null == protocolVersion ? _self.protocolVersion : protocolVersion // ignore: cast_nullable_to_non_nullable
as int,incomingCapabilities: null == incomingCapabilities ? _self._incomingCapabilities : incomingCapabilities // ignore: cast_nullable_to_non_nullable
as List<String>,outgoingCapabilities: null == outgoingCapabilities ? _self._outgoingCapabilities : outgoingCapabilities // ignore: cast_nullable_to_non_nullable
as List<String>,reachability: null == reachability ? _self.reachability : reachability // ignore: cast_nullable_to_non_nullable
as DeviceReachability,paired: null == paired ? _self.paired : paired // ignore: cast_nullable_to_non_nullable
as bool,pairing: null == pairing ? _self.pairing : pairing // ignore: cast_nullable_to_non_nullable
as bool,lastSeenAt: null == lastSeenAt ? _self.lastSeenAt : lastSeenAt // ignore: cast_nullable_to_non_nullable
as int,battery: freezed == battery ? _self.battery : battery // ignore: cast_nullable_to_non_nullable
as BatteryStatus?,
  ));
}

/// Create a copy of Device
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$BatteryStatusCopyWith<$Res>? get battery {
    if (_self.battery == null) {
    return null;
  }

  return $BatteryStatusCopyWith<$Res>(_self.battery!, (value) {
    return _then(_self.copyWith(battery: value));
  });
}
}

// dart format on
