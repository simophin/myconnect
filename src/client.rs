//! Client for the local MyConnect control API.

use std::{env, net::Ipv4Addr, path::Path, pin::Pin, time::Duration};

use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::{
    Client, Response, StatusCode, Url,
    header::{CONTENT_LENGTH, HeaderMap, HeaderValue},
    multipart::{Form, Part},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs::File, time::sleep};
use tokio_util::{io::ReaderStream, sync::CancellationToken};
use uuid::Uuid;

use crate::{
    api::DEFAULT_API_PORT,
    config::ApiToken,
    core::{
        CoreEvent, DeviceSnapshot, EventData, PairingSnapshot, PluginEventKind, SettingsPatch,
        SettingsSnapshot, TransferSnapshot, TransferStatus,
    },
    plugins::{
        browse::{DirectoryListing, FileEntry},
        clipboard::ClipboardSnapshot,
    },
};

pub const API_URL_ENV: &str = "MYCONNECT_API_URL";
pub const API_TOKEN_ENV: &str = "MYCONNECT_API_TOKEN";

pub type EventStream = Pin<Box<dyn Stream<Item = Result<CoreEvent, ClientError>> + Send + 'static>>;
pub type ByteStream =
    Pin<Box<dyn Stream<Item = Result<bytes::Bytes, ClientError>> + Send + 'static>>;

pub struct ApiClient {
    base_url: Url,
    token: Option<ApiToken>,
    http: Client,
}

impl ApiClient {
    pub fn from_environment() -> Result<Self, ClientError> {
        Self::from_environment_with(None, None)
    }

    /// Like [`ApiClient::from_environment`], but explicit values (e.g. from
    /// `--api-host`/`--api-port`/`--api-token` flags) take precedence over the
    /// `MYCONNECT_API_URL` and `MYCONNECT_API_TOKEN` environment variables.
    /// Without a token from either source, requests are sent unauthenticated.
    pub fn from_environment_with(
        base_url_override: Option<String>,
        token_override: Option<ApiToken>,
    ) -> Result<Self, ClientError> {
        let base_url = base_url_override.unwrap_or_else(|| {
            env::var(API_URL_ENV).unwrap_or_else(|_| format!("http://127.0.0.1:{DEFAULT_API_PORT}"))
        });
        let token = match token_override {
            Some(token) => Some(token),
            None => match env::var(API_TOKEN_ENV) {
                Ok(secret) if secret.is_empty() => None,
                Ok(secret) => Some(ApiToken::from_secret(secret)?),
                Err(env::VarError::NotPresent) => None,
                Err(env::VarError::NotUnicode(_)) => return Err(ClientError::InvalidToken),
            },
        };
        Self::new(&base_url, token)
    }

    pub fn new(base_url: &str, token: Option<ApiToken>) -> Result<Self, ClientError> {
        let mut base_url = Url::parse(base_url).map_err(|_| ClientError::InvalidApiUrl)?;
        if base_url.scheme() != "http" {
            return Err(ClientError::UnsupportedScheme);
        }
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        Ok(Self {
            base_url,
            token,
            http: Client::builder()
                .connect_timeout(Duration::from_secs(3))
                // The server emits SSE keepalives every 15 seconds, so a read
                // deadline detects a dead daemon without imposing a total
                // lifetime on watch connections.
                .read_timeout(Duration::from_secs(30))
                .build()
                .map_err(ClientError::Build)?,
        })
    }

    pub async fn devices(&self) -> Result<Vec<DeviceSnapshot>, ClientError> {
        self.get_json("api/v1/devices", "device").await
    }

    /// Trigger an immediate discovery broadcast, so newly reachable devices
    /// show up in [`ApiClient::devices`] without waiting for the periodic
    /// announce interval. With an `address`, announce to that IPv4 address
    /// only, for networks where broadcast doesn't reach the peer.
    pub async fn scan(&self, address: Option<Ipv4Addr>) -> Result<(), ClientError> {
        #[derive(Serialize)]
        struct Discovery {
            address: String,
        }

        let mut request = self.authorized(self.http.post(self.url("api/v1/discovery")?));
        if let Some(address) = address {
            request = request.json(&Discovery {
                address: address.to_string(),
            });
        }
        let response = request.send().await.map_err(map_transport)?;
        checked(response, "discovery").await?;
        Ok(())
    }

    pub async fn start_pairing(&self, device_id: &str) -> Result<PairingSnapshot, ClientError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct StartPairing<'a> {
            device_id: &'a str,
        }

        let response = self
            .authorized(self.http.post(self.url("api/v1/pairings")?))
            .json(&StartPairing { device_id })
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "pairing").await
    }

    pub async fn pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ClientError> {
        self.get_json(&format!("api/v1/pairings/{pairing_id}"), "pairing")
            .await
    }

    pub async fn accept_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ClientError> {
        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/pairings/{pairing_id}/accept"))?),
            )
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "pairing").await
    }

    pub async fn reject_pairing(&self, pairing_id: Uuid) -> Result<(), ClientError> {
        let response = self
            .authorized(
                self.http
                    .delete(self.url(&format!("api/v1/pairings/{pairing_id}"))?),
            )
            .send()
            .await
            .map_err(map_transport)?;
        checked(response, "pairing").await?;
        Ok(())
    }

    pub async fn unpair(&self, device_id: &str) -> Result<(), ClientError> {
        let response = self
            .authorized(
                self.http
                    .delete(self.url(&format!("api/v1/devices/{device_id}"))?),
            )
            .send()
            .await
            .map_err(map_transport)?;
        checked(response, "device").await?;
        Ok(())
    }

    /// Ping a paired, connected device, optionally attaching a message.
    pub async fn ping(&self, device_id: &str, message: Option<&str>) -> Result<(), ClientError> {
        #[derive(Serialize)]
        struct Ping<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            message: Option<&'a str>,
        }

        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/ping"))?),
            )
            .json(&Ping { message })
            .send()
            .await
            .map_err(map_transport)?;
        checked(response, "device").await?;
        Ok(())
    }

    /// Ask a paired, connected device to ring so it can be found.
    pub async fn ring(&self, device_id: &str) -> Result<(), ClientError> {
        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/ring"))?),
            )
            .send()
            .await
            .map_err(map_transport)?;
        checked(response, "device").await?;
        Ok(())
    }

    /// Send this machine's clipboard text to a paired, connected device.
    pub async fn send_clipboard(&self, device_id: &str) -> Result<(), ClientError> {
        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/clipboard"))?),
            )
            .send()
            .await
            .map_err(map_transport)?;
        checked(response, "device").await?;
        Ok(())
    }

    /// Send a file to a paired, connected device. The response comes once
    /// the whole file has been handed to the daemon, not when the device
    /// has it; follow the transfer to see it finish.
    pub async fn send_file(
        &self,
        device_id: &str,
        path: &Path,
    ) -> Result<TransferSnapshot, ClientError> {
        let (file_name, part) = file_part(path).await?;
        let form = Form::new().part("file", part.file_name(file_name));
        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/share"))?),
            )
            .multipart(form)
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "transfer").await
    }

    /// List a directory on a paired device, or, without `path`, the
    /// storage roots it shares.
    pub async fn list_files(
        &self,
        device_id: &str,
        path: Option<&str>,
    ) -> Result<DirectoryListing, ClientError> {
        let mut url = self.url(&format!("api/v1/devices/{device_id}/files"))?;
        if let Some(path) = path {
            url.query_pairs_mut().append_pair("path", path);
        }
        let response = self
            .authorized(self.http.get(url))
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "device or directory").await
    }

    /// Stream a file's content from a paired device.
    pub async fn file_content(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<ByteStream, ClientError> {
        let mut url = self.url(&format!("api/v1/devices/{device_id}/files/content"))?;
        url.query_pairs_mut().append_pair("path", path);
        let response = self
            .authorized(self.http.get(url))
            .send()
            .await
            .map_err(map_transport)?;
        let response = checked(response, "device or file").await?;
        Ok(Box::pin(
            response
                .bytes_stream()
                .map(|chunk| chunk.map_err(map_transport)),
        ))
    }

    /// Save a file from a paired device into the daemon's download
    /// directory, as an incoming transfer.
    pub async fn download_file(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<TransferSnapshot, ClientError> {
        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/files/download"))?),
            )
            .json(&FilePath { path })
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "device or file").await
    }

    /// Upload a local file into `directory` on a paired device. Completes
    /// once the whole file has been forwarded.
    pub async fn upload_file(
        &self,
        device_id: &str,
        directory: &str,
        path: &Path,
    ) -> Result<TransferSnapshot, ClientError> {
        let (file_name, part) = file_part(path).await?;
        let form = Form::new()
            .text("path", directory.to_owned())
            .part("file", part.file_name(file_name));
        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/files/upload"))?),
            )
            .multipart(form)
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "device or directory").await
    }

    /// Create a directory on a paired device.
    pub async fn create_directory(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<FileEntry, ClientError> {
        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/files/directories"))?),
            )
            .json(&FilePath { path })
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "device or directory").await
    }

    /// Move or rename a file or directory on a paired device.
    pub async fn move_file(
        &self,
        device_id: &str,
        from: &str,
        to: &str,
    ) -> Result<FileEntry, ClientError> {
        #[derive(Serialize)]
        struct Move<'a> {
            from: &'a str,
            to: &'a str,
        }

        let response = self
            .authorized(
                self.http
                    .post(self.url(&format!("api/v1/devices/{device_id}/files/move"))?),
            )
            .json(&Move { from, to })
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "device or file").await
    }

    /// Delete a file, or a directory and everything in it, on a paired
    /// device.
    pub async fn delete_file(&self, device_id: &str, path: &str) -> Result<(), ClientError> {
        let mut url = self.url(&format!("api/v1/devices/{device_id}/files"))?;
        url.query_pairs_mut().append_pair("path", path);
        let response = self
            .authorized(self.http.delete(url))
            .send()
            .await
            .map_err(map_transport)?;
        checked(response, "device or file").await?;
        Ok(())
    }

    pub async fn transfer(&self, transfer_id: Uuid) -> Result<TransferSnapshot, ClientError> {
        self.get_json(&format!("api/v1/transfers/{transfer_id}"), "transfer")
            .await
    }

    pub async fn clipboard(&self) -> Result<ClipboardSnapshot, ClientError> {
        self.get_json("api/v1/clipboard", "clipboard").await
    }

    pub async fn set_clipboard(&self, text: &str) -> Result<ClipboardSnapshot, ClientError> {
        #[derive(Serialize)]
        struct SetClipboard<'a> {
            text: &'a str,
        }

        let response = self
            .authorized(self.http.put(self.url("api/v1/clipboard")?))
            .json(&SetClipboard { text })
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "clipboard").await
    }

    pub async fn settings(&self) -> Result<SettingsSnapshot, ClientError> {
        self.get_json("api/v1/settings", "settings").await
    }

    pub async fn update_settings(
        &self,
        patch: &SettingsPatch,
    ) -> Result<SettingsSnapshot, ClientError> {
        let response = self
            .authorized(self.http.patch(self.url("api/v1/settings")?))
            .json(patch)
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, "settings").await
    }

    pub async fn events(&self) -> Result<EventStream, ClientError> {
        let response = self
            .authorized(self.http.get(self.url("api/v1/events")?))
            .send()
            .await
            .map_err(map_transport)?;
        let response = checked(response, "event stream").await?;
        let mut chunks = response.bytes_stream();
        let stream = async_stream::try_stream! {
            let mut pending = Vec::new();
            while let Some(chunk) = chunks.next().await {
                pending.extend_from_slice(&chunk.map_err(map_transport)?);
                while let Some((end, delimiter_length)) = find_sse_frame(&pending) {
                    let frame: Vec<u8> = pending.drain(..end + delimiter_length).collect();
                    if let Some(event) = decode_sse_frame(&frame)? {
                        yield event;
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }

    pub async fn watch_devices<F>(
        &self,
        cancellation: CancellationToken,
        mut on_update: F,
    ) -> Result<(), ClientError>
    where
        F: FnMut(DeviceWatchUpdate) + Send,
    {
        loop {
            on_update(DeviceWatchUpdate::Snapshot(self.devices().await?));
            if cancellation.is_cancelled() {
                return Ok(());
            }
            let mut events = self.events().await?;
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => return Ok(()),
                    event = events.next() => match event {
                        Some(Ok(event)) if is_device_event(&event.event) => {
                            on_update(DeviceWatchUpdate::Event(event));
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => break,
                    }
                }
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                _ = sleep(Duration::from_millis(250)) => {}
            }
        }
    }

    pub async fn watch_clipboard<F>(
        &self,
        cancellation: CancellationToken,
        mut on_update: F,
    ) -> Result<(), ClientError>
    where
        F: FnMut(ClipboardWatchUpdate) + Send,
    {
        loop {
            on_update(ClipboardWatchUpdate::Snapshot(self.clipboard().await?));
            if cancellation.is_cancelled() {
                return Ok(());
            }
            let mut events = self.events().await?;
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => return Ok(()),
                    event = events.next() => match event {
                        Some(Ok(event)) if event.event.event_type() == ClipboardSnapshot::TYPE => {
                            on_update(ClipboardWatchUpdate::Event(event));
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => break,
                    }
                }
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                _ = sleep(Duration::from_millis(250)) => {}
            }
        }
    }

    pub async fn watch_transfer<F>(
        &self,
        transfer_id: Uuid,
        cancellation: CancellationToken,
        mut on_update: F,
    ) -> Result<(), ClientError>
    where
        F: FnMut(TransferWatchUpdate) + Send,
    {
        loop {
            let snapshot = self.transfer(transfer_id).await?;
            let terminal = transfer_is_terminal(snapshot.status);
            on_update(TransferWatchUpdate::Snapshot(snapshot));
            if terminal || cancellation.is_cancelled() {
                return Ok(());
            }
            let mut events = self.events().await?;
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => return Ok(()),
                    event = events.next() => match event {
                        Some(Ok(event)) => {
                            if let Some(transfer) = transfer_from_event(&event.event)
                                && transfer.id == transfer_id
                            {
                                let terminal = transfer_is_terminal(transfer.status);
                                on_update(TransferWatchUpdate::Event(event));
                                if terminal {
                                    return Ok(());
                                }
                            }
                        }
                        Some(Err(_)) | None => break,
                    }
                }
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                _ = sleep(Duration::from_millis(250)) => {}
            }
        }
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        resource: &'static str,
    ) -> Result<T, ClientError> {
        let response = self
            .authorized(self.http.get(self.url(path)?))
            .send()
            .await
            .map_err(map_transport)?;
        decode_json(response, resource).await
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => request.bearer_auth(token.expose_secret()),
            None => request,
        }
    }

    fn url(&self, path: &str) -> Result<Url, ClientError> {
        self.base_url
            .join(path)
            .map_err(|_| ClientError::InvalidApiUrl)
    }
}

#[derive(Serialize)]
struct FilePath<'a> {
    path: &'a str,
}

/// A multipart part streaming the file at `path`, and the file's name.
async fn file_part(path: &Path) -> Result<(String, Part), ClientError> {
    let file = File::open(path)
        .await
        .map_err(|source| ClientError::OpenFile {
            path: path.to_owned(),
            source,
        })?;
    let length = file
        .metadata()
        .await
        .map_err(|source| ClientError::OpenFile {
            path: path.to_owned(),
            source,
        })?
        .len();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(ClientError::InvalidFileName)?
        .to_owned();
    let stream = ReaderStream::new(file);
    // The daemon checks the declared size before streaming, and reads it
    // from the part's own Content-Length header, which
    // `stream_with_length` does not set.
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_LENGTH, HeaderValue::from(length));
    let part = Part::stream_with_length(reqwest::Body::wrap_stream(stream), length)
        .headers(headers)
        .mime_str("application/octet-stream")
        .map_err(ClientError::Build)?;
    Ok((file_name, part))
}

pub enum DeviceWatchUpdate {
    Snapshot(Vec<DeviceSnapshot>),
    Event(CoreEvent),
}

pub enum ClipboardWatchUpdate {
    Snapshot(ClipboardSnapshot),
    Event(CoreEvent),
}

pub enum TransferWatchUpdate {
    Snapshot(TransferSnapshot),
    Event(CoreEvent),
}

fn is_device_event(event: &EventData) -> bool {
    matches!(
        event,
        EventData::DeviceDiscovered(_)
            | EventData::DeviceConnected(_)
            | EventData::DeviceUpdated(_)
            | EventData::DeviceDisconnected(_)
            | EventData::DeviceForgotten(_)
    )
}

fn transfer_from_event(event: &EventData) -> Option<&TransferSnapshot> {
    match event {
        EventData::TransferStarted(transfer)
        | EventData::TransferProgress(transfer)
        | EventData::TransferCompleted(transfer)
        | EventData::TransferFailed(transfer) => Some(transfer),
        _ => None,
    }
}

fn transfer_is_terminal(status: TransferStatus) -> bool {
    matches!(
        status,
        TransferStatus::Completed | TransferStatus::Cancelled | TransferStatus::Failed
    )
}

fn find_sse_frame(bytes: &[u8]) -> Option<(usize, usize)> {
    let lf = bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2));
    let crlf = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(frame), None) | (None, Some(frame)) => Some(frame),
        (None, None) => None,
    }
}

fn decode_sse_frame(frame: &[u8]) -> Result<Option<CoreEvent>, ClientError> {
    let mut data = Vec::new();
    for line in frame.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(value) = line.strip_prefix(b"data:") {
            if !data.is_empty() {
                data.push(b'\n');
            }
            data.extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
        }
    }
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(&data)
        .map(Some)
        .map_err(|_| ClientError::InvalidEvent)
}

async fn decode_json<T: for<'de> Deserialize<'de>>(
    response: Response,
    resource: &'static str,
) -> Result<T, ClientError> {
    checked(response, resource)
        .await?
        .json()
        .await
        .map_err(|_| ClientError::InvalidResponse)
}

async fn checked(response: Response, resource: &'static str) -> Result<Response, ClientError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    if status == StatusCode::UNAUTHORIZED {
        return Err(ClientError::Unauthorized);
    }
    if status == StatusCode::NOT_FOUND {
        return Err(ClientError::NotFound(resource));
    }
    let code = response
        .json::<Problem>()
        .await
        .ok()
        .map(|problem| problem.code)
        .unwrap_or_else(|| "unknown_error".to_owned());
    Err(ClientError::OperationFailed {
        status: status.as_u16(),
        code,
    })
}

fn map_transport(error: reqwest::Error) -> ClientError {
    if error.is_connect() || error.is_timeout() {
        ClientError::DaemonUnavailable
    } else {
        ClientError::Transport(error)
    }
}

#[derive(Deserialize)]
struct Problem {
    code: String,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("the MyConnect daemon is unavailable; start it with `myconnect run`")]
    DaemonUnavailable,
    #[error("the daemon requires a valid API token; pass --api-token or set {API_TOKEN_ENV}")]
    Unauthorized,
    #[error("the requested {0} was not found")]
    NotFound(&'static str),
    #[error("the daemon could not complete the operation ({status}: {code})")]
    OperationFailed { status: u16, code: String },
    #[error("the daemon returned an invalid response")]
    InvalidResponse,
    #[error("the daemon returned an invalid event")]
    InvalidEvent,
    #[error("the API URL is invalid")]
    InvalidApiUrl,
    #[error("the API URL must use HTTP")]
    UnsupportedScheme,
    #[error("the API token is invalid")]
    InvalidToken,
    #[error("could not open {path}")]
    OpenFile {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the file name is not valid Unicode")]
    InvalidFileName,
    #[error("could not construct the HTTP client request")]
    Build(#[source] reqwest::Error),
    #[error("local API request failed")]
    Transport(#[source] reqwest::Error),
    #[error("the API token is invalid")]
    Token(#[from] crate::config::ApiTokenError),
}
