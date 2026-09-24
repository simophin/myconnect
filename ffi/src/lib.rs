//! C ABI that lets a GUI process embed a MyConnect daemon.
//!
//! The surface is deliberately tiny — start, stop, free — and speaks JSON
//! strings rather than C structs, so the ABI never changes when a start option
//! is added. Everything else a frontend does goes through the daemon's HTTP API
//! at the address `myconnect_start` returns.
//!
//! Every function returns a heap-allocated, NUL-terminated UTF-8 JSON string
//! that the caller must release with [`myconnect_free_string`]. On failure the
//! string is `{"error": "<message>"}`.

use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char},
    net::{IpAddr, Ipv4Addr},
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{
        Mutex, Once,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use myconnect::{
    application::{RunRequest, RunningService},
    config::ApiToken,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::runtime::Runtime;

/// Start options, mirroring `myconnect run`. Every field is optional.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct StartConfig {
    pub data_dir: Option<PathBuf>,
    pub download_dir: Option<PathBuf>,
    pub device_name: Option<String>,
    pub discovery_loopback: bool,
    /// Defaults to `127.0.0.1`.
    pub api_host: Option<IpAddr>,
    /// Defaults to `0`, letting the OS pick a free port.
    pub api_port: Option<u16>,
    /// Defaults to a freshly generated random token, so only the embedding
    /// process can use the instance it started.
    pub api_token: Option<String>,
}

/// What the embedder needs to talk to the started instance.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedInstance {
    pub handle: u64,
    pub api_host: IpAddr,
    pub api_port: u16,
    pub api_token: String,
}

struct Instance {
    runtime: Runtime,
    service: RunningService,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static INSTANCES: Mutex<Option<HashMap<u64, Instance>>> = Mutex::new(None);
static TRACING: Once = Once::new();

/// Start a daemon instance. `config_json` is a JSON [`StartConfig`] object, or
/// NULL for all defaults. Returns a [`StartedInstance`] as JSON.
///
/// Blocks until the LAN transport and control API are listening.
///
/// # Safety
///
/// `config_json` must be NULL or point to a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn myconnect_start(config_json: *const c_char) -> *mut c_char {
    respond(|| {
        let config = if config_json.is_null() {
            StartConfig::default()
        } else {
            // SAFETY: the caller guarantees a valid NUL-terminated string.
            let raw = unsafe { CStr::from_ptr(config_json) }
                .to_str()
                .context("start config is not UTF-8")?;
            serde_json::from_str(raw).context("invalid start config")?
        };
        let started = start(config)?;
        Ok(serde_json::to_value(started)?)
    })
}

/// Stop the instance returned by `myconnect_start`, shutting down its control
/// API, LAN transport, and in-flight transfers, then its runtime. Returns
/// `{"stopped": true}`.
#[unsafe(no_mangle)]
pub extern "C" fn myconnect_stop(handle: u64) -> *mut c_char {
    respond(|| {
        stop(handle)?;
        Ok(json!({ "stopped": true }))
    })
}

/// Release a string returned by any function in this library.
///
/// # Safety
///
/// `value` must be NULL or a pointer returned by this library that has not
/// been freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn myconnect_free_string(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: the caller guarantees the pointer came from `CString::into_raw`.
        drop(unsafe { CString::from_raw(value) });
    }
}

/// Start an instance and register it under a new handle.
pub fn start(config: StartConfig) -> Result<StartedInstance> {
    init_tracing();
    let api_token = match config.api_token {
        Some(secret) => ApiToken::from_secret(secret)?,
        None => ApiToken::generate(),
    };
    let request = RunRequest {
        api_token: Some(api_token.clone()),
        data_dir: config.data_dir,
        download_dir: config.download_dir,
        device_name: config.device_name,
        discovery_loopback: config.discovery_loopback,
        api_host: config.api_host.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        api_port: config.api_port.unwrap_or(0),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("myconnect")
        .build()
        .context("could not start async runtime")?;
    let service = runtime.block_on(RunningService::start(request))?;
    let address = service.api_addr();
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    instances()?
        .get_or_insert_with(HashMap::new)
        .insert(handle, Instance { runtime, service });
    Ok(StartedInstance {
        handle,
        api_host: address.ip(),
        api_port: address.port(),
        api_token: api_token.expose_secret().to_owned(),
    })
}

/// Stop and forget the instance registered under `handle`.
pub fn stop(handle: u64) -> Result<()> {
    let Instance { runtime, service } = instances()?
        .as_mut()
        .and_then(|instances| instances.remove(&handle))
        .ok_or_else(|| anyhow!("unknown instance handle {handle}"))?;
    let result = runtime.block_on(service.shutdown());
    runtime.shutdown_timeout(Duration::from_secs(2));
    result
}

fn instances() -> Result<std::sync::MutexGuard<'static, Option<HashMap<u64, Instance>>>> {
    INSTANCES
        .lock()
        .map_err(|_| anyhow!("instance registry is poisoned"))
}

fn init_tracing() {
    TRACING.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
        // The host process may already have installed a subscriber.
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    });
}

/// Run `body`, converting errors and panics into `{"error": ...}` so nothing
/// unwinds across the C ABI.
fn respond(body: impl FnOnce() -> Result<serde_json::Value>) -> *mut c_char {
    let value = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => json!({ "error": format!("{error:#}") }),
        Err(_) => json!({ "error": "internal panic in myconnect" }),
    };
    CString::new(value.to_string())
        .expect("JSON never contains NUL bytes")
        .into_raw()
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };

    use super::*;

    fn call(result: *mut c_char) -> serde_json::Value {
        // SAFETY: `result` was just returned by this library.
        let text = unsafe { CStr::from_ptr(result) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { myconnect_free_string(result) };
        serde_json::from_str(&text).unwrap()
    }

    fn get_status(port: u16, token: Option<&str>) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let authorization = token
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "GET /api/v1/status HTTP/1.1\r\nHost: localhost\r\n{authorization}Connection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn start_serves_an_authenticated_api_on_a_free_port_until_stopped() {
        let directory = tempfile::tempdir().unwrap();
        let config = CString::new(
            json!({
                "dataDir": directory.path().join("data"),
                "downloadDir": directory.path().join("downloads"),
                "deviceName": "FFI Test",
                "discoveryLoopback": true,
            })
            .to_string(),
        )
        .unwrap();

        let started = call(unsafe { myconnect_start(config.as_ptr()) });
        let handle = started["handle"].as_u64().expect("started instance");
        let port = started["apiPort"].as_u64().unwrap() as u16;
        let token = started["apiToken"].as_str().unwrap().to_owned();
        assert_ne!(port, 0);
        assert_eq!(token.len(), 64);

        assert!(get_status(port, None).starts_with("HTTP/1.1 401"));
        let status = get_status(port, Some(&token));
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        assert!(status.contains("FFI Test"));

        assert_eq!(call(myconnect_stop(handle)), json!({ "stopped": true }));
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
        assert!(call(myconnect_stop(handle))["error"].is_string());
    }

    #[test]
    fn invalid_config_is_reported_as_an_error() {
        let config = CString::new(r#"{"unknownOption": 1}"#).unwrap();
        let result = call(unsafe { myconnect_start(config.as_ptr()) });
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("invalid start config")
        );
    }
}
