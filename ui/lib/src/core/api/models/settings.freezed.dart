// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'settings.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$DaemonSettings {

 String get deviceName; String get downloadDir;/// Owned by the UI; the daemon only stores it.
 bool get closeToTray;/// The daemon's plugins' settings, keyed by plugin id. Read through
/// getters such as [clipboardSyncEnabled].
 Map<String, Object?> get plugins;
/// Create a copy of DaemonSettings
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$DaemonSettingsCopyWith<DaemonSettings> get copyWith => _$DaemonSettingsCopyWithImpl<DaemonSettings>(this as DaemonSettings, _$identity);

  /// Serializes this DaemonSettings to a JSON map.
  Map<String, dynamic> toJson();


@override
bool operator ==(Object other) {
  final _this = this as DaemonSettings;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is DaemonSettings&&(identical(other.deviceName, _this.deviceName) || other.deviceName == _this.deviceName)&&(identical(other.downloadDir, _this.downloadDir) || other.downloadDir == _this.downloadDir)&&(identical(other.closeToTray, _this.closeToTray) || other.closeToTray == _this.closeToTray)&&const DeepCollectionEquality().equals(other.plugins, _this.plugins));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
  final _this = this as DaemonSettings;
  return Object.hash(runtimeType,_this.deviceName,_this.downloadDir,_this.closeToTray,const DeepCollectionEquality().hash(_this.plugins));
}

@override
String toString() {
  final _this = this as DaemonSettings;
  return 'DaemonSettings(deviceName: ${_this.deviceName}, downloadDir: ${_this.downloadDir}, closeToTray: ${_this.closeToTray}, plugins: ${_this.plugins})';
}


}

/// @nodoc
abstract mixin class $DaemonSettingsCopyWith<$Res>  {
  factory $DaemonSettingsCopyWith(DaemonSettings value, $Res Function(DaemonSettings) _then) = _$DaemonSettingsCopyWithImpl;
@useResult
$Res call({
 String deviceName, String downloadDir, bool closeToTray, Map<String, Object?> plugins
});




}
/// @nodoc
class _$DaemonSettingsCopyWithImpl<$Res>
    implements $DaemonSettingsCopyWith<$Res> {
  _$DaemonSettingsCopyWithImpl(this._self, this._then);

  final DaemonSettings _self;
  final $Res Function(DaemonSettings) _then;

/// Create a copy of DaemonSettings
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? deviceName = null,Object? downloadDir = null,Object? closeToTray = null,Object? plugins = null,}) {
  return _then(DaemonSettings(
deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,downloadDir: null == downloadDir ? _self.downloadDir : downloadDir // ignore: cast_nullable_to_non_nullable
as String,closeToTray: null == closeToTray ? _self.closeToTray : closeToTray // ignore: cast_nullable_to_non_nullable
as bool,plugins: null == plugins ? _self.plugins : plugins // ignore: cast_nullable_to_non_nullable
as Map<String, Object?>,
  ));
}

}


/// Adds pattern-matching-related methods to [DaemonSettings].
extension DaemonSettingsPatterns on DaemonSettings {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>(TResult Function( _DaemonSettings value)?  $default,{required TResult orElse(),}){
final _that = this;
switch (_that) {
case _DaemonSettings() when $default != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>(TResult Function( _DaemonSettings value)  $default,){
final _that = this;
switch (_that) {
case _DaemonSettings():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>(TResult? Function( _DaemonSettings value)?  $default,){
final _that = this;
switch (_that) {
case _DaemonSettings() when $default != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>(TResult Function( String deviceName,  String downloadDir,  bool closeToTray,  Map<String, Object?> plugins)?  $default,{required TResult orElse(),}) {final _that = this;
switch (_that) {
case _DaemonSettings() when $default != null:
return $default(_that.deviceName,_that.downloadDir,_that.closeToTray,_that.plugins);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>(TResult Function( String deviceName,  String downloadDir,  bool closeToTray,  Map<String, Object?> plugins)  $default,) {final _that = this;
switch (_that) {
case _DaemonSettings():
return $default(_that.deviceName,_that.downloadDir,_that.closeToTray,_that.plugins);case _:
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>(TResult? Function( String deviceName,  String downloadDir,  bool closeToTray,  Map<String, Object?> plugins)?  $default,) {final _that = this;
switch (_that) {
case _DaemonSettings() when $default != null:
return $default(_that.deviceName,_that.downloadDir,_that.closeToTray,_that.plugins);case _:
  return null;

}
}

}

/// @nodoc
@JsonSerializable()

class _DaemonSettings extends DaemonSettings {
  const _DaemonSettings({required this.deviceName, required this.downloadDir, required this.closeToTray,  Map<String, Object?> plugins = const <String, Object?>{}}): _plugins = plugins,super._();
  factory _DaemonSettings.fromJson(Map<String, dynamic> json) => _$DaemonSettingsFromJson(json);

@override final  String deviceName;
@override final  String downloadDir;
/// Owned by the UI; the daemon only stores it.
@override final  bool closeToTray;
/// The daemon's plugins' settings, keyed by plugin id. Read through
/// getters such as [clipboardSyncEnabled].
 final  Map<String, Object?> _plugins;
/// The daemon's plugins' settings, keyed by plugin id. Read through
/// getters such as [clipboardSyncEnabled].
@override@JsonKey() Map<String, Object?> get plugins {
  if (_plugins is EqualUnmodifiableMapView) return _plugins;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableMapView(_plugins);
}


/// Create a copy of DaemonSettings
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$DaemonSettingsCopyWith<_DaemonSettings> get copyWith => __$DaemonSettingsCopyWithImpl<_DaemonSettings>(this, _$identity);

@override
Map<String, dynamic> toJson() {
  return _$DaemonSettingsToJson(this, );
}

@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _DaemonSettings&&(identical(other.deviceName, deviceName) || other.deviceName == deviceName)&&(identical(other.downloadDir, downloadDir) || other.downloadDir == downloadDir)&&(identical(other.closeToTray, closeToTray) || other.closeToTray == closeToTray)&&const DeepCollectionEquality().equals(other.plugins, _plugins));
}

@JsonKey(includeFromJson: false, includeToJson: false)
@override
int get hashCode {
    return Object.hash(runtimeType,deviceName,downloadDir,closeToTray,const DeepCollectionEquality().hash(_plugins));
}

@override
String toString() {
    return 'DaemonSettings(deviceName: $deviceName, downloadDir: $downloadDir, closeToTray: $closeToTray, plugins: $plugins)';
}


}

/// @nodoc
abstract mixin class _$DaemonSettingsCopyWith<$Res> implements $DaemonSettingsCopyWith<$Res> {
  factory _$DaemonSettingsCopyWith(_DaemonSettings value, $Res Function(_DaemonSettings) _then) = __$DaemonSettingsCopyWithImpl;
@override @useResult
$Res call({
 String deviceName, String downloadDir, bool closeToTray, Map<String, Object?> plugins
});




}
/// @nodoc
class __$DaemonSettingsCopyWithImpl<$Res>
    implements _$DaemonSettingsCopyWith<$Res> {
  __$DaemonSettingsCopyWithImpl(this._self, this._then);

  final _DaemonSettings _self;
  final $Res Function(_DaemonSettings) _then;

/// Create a copy of DaemonSettings
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? deviceName = null,Object? downloadDir = null,Object? closeToTray = null,Object? plugins = null,}) {
  return _then(_DaemonSettings(
deviceName: null == deviceName ? _self.deviceName : deviceName // ignore: cast_nullable_to_non_nullable
as String,downloadDir: null == downloadDir ? _self.downloadDir : downloadDir // ignore: cast_nullable_to_non_nullable
as String,closeToTray: null == closeToTray ? _self.closeToTray : closeToTray // ignore: cast_nullable_to_non_nullable
as bool,plugins: null == plugins ? _self._plugins : plugins // ignore: cast_nullable_to_non_nullable
as Map<String, Object?>,
  ));
}


}

// dart format on
