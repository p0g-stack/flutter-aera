//! `flutter.baseflow.com/permissions/methods` (StandardMethodCodec), the
//! channel of the stock `permission_handler` plugin. It has no Linux half,
//! so on a desktop every call fails with MissingPluginException; here the
//! embedder answers it, so apps that check or request permissions work
//! unchanged.
//!
//! An AERA plugin runs as root and reads any file, so the storage and media
//! permissions are granted. Everything else needs hardware or a service
//! recovery doesn't have (camera, microphone, location, contacts,
//! notifications, Bluetooth...): it is `denied` until asked and
//! `permanentlyDenied` once asked, as on Android when the user can never
//! grant it, and `openAppSettings` opens nothing. Service status is
//! `disabled` for location, phone and Bluetooth, `notApplicable` otherwise.

use super::codec;
use super::standard::{self, Value};

pub const CHANNEL: &str = "flutter.baseflow.com/permissions/methods";

// PermissionStatus.value
const DENIED: i64 = 0;
const GRANTED: i64 = 1;
const PERMANENTLY_DENIED: i64 = 4;

// ServiceStatus.value
const DISABLED: i64 = 0;
const NOT_APPLICABLE: i64 = 2;

/// Permission.value of what AERA grants: mediaLibrary, photos, storage,
/// ignoreBatteryOptimizations, accessMediaLocation, manageExternalStorage,
/// videos, audio.
fn grantable(permission: i64) -> bool {
    matches!(permission, 6 | 9 | 15 | 16 | 18 | 22 | 32 | 33)
}

fn status(permission: i64, asked: bool) -> i64 {
    if grantable(permission) {
        GRANTED
    } else if asked {
        PERMANENTLY_DENIED
    } else {
        DENIED
    }
}

/// Permission.value of what has a system service: location,
/// locationAlways, locationWhenInUse, phone, bluetooth.
fn service_status(permission: i64) -> i64 {
    if matches!(permission, 3 | 4 | 5 | 8 | 21) { DISABLED } else { NOT_APPLICABLE }
}

fn success(value: &Value) -> Vec<u8> {
    let mut reply = vec![0];
    standard::encode_into(&mut reply, value);
    reply
}

pub fn handle(bytes: &[u8]) -> Vec<u8> {
    let Some((method, args)) = standard::decode_call(bytes) else {
        return codec::not_implemented();
    };
    let int = |v: &Value| if let Value::Int(i) = v { Some(*i) } else { None };
    match (method.as_str(), &args) {
        ("checkPermissionStatus", Value::Int(p)) => success(&Value::Int(status(*p, false))),
        ("checkServiceStatus", Value::Int(p)) => success(&Value::Int(service_status(*p))),
        ("shouldShowRequestPermissionRationale", Value::Int(_)) => success(&Value::Bool(false)),
        ("openAppSettings", _) => success(&Value::Bool(false)),
        ("requestPermissions", Value::List(list)) => {
            let results = list.iter().filter_map(int).map(|p| (Value::Int(p), Value::Int(status(p, true)))).collect();
            success(&Value::Map(results))
        }
        _ => codec::not_implemented(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(method: &str, args: Value) -> Vec<u8> {
        let mut bytes = standard::encode(&Value::Str(method.into()));
        standard::encode_into(&mut bytes, &args);
        bytes
    }

    fn result(reply: Vec<u8>) -> Value {
        assert_eq!(reply[0], 0, "success envelope");
        standard::decode(&reply[1..]).unwrap()
    }

    #[test]
    fn storage_is_granted_camera_is_not() {
        assert_eq!(result(handle(&call("checkPermissionStatus", Value::Int(15)))), Value::Int(GRANTED));
        assert_eq!(result(handle(&call("checkPermissionStatus", Value::Int(1)))), Value::Int(DENIED));
        let requested = result(handle(&call("requestPermissions", Value::List(vec![Value::Int(1), Value::Int(33)]))));
        assert_eq!(
            requested,
            Value::Map(vec![(Value::Int(1), Value::Int(PERMANENTLY_DENIED)), (Value::Int(33), Value::Int(GRANTED))])
        );
    }

    #[test]
    fn services_rationale_and_settings() {
        assert_eq!(result(handle(&call("checkServiceStatus", Value::Int(3)))), Value::Int(DISABLED));
        assert_eq!(result(handle(&call("checkServiceStatus", Value::Int(1)))), Value::Int(NOT_APPLICABLE));
        assert_eq!(result(handle(&call("shouldShowRequestPermissionRationale", Value::Int(1)))), Value::Bool(false));
        assert_eq!(result(handle(&call("openAppSettings", Value::Null))), Value::Bool(false));
        assert!(handle(&call("somethingNew", Value::Null)).is_empty());
    }
}
