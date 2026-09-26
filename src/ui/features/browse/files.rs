//! What the browser does with a device's files: [`BrowsePlugin`]'s methods
//! behind a trait, so tests can put a fake phone's files in their place.

use std::{path::PathBuf, sync::Arc};

use futures_util::future::BoxFuture;
use tokio::io::AsyncReadExt;

use crate::{
    core::{PluginContext, TransferSnapshot},
    plugins::browse::{BrowseError, BrowsePlugin, DirectoryListing, FileEntry, UploadPathError},
};

/// A future of a device's answer.
pub type Answer<T> = BoxFuture<'static, Result<T, BrowseError>>;

/// What the browser does with a device's files: [`BrowsePlugin`]'s
/// methods, or a fake in tests. The futures run on the daemon's runtime.
pub trait Files: Send + Sync + 'static {
    fn list(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        folder: Option<String>,
    ) -> Answer<DirectoryListing>;
    /// A file's content, cut off after `limit` bytes.
    fn read(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        path: String,
        limit: u64,
    ) -> Answer<Vec<u8>>;
    fn download(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        path: String,
    ) -> Answer<TransferSnapshot>;
    fn upload(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        folder: String,
        local: PathBuf,
    ) -> BoxFuture<'static, Result<TransferSnapshot, UploadPathError>>;
    fn create_directory(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        path: String,
    ) -> Answer<FileEntry>;
    fn move_file(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        from: String,
        to: String,
    ) -> Answer<FileEntry>;
    fn delete(self: Arc<Self>, ctx: PluginContext, device_id: String, path: String) -> Answer<()>;
}

impl Files for BrowsePlugin {
    fn list(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        folder: Option<String>,
    ) -> Answer<DirectoryListing> {
        Box::pin(async move { self.list_files(&ctx, &device_id, folder.as_deref()).await })
    }

    fn read(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        path: String,
        limit: u64,
    ) -> Answer<Vec<u8>> {
        Box::pin(async move {
            let content = self.open_file(&ctx, &device_id, &path).await?;
            let mut bytes = Vec::new();
            content
                .take(limit)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| BrowseError::Failed)?;
            Ok(bytes)
        })
    }

    fn download(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        path: String,
    ) -> Answer<TransferSnapshot> {
        Box::pin(async move { BrowsePlugin::download(&self, &ctx, &device_id, &path).await })
    }

    fn upload(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        folder: String,
        local: PathBuf,
    ) -> BoxFuture<'static, Result<TransferSnapshot, UploadPathError>> {
        Box::pin(async move { self.upload_path(&ctx, &device_id, &folder, &local).await })
    }

    fn create_directory(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        path: String,
    ) -> Answer<FileEntry> {
        Box::pin(
            async move { BrowsePlugin::create_directory(&self, &ctx, &device_id, &path).await },
        )
    }

    fn move_file(
        self: Arc<Self>,
        ctx: PluginContext,
        device_id: String,
        from: String,
        to: String,
    ) -> Answer<FileEntry> {
        Box::pin(async move { BrowsePlugin::move_file(&self, &ctx, &device_id, &from, &to).await })
    }

    fn delete(self: Arc<Self>, ctx: PluginContext, device_id: String, path: String) -> Answer<()> {
        Box::pin(async move { BrowsePlugin::delete(&self, &ctx, &device_id, &path).await })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        collections::HashMap,
        io,
        sync::{Mutex, PoisonError},
    };

    use chrono::TimeZone;

    use super::*;
    use crate::{
        core::{DeviceSnapshot, TransferDirection, TransferStatus},
        plugins::browse::{FileKind, REQUEST_PACKET_TYPE},
        ui::{features::browse::BrowseUi, pages::transfers::tests::transfer, testing},
    };

    pub(crate) const INTERNAL: &str = "/storage/emulated/0";
    pub(crate) const SD_CARD: &str = "/storage/sdcard";

    /// What the browser asked of the device.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Call {
        List(Option<String>),
        Read(String),
        Download(String),
        Upload { folder: String, local: PathBuf },
        CreateDirectory(String),
        Move(String, String),
        Delete(String),
    }

    /// A phone's files, held in memory: every call is recorded, folders
    /// not listed here don't exist.
    #[derive(Default)]
    pub(crate) struct FakeFiles {
        pub(crate) listings: Mutex<HashMap<Option<String>, DirectoryListing>>,
        pub(crate) calls: Mutex<Vec<Call>>,
        /// What listing answers instead, if set.
        pub(crate) refuse: Mutex<Option<fn() -> BrowseError>>,
        /// What changes answer instead, if set.
        pub(crate) fail_changes: Mutex<Option<fn() -> BrowseError>>,
        /// Every file's content.
        pub(crate) content: Mutex<Vec<u8>>,
    }

    pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    impl FakeFiles {
        pub(crate) fn calls(&self) -> Vec<Call> {
            lock(&self.calls).clone()
        }

        fn record(&self, call: Call) {
            lock(&self.calls).push(call);
        }

        fn change<T: Send + 'static>(&self, call: Call, answer: T) -> Answer<T> {
            self.record(call);
            let failure = *lock(&self.fail_changes);
            Box::pin(async move { failure.map_or(Ok(answer), |failure| Err(failure())) })
        }

        pub(crate) fn listed(&self, folder: Option<&str>) -> usize {
            let call = Call::List(folder.map(str::to_owned));
            self.calls().iter().filter(|made| **made == call).count()
        }
    }

    pub(crate) fn directory(path: &str, name: Option<&str>) -> FileEntry {
        FileEntry {
            name: name
                .unwrap_or_else(|| path.rsplit('/').next().unwrap())
                .into(),
            path: path.into(),
            kind: FileKind::Directory,
            size: None,
            modified_at: None,
        }
    }

    fn september_24_at_14_03() -> u64 {
        chrono::Local
            .with_ymd_and_hms(2026, 9, 24, 14, 3, 0)
            .unwrap()
            .timestamp_millis()
            .try_into()
            .unwrap()
    }

    pub(crate) fn file(path: &str, size: u64) -> FileEntry {
        FileEntry {
            name: path.rsplit('/').next().unwrap().into(),
            path: path.into(),
            kind: FileKind::File,
            size: Some(size),
            modified_at: Some(september_24_at_14_03()),
        }
    }

    /// A phone sharing its internal storage and an SD card, with a few
    /// files.
    pub(crate) fn phone_files() -> Arc<FakeFiles> {
        let listing = |path: Option<&str>, entries| DirectoryListing {
            path: path.map(str::to_owned),
            entries,
        };
        let camera = format!("{INTERNAL}/DCIM/Camera");
        let dcim = format!("{INTERNAL}/DCIM");
        let files = FakeFiles::default();
        *lock(&files.listings) = HashMap::from([
            (
                None,
                listing(
                    None,
                    vec![
                        directory(INTERNAL, Some("All files")),
                        directory(SD_CARD, Some("SD card")),
                    ],
                ),
            ),
            (
                Some(INTERNAL.into()),
                listing(
                    Some(INTERNAL),
                    vec![
                        file(&format!("{INTERNAL}/notes.txt"), 2048),
                        directory(&dcim, None),
                        file(&format!("{INTERNAL}/photo.png"), 100),
                        file(&format!("{INTERNAL}/.nomedia"), 0),
                    ],
                ),
            ),
            (
                Some(dcim.clone()),
                listing(Some(&dcim), vec![directory(&camera, None)]),
            ),
            (Some(camera.clone()), listing(Some(&camera), vec![])),
            (Some(SD_CARD.into()), listing(Some(SD_CARD), vec![])),
        ]);
        Arc::new(files)
    }

    impl Files for FakeFiles {
        fn list(
            self: Arc<Self>,
            _ctx: PluginContext,
            _device_id: String,
            folder: Option<String>,
        ) -> Answer<DirectoryListing> {
            self.record(Call::List(folder.clone()));
            let answer = match *lock(&self.refuse) {
                Some(refuse) => Err(refuse()),
                None => lock(&self.listings)
                    .get(&folder)
                    .cloned()
                    .ok_or(BrowseError::NotFound),
            };
            Box::pin(async move { answer })
        }

        fn read(
            self: Arc<Self>,
            _ctx: PluginContext,
            _device_id: String,
            path: String,
            _limit: u64,
        ) -> Answer<Vec<u8>> {
            self.record(Call::Read(path));
            let content = lock(&self.content).clone();
            Box::pin(async move { Ok(content) })
        }

        fn download(
            self: Arc<Self>,
            _ctx: PluginContext,
            _device_id: String,
            path: String,
        ) -> Answer<TransferSnapshot> {
            let name = path.rsplit('/').next().unwrap().to_owned();
            self.record(Call::Download(path));
            Box::pin(async move {
                Ok(transfer(
                    &name,
                    TransferDirection::Incoming,
                    TransferStatus::Queued,
                    1,
                ))
            })
        }

        fn upload(
            self: Arc<Self>,
            _ctx: PluginContext,
            _device_id: String,
            folder: String,
            local: PathBuf,
        ) -> BoxFuture<'static, Result<TransferSnapshot, UploadPathError>> {
            self.record(Call::Upload {
                folder,
                local: local.clone(),
            });
            Box::pin(async move {
                if !local.is_file() {
                    return Err(UploadPathError::File(io::ErrorKind::NotFound.into()));
                }
                Ok(transfer(
                    "upload",
                    TransferDirection::Outgoing,
                    TransferStatus::Transferring,
                    1,
                ))
            })
        }

        fn create_directory(
            self: Arc<Self>,
            _ctx: PluginContext,
            _device_id: String,
            path: String,
        ) -> Answer<FileEntry> {
            let entry = directory(&path, None);
            self.change(Call::CreateDirectory(path), entry)
        }

        fn move_file(
            self: Arc<Self>,
            _ctx: PluginContext,
            _device_id: String,
            from: String,
            to: String,
        ) -> Answer<FileEntry> {
            let entry = file(&to, 1);
            self.change(Call::Move(from, to), entry)
        }

        fn delete(
            self: Arc<Self>,
            _ctx: PluginContext,
            _device_id: String,
            path: String,
        ) -> Answer<()> {
            self.change(Call::Delete(path), ())
        }
    }

    /// A paired, connected phone named "Pixel" that shares its files.
    pub(crate) fn pixel() -> DeviceSnapshot {
        let mut device = testing::device("Pixel");
        device.incoming_capabilities = vec![REQUEST_PACKET_TYPE.into()];
        device
    }

    impl BrowseUi {
        pub(crate) fn with_files(files: Arc<FakeFiles>) -> Self {
            Self::over(files)
        }
    }
}
