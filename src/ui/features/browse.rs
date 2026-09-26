//! Browse's UI: the *Browse files* action and the file browser, over the
//! shared [`BrowsePlugin`]'s methods.
//!
//! The page is [`Route::Browse`], which names the open folder, so Back, the
//! drop target and the shell all see which folder shows. A device's files
//! have no events: nothing tells the daemon when they change on the device
//! (ADR 0008). A folder is listed when it opens, again after every change
//! made here, on Refresh, and when the device can share its files again
//! after it couldn't (it reconnected).

use std::{cmp::Ordering, collections::HashMap, ops::Range, path::PathBuf, sync::Arc};

use futures_util::future::BoxFuture;
use iced::{
    Alignment, Background, Border, Element, Length, Padding, Subscription, Task, Theme, keyboard,
    widget::{Space, button, column, container, image, responsive, row, rule, scrollable, text},
};
use iced_fonts::lucide;
use tokio::io::AsyncReadExt;

use super::{DeviceAction, DropTarget, Feature};
use crate::{
    core::{CoreEvent, DeviceReachability, DeviceSnapshot, PluginContext, TransferSnapshot},
    plugins::browse::{
        BrowseError, BrowsePlugin, DirectoryListing, FileEntry, FileKind, REQUEST_PACKET_TYPE,
        UploadPathError,
        files::{join_remote_path, split_remote_path},
    },
    ui::{
        self, Origin,
        context::UiContext,
        error::{describe_code, describe_file_failures},
        overlay::dialog,
        route::Route,
        shell,
        widgets::{self, HeaderAction, Icon, bold, format_bytes, format_timestamp},
    },
};

/// Images opened as a preview rather than downloaded, up to
/// [`MAX_PREVIEW_BYTES`].
const PREVIEW_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "webp", "bmp"];
const MAX_PREVIEW_BYTES: u64 = 32 * 1024 * 1024;

/// From this width the listing shows when files were modified.
const WIDE: f32 = 600.0;
const SIZE_WIDTH: f32 = 96.0;
const MODIFIED_WIDTH: f32 = 168.0;
const ICON_WIDTH: f32 = 32.0;
const MORE_WIDTH: f32 = 40.0;

/// Where the browser is: a device, and a folder of it or its storage.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    device_id: String,
    folder: Option<String>,
}

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

pub struct BrowseUi {
    files: Arc<dyn Files>,
    /// Where the window is, while it shows the browser's page.
    open: Option<Place>,
    /// Whether the open device could share its files when last seen.
    shares: bool,
    /// The open device's folders as last listed (`None`: its storage).
    listings: HashMap<Option<String>, Listing>,
    /// Counts listing requests, so only a folder's newest answer counts.
    requests: u64,
    show_hidden: bool,
    sort: Sort,
    /// The file whose actions show under its row, by path.
    menu: Option<String>,
    preview: Option<Preview>,
}

#[derive(Default)]
struct Listing {
    /// The request whose answer is awaited or held.
    request: u64,
    /// The last answer; kept on show while a new one is on its way.
    answer: Option<Result<DirectoryListing, String>>,
}

/// A column of the listing to sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Column {
    #[default]
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sort {
    by: Column,
    ascending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            by: Column::Name,
            ascending: true,
        }
    }
}

/// An image from the device, shown over the page.
struct Preview {
    file: FileEntry,
    /// The decoded image, or why it can't be shown; `None` while loading.
    image: Option<Result<image::Handle, String>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Open a device's files, at its storage.
    Browse {
        device_id: String,
    },
    /// Back to the device.
    Back,
    /// Show a folder of the open device, or its storage (`None`).
    Open(Option<String>),
    /// Up a folder; from a storage root, to the storage.
    Up,
    Listed {
        device_id: String,
        folder: Option<String>,
        request: u64,
        result: Result<DirectoryListing, String>,
    },
    Refresh,
    ToggleHidden,
    Sort(Column),
    /// Show or hide a file's actions.
    Menu(String),
    /// A click on a file: a folder opens, a small image previews, anything
    /// else downloads.
    Activate(FileEntry),
    Download(FileEntry),
    /// A download started, of the file of this name, or why it didn't.
    Downloading(Result<String, String>),
    Preview(FileEntry),
    Previewed {
        path: String,
        result: Result<image::Handle, String>,
    },
    ClosePreview,
    NewFolder,
    Rename(FileEntry),
    Delete(FileEntry),
    /// Change something in `folder` on the device, then list it again.
    Change {
        device_id: String,
        folder: String,
        change: Change,
    },
    /// A change or upload in `folder` finished, or failed with this
    /// sentence.
    Changed {
        device_id: String,
        folder: String,
        error: Option<String>,
    },
    /// Pick files to upload to the open folder.
    PickUploads,
    /// Upload these files into `folder`, one at a time.
    Upload {
        device_id: String,
        folder: String,
        paths: Vec<PathBuf>,
    },
}

/// A change to a device's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    CreateDirectory(String),
    Move { from: String, to: String },
    Delete(String),
}

/// Listed for every device, enabled while it shares its files.
pub fn device_actions(device: &DeviceSnapshot) -> Vec<DeviceAction> {
    vec![DeviceAction {
        id: "browse-files",
        label: "Browse files".into(),
        icon: lucide::folder_open,
        enabled: shares_files(device),
        // The tray lists it only for a device that can share files.
        visible_in_tray: advertises_browse(device),
        message: Feature::Browse(Message::Browse {
            device_id: device.device_id.clone(),
        }),
    }]
}

/// `message` as the app's, from `origin`.
fn to_app(message: Message, origin: Origin) -> ui::Message {
    ui::Message::Feature(Feature::Browse(message), origin)
}

impl BrowseUi {
    /// The browser over `plugin`, the instance the core runs.
    pub fn new(plugin: Arc<BrowsePlugin>) -> Self {
        Self::over(plugin)
    }

    fn over(files: Arc<dyn Files>) -> Self {
        Self {
            files,
            open: None,
            shares: false,
            listings: HashMap::new(),
            requests: 0,
            show_hidden: false,
            sort: Sort::default(),
            menu: None,
            preview: None,
        }
    }

    /// List `folder` of the open device again, keeping what it held on show
    /// until the answer comes.
    fn fetch(
        &mut self,
        ctx: &UiContext,
        folder: Option<String>,
        origin: Origin,
    ) -> Task<ui::Message> {
        let Some(open) = &self.open else {
            return Task::none();
        };
        self.requests += 1;
        let request = self.requests;
        self.listings.entry(folder.clone()).or_default().request = request;
        let device_id = open.device_id.clone();
        let listed =
            self.files
                .clone()
                .list(ctx.plugin_context(), device_id.clone(), folder.clone());
        ctx.spawn(
            async move { listed.await.map_err(|error| describe_error(&error)) },
            move |result| {
                to_app(
                    Message::Listed {
                        device_id,
                        folder,
                        request,
                        result,
                    },
                    origin,
                )
            },
        )
    }

    /// List the open folder, and the storage too when it isn't held, for
    /// the breadcrumbs.
    fn fetch_open(&mut self, ctx: &UiContext) -> Task<ui::Message> {
        let Some(folder) = self.open.as_ref().map(|open| open.folder.clone()) else {
            return Task::none();
        };
        let roots_held = matches!(
            self.listings.get(&None),
            Some(Listing {
                answer: Some(Ok(_)),
                ..
            })
        );
        let mut tasks = vec![self.fetch(ctx, folder.clone(), Origin::Window)];
        if folder.is_some() && !roots_held {
            tasks.push(self.fetch(ctx, None, Origin::Window));
        }
        Task::batch(tasks)
    }

    fn open_device(&self) -> Option<String> {
        self.open.as_ref().map(|open| open.device_id.clone())
    }

    fn open_folder(&self) -> Option<&str> {
        self.open.as_ref()?.folder.as_deref()
    }

    fn roots(&self) -> &[FileEntry] {
        match self.listings.get(&None) {
            Some(Listing {
                answer: Some(Ok(listing)),
                ..
            }) => &listing.entries,
            _ => &[],
        }
    }

    fn navigate(&self, folder: Option<&str>, origin: Origin) -> Task<ui::Message> {
        match self.open_device() {
            Some(device) => shell::navigate(
                origin,
                Route::Browse {
                    device,
                    folder: folder.map(str::to_owned),
                },
            ),
            None => Task::none(),
        }
    }

    /// Ask for a name, then make `change` of it.
    fn prompt(
        &self,
        title: &str,
        confirm_label: &str,
        initial: &str,
        folder: String,
        origin: Origin,
        change: impl Fn(String) -> Option<Change> + Send + Sync + 'static,
    ) -> Task<ui::Message> {
        let Some(device_id) = self.open_device() else {
            return Task::none();
        };
        // Select the name without its extension, as file managers do.
        let stem = match initial.rfind('.') {
            Some(dot) if dot > 0 => initial[..dot].chars().count(),
            _ => initial.chars().count(),
        };
        shell::prompt(shell::Prompt {
            title: title.into(),
            label: "Name".into(),
            initial: initial.into(),
            selection: (!initial.is_empty()).then_some(Range {
                start: 0,
                end: stem,
            }),
            confirm_label: confirm_label.into(),
            validate: Arc::new(|name| invalid_name_reason(name).map(str::to_owned)),
            then: Arc::new(move |name| {
                Feature::Browse(match change(name) {
                    Some(change) => Message::Change {
                        device_id: device_id.clone(),
                        folder: folder.clone(),
                        change,
                    },
                    // Nothing to do; refetching is harmless.
                    None => Message::Changed {
                        device_id: device_id.clone(),
                        folder: folder.clone(),
                        error: None,
                    },
                })
            }),
            origin,
        })
    }

    fn change(
        &self,
        ctx: &UiContext,
        device_id: String,
        folder: String,
        change: Change,
        origin: Origin,
    ) -> Task<ui::Message> {
        let files = self.files.clone();
        let plugin_ctx = ctx.plugin_context();
        let changed = {
            let device_id = device_id.clone();
            async move {
                let result = match change {
                    Change::CreateDirectory(path) => files
                        .create_directory(plugin_ctx, device_id, path)
                        .await
                        .map(drop),
                    Change::Move { from, to } => files
                        .move_file(plugin_ctx, device_id, from, to)
                        .await
                        .map(drop),
                    Change::Delete(path) => files.delete(plugin_ctx, device_id, path).await,
                };
                result.err().map(|error| describe_error(&error))
            }
        };
        ctx.spawn(changed, move |error| {
            to_app(
                Message::Changed {
                    device_id,
                    folder,
                    error,
                },
                origin,
            )
        })
    }

    fn download(&self, ctx: &UiContext, file: FileEntry, origin: Origin) -> Task<ui::Message> {
        let Some(device_id) = self.open_device() else {
            return Task::none();
        };
        let started =
            self.files
                .clone()
                .download(ctx.plugin_context(), device_id, file.path.clone());
        ctx.spawn(
            async move {
                started
                    .await
                    .map(|_| file.name)
                    .map_err(|error| describe_error(&error))
            },
            move |result| to_app(Message::Downloading(result), origin),
        )
    }

    fn preview(&mut self, ctx: &UiContext, file: FileEntry, origin: Origin) -> Task<ui::Message> {
        let Some(device_id) = self.open_device() else {
            return Task::none();
        };
        let path = file.path.clone();
        let read = self.files.clone().read(
            ctx.plugin_context(),
            device_id,
            path.clone(),
            MAX_PREVIEW_BYTES,
        );
        self.preview = Some(Preview { file, image: None });
        ctx.spawn(
            async move {
                let bytes = read.await.map_err(|error| describe_error(&error))?;
                tokio::task::spawn_blocking(move || decode(&bytes))
                    .await
                    .unwrap_or_else(|_| Err(CANT_SHOW.into()))
            },
            move |result| to_app(Message::Previewed { path, result }, origin),
        )
    }

    fn upload(
        &self,
        ctx: &UiContext,
        device_id: String,
        folder: String,
        paths: Vec<PathBuf>,
        origin: Origin,
    ) -> Task<ui::Message> {
        let files = self.files.clone();
        let plugin_ctx = ctx.plugin_context();
        let uploaded = {
            let device_id = device_id.clone();
            let folder = folder.clone();
            async move {
                let mut failures = Vec::new();
                for path in paths {
                    let upload = files.clone().upload(
                        plugin_ctx.clone(),
                        device_id.clone(),
                        folder.clone(),
                        path.clone(),
                    );
                    if let Err(error) = upload.await {
                        failures.push((path, describe_upload_error(&error)));
                    }
                }
                describe_file_failures("upload", &failures)
            }
        };
        ctx.spawn(uploaded, move |error| {
            to_app(
                Message::Changed {
                    device_id,
                    folder,
                    error,
                },
                origin,
            )
        })
    }

    /// Files dropped on an open folder are uploaded into it.
    pub fn drop_target(&self, device: &DeviceSnapshot, route: &Route) -> Option<DropTarget> {
        let Route::Browse {
            device: device_id,
            folder: Some(folder),
        } = route
        else {
            return None;
        };
        if *device_id != device.device_id || !shares_files(device) {
            return None;
        }
        let device_id = device_id.clone();
        let folder = folder.clone();
        Some(DropTarget {
            label: format!("Drop to upload to {}", folder_name(&folder, self.roots())),
            on_drop: Arc::new(move |paths| {
                Feature::Browse(Message::Upload {
                    device_id: device_id.clone(),
                    folder: folder.clone(),
                    paths,
                })
            }),
        })
    }

    /// The window now shows `route`. The browser loads what its page needs
    /// when the route is its own, and lets go of it otherwise.
    pub(crate) fn on_route(&mut self, ctx: &UiContext, route: &Route) -> Task<ui::Message> {
        let Route::Browse { device, folder } = route else {
            // Let go of the device's files, as the Flutter page did.
            self.open = None;
            self.listings.clear();
            self.menu = None;
            self.preview = None;
            return Task::none();
        };
        if self.open_device().as_deref() != Some(device.as_str()) {
            self.listings.clear();
            self.show_hidden = false;
            self.sort = Sort::default();
            self.preview = None;
        }
        self.menu = None;
        self.shares = ctx.device(device).is_some_and(shares_files);
        self.open = Some(Place {
            device_id: device.clone(),
            folder: folder.clone(),
        });
        if self.shares {
            self.fetch_open(ctx)
        } else {
            Task::none()
        }
    }

    /// When the open device can share its files again (it reconnected),
    /// list them again.
    pub(crate) fn on_event(&mut self, ctx: &UiContext, _event: &CoreEvent) -> Task<ui::Message> {
        let Some(device_id) = self.open_device() else {
            return Task::none();
        };
        let shares = ctx.device(&device_id).is_some_and(shares_files);
        if shares == self.shares {
            return Task::none();
        }
        self.shares = shares;
        if !shares {
            return Task::none();
        }
        let folder = self.open_folder().map(str::to_owned);
        let mut tasks = vec![self.fetch(ctx, None, Origin::Window)];
        if folder.is_some() {
            tasks.push(self.fetch(ctx, folder, Origin::Window));
        }
        Task::batch(tasks)
    }

    pub(crate) fn update(
        &mut self,
        ctx: &UiContext,
        message: Message,
        origin: Origin,
    ) -> Task<ui::Message> {
        match message {
            Message::Browse { device_id } => shell::navigate(
                origin,
                Route::Browse {
                    device: device_id,
                    folder: None,
                },
            ),
            Message::Back => match self.open_device() {
                Some(device_id) => shell::navigate(origin, Route::Device(device_id)),
                None => Task::none(),
            },
            Message::Open(folder) => self.navigate(folder.as_deref(), origin),
            Message::Up => {
                let Some(folder) = self.open_folder() else {
                    return Task::none();
                };
                let up = if self.roots().iter().any(|root| root.path == folder) {
                    None
                } else {
                    parent_of(folder)
                };
                self.navigate(up.as_deref(), origin)
            }
            Message::Listed {
                device_id,
                folder,
                request,
                result,
            } => {
                if self.open_device().as_deref() == Some(device_id.as_str())
                    && let Some(listing) = self.listings.get_mut(&folder)
                    && listing.request == request
                {
                    listing.answer = Some(result);
                }
                Task::none()
            }
            Message::Refresh => {
                let folder = self.open_folder().map(str::to_owned);
                self.fetch(ctx, folder, origin)
            }
            Message::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                Task::none()
            }
            Message::Sort(by) => {
                self.sort = Sort {
                    by,
                    ascending: self.sort.by != by || !self.sort.ascending,
                };
                Task::none()
            }
            Message::Menu(path) => {
                self.menu = (self.menu.as_ref() != Some(&path)).then_some(path);
                Task::none()
            }
            Message::Activate(file) => {
                self.menu = None;
                if file.kind == FileKind::Directory {
                    self.navigate(Some(&file.path), origin)
                } else if can_preview(&file) {
                    self.preview(ctx, file, origin)
                } else {
                    self.download(ctx, file, origin)
                }
            }
            Message::Download(file) => {
                self.menu = None;
                self.download(ctx, file, origin)
            }
            Message::Downloading(Ok(name)) => Task::done(ui::Message::Toast {
                text: format!("Downloading {name}"),
                action: Some(("Transfers".into(), Route::Transfers)),
                origin,
            }),
            Message::Downloading(Err(error)) => shell::toast(origin, error),
            Message::Preview(file) => {
                self.menu = None;
                self.preview(ctx, file, origin)
            }
            Message::Previewed { path, result } => {
                if let Some(preview) = &mut self.preview
                    && preview.file.path == path
                {
                    preview.image = Some(result);
                }
                Task::none()
            }
            Message::ClosePreview => {
                self.preview = None;
                Task::none()
            }
            Message::NewFolder => {
                let Some(folder) = self.open_folder().map(str::to_owned) else {
                    return Task::none();
                };
                let parent = folder.clone();
                self.prompt("New folder", "Create", "", folder, origin, move |name| {
                    Some(Change::CreateDirectory(join_remote_path(&parent, &name)))
                })
            }
            Message::Rename(file) => {
                self.menu = None;
                let Some(parent) = parent_of(&file.path) else {
                    return Task::none();
                };
                let folder = parent.clone();
                let initial = file.name.clone();
                self.prompt("Rename", "Rename", &initial, folder, origin, move |name| {
                    (name != file.name).then(|| Change::Move {
                        from: file.path.clone(),
                        to: join_remote_path(&parent, &name),
                    })
                })
            }
            Message::Delete(file) => {
                self.menu = None;
                let (Some(device_id), Some(folder)) = (self.open_device(), parent_of(&file.path))
                else {
                    return Task::none();
                };
                let body = if file.kind == FileKind::Directory {
                    "The folder and everything in it will be deleted from the device. This \
                     can’t be undone."
                } else {
                    "The file will be deleted from the device. This can’t be undone."
                };
                shell::confirm(
                    origin,
                    format!("Delete {}?", file.name),
                    body,
                    "Delete",
                    Feature::Browse(Message::Change {
                        device_id,
                        folder,
                        change: Change::Delete(file.path),
                    }),
                )
            }
            Message::Change {
                device_id,
                folder,
                change,
            } => self.change(ctx, device_id, folder, change, origin),
            Message::Changed {
                device_id,
                folder,
                error,
            } => {
                // The folder shows what the change did, whether it worked or
                // not, if it is still held.
                let folder = Some(folder);
                let refetch = if self.open_device().as_deref() == Some(device_id.as_str())
                    && self.listings.contains_key(&folder)
                {
                    self.fetch(ctx, folder, origin)
                } else {
                    Task::none()
                };
                match error {
                    Some(error) => Task::batch([refetch, shell::toast(origin, error)]),
                    None => refetch,
                }
            }
            Message::PickUploads => {
                let (Some(device_id), Some(folder)) =
                    (self.open_device(), self.open_folder().map(str::to_owned))
                else {
                    return Task::none();
                };
                shell::pick_files(
                    origin,
                    format!("Upload files to {}", folder_name(&folder, self.roots())),
                    Arc::new(move |paths| {
                        Feature::Browse(Message::Upload {
                            device_id: device_id.clone(),
                            folder: folder.clone(),
                            paths,
                        })
                    }),
                )
            }
            Message::Upload {
                device_id,
                folder,
                paths,
            } => self.upload(ctx, device_id, folder, paths, origin),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if self.preview.is_none() {
            return Subscription::none();
        }
        keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            } => Some(Message::ClosePreview),
            _ => None,
        })
    }

    /// The page of `device`'s files showing `folder`, or its storage, with
    /// the image preview over it.
    pub fn view<'a>(
        &'a self,
        device: &'a DeviceSnapshot,
        folder: Option<String>,
    ) -> Element<'a, Message> {
        let page = self.page(device, folder);
        match &self.preview {
            Some(preview) => {
                dialog::modal(page, preview_view(preview), Some(Message::ClosePreview))
            }
            None => page,
        }
    }

    fn page<'a>(
        &'a self,
        device: &'a DeviceSnapshot,
        folder: Option<String>,
    ) -> Element<'a, Message> {
        let available = shares_files(device);
        let in_folder = folder.is_some();
        let when = |enabled: bool, message: Message| enabled.then_some(message);
        let actions = vec![
            HeaderAction {
                icon: lucide::file_up,
                tooltip: "Upload files".into(),
                on_press: when(available && in_folder, Message::PickUploads),
            },
            HeaderAction {
                icon: lucide::folder_plus,
                tooltip: "New folder".into(),
                on_press: when(available && in_folder, Message::NewFolder),
            },
            HeaderAction {
                icon: lucide::refresh_cw,
                tooltip: "Refresh".into(),
                on_press: when(available, Message::Refresh),
            },
            if self.show_hidden {
                HeaderAction::new(lucide::eye_off, "Hide hidden files", Message::ToggleHidden)
            } else {
                HeaderAction::new(lucide::eye, "Show hidden files", Message::ToggleHidden)
            },
        ];
        let header = widgets::page_header(
            format!("Files on {}", device.device_name),
            Some(Message::Back),
            actions,
        );
        let body: Element<'a, Message> = if !available {
            let reason = if device.reachability == DeviceReachability::Connected {
                format!("{} doesn’t share its files.", device.device_name)
            } else {
                format!("Connect {} to browse its files.", device.device_name)
            };
            widgets::error_view(reason, None)
        } else {
            column![
                breadcrumbs(folder.as_deref(), self.roots()),
                rule::horizontal(1),
                self.listing(folder),
            ]
            .spacing(4)
            .into()
        };
        widgets::page(header, body)
    }

    fn listing<'a>(&'a self, folder: Option<String>) -> Element<'a, Message> {
        let in_folder = folder.is_some();
        let answer = self
            .listings
            .get(&folder)
            .and_then(|listing| listing.answer.as_ref());
        let listing = match answer {
            None if in_folder => return widgets::loading("Loading the folder…"),
            None => return widgets::loading("Connecting to the device…"),
            Some(Err(error)) => return widgets::error_view(error.as_str(), Some(Message::Refresh)),
            Some(Ok(listing)) => listing,
        };
        let entries: Vec<&FileEntry> = listing
            .entries
            .iter()
            .filter(|entry| self.show_hidden || !entry.name.starts_with('.'))
            .collect();
        if !in_folder {
            if entries.is_empty() {
                return widgets::empty_state(
                    lucide::hard_drive,
                    "The device isn’t sharing any storage.",
                    None,
                    None,
                );
            }
            let rows = entries.into_iter().map(|root| {
                button(
                    row![
                        lucide::hard_drive().size(20).style(text::secondary),
                        column![
                            text(&root.name),
                            text(&root.path).size(13).style(text::secondary),
                        ]
                        .spacing(2),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center),
                )
                .padding([12, 14])
                .width(Length::Fill)
                .style(widgets::card_button)
                .on_press(Message::Open(Some(root.path.clone())))
                .into()
            });
            return scrollable(column(rows).spacing(8).padding([8, 0]))
                .spacing(6)
                .height(Length::Fill)
                .into();
        }
        let sort = self.sort;
        let mut entries = entries;
        entries.sort_by(|a, b| compare(a, b, sort));
        responsive(move |size| {
            let wide = size.width >= WIDE;
            let list: Element<'a, Message> = if entries.is_empty() {
                widgets::empty_state(
                    lucide::folder_open,
                    "This folder is empty. Drop files here to upload them.",
                    None,
                    None,
                )
            } else {
                scrollable(column(entries.iter().map(|file| self.file_row(file, wide))).spacing(2))
                    .spacing(6)
                    .height(Length::Fill)
                    .into()
            };
            column![header_row(wide, sort), rule::horizontal(1), list]
                .spacing(2)
                .into()
        })
        .into()
    }

    fn file_row<'a>(&'a self, file: &'a FileEntry, wide: bool) -> Element<'a, Message> {
        let mut line = row![
            container(file_icon(file)().size(18).style(text::secondary)).width(ICON_WIDTH),
            container(text(&file.name).wrapping(text::Wrapping::None))
                .width(Length::Fill)
                .clip(true),
            text(file.size.map(format_bytes).unwrap_or_default())
                .size(13)
                .style(text::secondary)
                .width(SIZE_WIDTH)
                .align_x(Alignment::End),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if wide {
            line = line.push(
                text(file.modified_at.map(format_timestamp).unwrap_or_default())
                    .size(13)
                    .style(text::secondary)
                    .width(MODIFIED_WIDTH)
                    .align_x(Alignment::End),
            );
        }
        line = line.push(
            container(widgets::icon_button(
                lucide::ellipsis_vertical,
                "More",
                Some(Message::Menu(file.path.clone())),
            ))
            .width(MORE_WIDTH)
            .align_x(Alignment::End),
        );
        let row_button = button(line)
            .padding([2, 8])
            .width(Length::Fill)
            .style(row_style)
            .on_press(Message::Activate(file.clone()));
        if self.menu.as_deref() != Some(file.path.as_str()) {
            return row_button.into();
        }

        let mut actions = row![].spacing(8);
        if can_preview(file) {
            actions = actions.push(menu_button(
                lucide::eye,
                "Preview",
                Message::Preview(file.clone()),
            ));
        }
        if file.kind != FileKind::Directory {
            actions = actions.push(menu_button(
                lucide::download,
                "Download",
                Message::Download(file.clone()),
            ));
        }
        actions = actions
            .push(menu_button(
                lucide::pencil,
                "Rename",
                Message::Rename(file.clone()),
            ))
            .push(menu_button(
                lucide::trash_two,
                "Delete",
                Message::Delete(file.clone()),
            ));
        column![
            row_button,
            container(actions.wrap().vertical_spacing(8)).padding(Padding {
                // Under the name, past the row's icon.
                left: 8.0 + ICON_WIDTH + 8.0,
                ..Padding::from([4, 8])
            })
        ]
        .spacing(2)
        .into()
    }
}

/// The page's path from the storage: "Storage", the root as the device
/// names it, then each folder, each a link back up, and Up before them.
fn breadcrumbs<'a>(folder: Option<&str>, roots: &[FileEntry]) -> Element<'a, Message> {
    let crumbs = crumbs(folder, roots);
    let last = crumbs.len() - 1;
    let mut trail = row![].spacing(2).align_y(Alignment::Center);
    for (index, (label, target)) in crumbs.into_iter().enumerate() {
        if index > 0 {
            trail = trail.push(lucide::chevron_right().size(14).style(text::secondary));
        }
        trail = trail.push(if index == last {
            container(text(label).font(bold()).wrapping(text::Wrapping::None))
                .padding([6, 12])
                .into()
        } else {
            widgets::link_button(label, Message::Open(target))
        });
    }
    row![
        widgets::icon_button(lucide::arrow_up, "Up", folder.map(|_| Message::Up)),
        // Deep paths scroll, showing their end.
        scrollable(trail)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(3).scroller_width(3),
            ))
            .anchor_right()
            .width(Length::Fill),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// Each breadcrumb's label and the folder it opens (`None`: the storage).
fn crumbs(folder: Option<&str>, roots: &[FileEntry]) -> Vec<(String, Option<String>)> {
    let mut crumbs = vec![("Storage".to_owned(), None)];
    let Some(folder) = folder else {
        return crumbs;
    };
    let root = root_of(folder, roots);
    let mut current = root.map(|root| root.path.clone()).unwrap_or_default();
    if let Some(root) = root {
        crumbs.push((root.name.clone(), Some(root.path.clone())));
    }
    let rest = match root {
        Some(root) => &folder[root.path.len()..],
        None => folder,
    };
    for segment in rest.split('/').filter(|segment| !segment.is_empty()) {
        current = join_remote_path(if current.is_empty() { "/" } else { &current }, segment);
        crumbs.push((segment.to_owned(), Some(current.clone())));
    }
    crumbs
}

/// The folder's name as the breadcrumbs end with it.
fn folder_name(folder: &str, roots: &[FileEntry]) -> String {
    crumbs(Some(folder), roots)
        .pop()
        .map(|(label, _)| label)
        .unwrap_or_default()
}

fn header_row<'a>(wide: bool, sort: Sort) -> Element<'a, Message> {
    let column_button = |label: &'static str, by: Column| -> Element<'a, Message> {
        let mut content = row![text(label).size(13).font(bold())]
            .spacing(4)
            .align_y(Alignment::Center);
        if sort.by == by {
            let arrow = if sort.ascending {
                lucide::arrow_up()
            } else {
                lucide::arrow_down()
            };
            content = content.push(arrow.size(13));
        }
        button(content)
            .padding([6, 0])
            .style(row_style)
            .on_press(Message::Sort(by))
            .into()
    };
    let mut header = row![
        Space::new().width(ICON_WIDTH),
        container(column_button("Name", Column::Name)).width(Length::Fill),
        container(column_button("Size", Column::Size))
            .width(SIZE_WIDTH)
            .align_x(Alignment::End),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if wide {
        header = header.push(
            container(column_button("Modified", Column::Modified))
                .width(MODIFIED_WIDTH)
                .align_x(Alignment::End),
        );
    }
    container(header.push(Space::new().width(MORE_WIDTH)))
        .padding([0, 8])
        .into()
}

/// A plain row that shows hover and press.
fn row_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let background = match status {
        button::Status::Hovered => Some(palette.background.weak.color),
        button::Status::Pressed => Some(palette.background.strong.color),
        _ => None,
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: palette.background.base.text,
        border: Border::default().rounded(8),
        ..button::Style::default()
    }
}

fn menu_button<'a>(icon: Icon, label: &'a str, message: Message) -> Element<'a, Message> {
    button(
        row![icon().size(14), text(label).size(13)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .style(widgets::tonal)
    .on_press(message)
    .into()
}

fn preview_view(preview: &Preview) -> Element<'_, Message> {
    let body: Element<'_, Message> = match &preview.image {
        None => widgets::loading("Loading the image…"),
        Some(Err(error)) => widgets::error_view(error.as_str(), None),
        Some(Ok(handle)) => image::viewer(handle.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    };
    let card = container(
        column![
            row![
                container(
                    text(&preview.file.name)
                        .size(18)
                        .font(bold())
                        .wrapping(text::Wrapping::None)
                )
                .width(Length::Fill)
                .clip(true),
                widgets::icon_button(lucide::x, "Close", Some(Message::ClosePreview)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            container(body).height(Length::Fill),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill)
    .max_width(960)
    .max_height(720)
    .style(dialog::surface_style);
    container(card).padding(32).into()
}

/// What an image that can't be decoded says.
const CANT_SHOW: &str = "This image can’t be shown.";

/// Decode an image for the preview. iced would decode it only when drawn,
/// and drop the error.
fn decode(bytes: &[u8]) -> Result<image::Handle, String> {
    let decoded = ::image::load_from_memory(bytes).map_err(|_| CANT_SHOW.to_owned())?;
    let rgba = decoded.into_rgba8();
    Ok(image::Handle::from_rgba(
        rgba.width(),
        rgba.height(),
        rgba.into_raw(),
    ))
}

/// Folders first, then by the chosen column, then by name.
fn compare(a: &FileEntry, b: &FileEntry, sort: Sort) -> Ordering {
    let (a_folder, b_folder) = (a.kind == FileKind::Directory, b.kind == FileKind::Directory);
    if a_folder != b_folder {
        return b_folder.cmp(&a_folder);
    }
    let by_name = a.name.to_lowercase().cmp(&b.name.to_lowercase());
    let order = match sort.by {
        Column::Name => by_name,
        Column::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
        Column::Modified => a.modified_at.unwrap_or(0).cmp(&b.modified_at.unwrap_or(0)),
    }
    .then(by_name);
    if sort.ascending {
        order
    } else {
        order.reverse()
    }
}

/// Whether a click on `file` previews it rather than downloading it.
fn can_preview(file: &FileEntry) -> bool {
    file.kind != FileKind::Directory
        && extension(&file.name)
            .is_some_and(|extension| PREVIEW_EXTENSIONS.contains(&extension.as_str()))
        && file.size.unwrap_or(0) <= MAX_PREVIEW_BYTES
}

/// A file's extension, lowercase; a leading dot doesn't start one.
fn extension(name: &str) -> Option<String> {
    match name.rfind('.') {
        Some(dot) if dot > 0 => Some(name[dot + 1..].to_lowercase()),
        _ => None,
    }
}

fn file_icon(file: &FileEntry) -> Icon {
    if file.kind == FileKind::Directory {
        return lucide::folder;
    }
    match extension(&file.name).as_deref().unwrap_or("") {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" => lucide::image,
        "mp4" | "mkv" | "mov" | "webm" | "3gp" => lucide::film,
        "mp3" | "m4a" | "ogg" | "opus" | "flac" | "wav" => lucide::music,
        "pdf" => lucide::file_text,
        "zip" | "tar" | "gz" | "7z" | "rar" => lucide::file_archive,
        "apk" => lucide::package,
        _ => lucide::file,
    }
}

/// The storage root that `path` is in, among `roots`.
fn root_of<'a>(path: &str, roots: &'a [FileEntry]) -> Option<&'a FileEntry> {
    roots.iter().find(|root| {
        path == root.path
            || path
                .strip_prefix(&root.path)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The folder holding `path`; `/` has none.
fn parent_of(path: &str) -> Option<String> {
    split_remote_path(path).map(|(parent, _)| parent.to_owned())
}

/// Why `name` can't name a file, checked before asking the device.
fn invalid_name_reason(name: &str) -> Option<&'static str> {
    if name.trim().is_empty() {
        Some("Enter a name.")
    } else if name == "." || name == ".." {
        Some("That name is reserved.")
    } else if name.contains('/') {
        Some("Names can’t contain “/”.")
    } else {
        None
    }
}

/// Whether the device's files can be browsed now.
fn shares_files(device: &DeviceSnapshot) -> bool {
    device.paired
        && device.reachability == DeviceReachability::Connected
        && advertises_browse(device)
}

/// Whether the device says it can share its files at all.
fn advertises_browse(device: &DeviceSnapshot) -> bool {
    device
        .incoming_capabilities
        .iter()
        .any(|capability| capability == REQUEST_PACKET_TYPE)
}

/// A sentence for the user about why an upload didn't start.
fn describe_upload_error(error: &UploadPathError) -> String {
    match error {
        UploadPathError::Browse(error) => describe_error(error),
        UploadPathError::File(_) => "The file couldn’t be read.".into(),
    }
}

/// A sentence for the user about `error`.
pub fn describe_error(error: &BrowseError) -> String {
    let reason = match error {
        BrowseError::Unavailable { reason } => reason.as_deref(),
        _ => None,
    };
    describe(error.code(), reason)
}

/// Words this feature's codes, with the peer's `reason` where it gives one,
/// and leaves the rest to `ui::error`.
fn describe(code: &str, reason: Option<&str>) -> String {
    let message = match code {
        "files_unavailable" => {
            let reason = reason
                .map(|reason| format!(" ({reason})"))
                .unwrap_or_default();
            return format!(
                "The device isn’t sharing its files{reason}. In KDE Connect on the device, \
                 allow access to files in the Filesystem expose plugin."
            );
        }
        "file_not_found" => "That file or folder no longer exists.",
        "file_exists" => "There is already a file or folder with that name.",
        "file_permission_denied" => "The device doesn’t allow that.",
        "not_a_directory" => "That isn’t a folder.",
        "is_a_directory" => "Folders can’t be downloaded, only files.",
        "invalid_path" => "That name or location can’t be used.",
        "files_failed" => "The device’s files couldn’t be reached.",
        "files_timed_out" => "The device took too long to answer.",
        "files_host_key_mismatch" => {
            "The device’s file server didn’t prove it is the paired device, so MyConnect \
             didn’t connect to it."
        }
        code => return describe_code(code),
    };
    message.into()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        io,
        sync::{Mutex, PoisonError},
    };

    use chrono::TimeZone;
    use iced::widget;
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::{
        core::{CoreError, EventData, TransferDirection, TransferStatus, testing::handle},
        ui::{
            pages::transfers::tests::transfer,
            shell::{PickFiles, Prompt},
            testing,
        },
    };

    const INTERNAL: &str = "/storage/emulated/0";
    const SD_CARD: &str = "/storage/sdcard";

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
        listings: Mutex<HashMap<Option<String>, DirectoryListing>>,
        calls: Mutex<Vec<Call>>,
        /// What listing answers instead, if set.
        refuse: Mutex<Option<fn() -> BrowseError>>,
        /// What changes answer instead, if set.
        fail_changes: Mutex<Option<fn() -> BrowseError>>,
        /// Every file's content.
        content: Mutex<Vec<u8>>,
    }

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
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

        fn listed(&self, folder: Option<&str>) -> usize {
            let call = Call::List(folder.map(str::to_owned));
            self.calls().iter().filter(|made| **made == call).count()
        }
    }

    fn directory(path: &str, name: Option<&str>) -> FileEntry {
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

    fn file(path: &str, size: u64) -> FileEntry {
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

    /// The page of the test phone's files showing `folder`, or its storage.
    fn files_route(folder: Option<&str>) -> Route {
        Route::Browse {
            device: pixel().device_id,
            folder: folder.map(str::to_owned),
        }
    }

    /// The browse message `feature` holds.
    fn browse(feature: Feature) -> Message {
        let Feature::Browse(message) = feature else {
            panic!("not a browse message: {feature:?}");
        };
        message
    }

    /// The browser, with the shell's part played: navigation reaches
    /// `on_route`, and the other requests are kept to look at.
    struct Browser {
        ui: BrowseUi,
        ctx: UiContext,
        files: Arc<FakeFiles>,
        route: Route,
        requests: Vec<ui::Message>,
    }

    impl Browser {
        fn new() -> Self {
            Self::with_device(pixel())
        }

        fn with_device(device: DeviceSnapshot) -> Self {
            let (core, _commands) = handle();
            let mut ctx = UiContext::new(core, tokio::runtime::Handle::current());
            *ctx.store_mut() = testing::store("Desk", vec![device]);
            let files = phone_files();
            Self {
                ui: BrowseUi::with_files(files.clone()),
                ctx,
                files,
                route: Route::Devices,
                requests: Vec::new(),
            }
        }

        fn device(&self) -> &DeviceSnapshot {
            self.ctx.device(&pixel().device_id).expect("the phone")
        }

        /// Run `task`, its messages and what they lead to.
        async fn run(&mut self, task: Task<ui::Message>) {
            let mut pending = vec![task];
            while let Some(task) = pending.pop() {
                for message in testing::outputs(task).await {
                    match message {
                        ui::Message::Feature(Feature::Browse(message), origin) => {
                            pending.push(self.ui.update(&self.ctx, message, origin));
                        }
                        ui::Message::Navigate(route, _) => {
                            self.route = route.clone();
                            pending.push(self.ui.on_route(&self.ctx, &route));
                        }
                        request => self.requests.push(request),
                    }
                }
            }
        }

        async fn send(&mut self, message: Message) {
            let task = self.ui.update(&self.ctx, message, Origin::Window);
            self.run(task).await;
        }

        async fn go(&mut self, folder: Option<&str>) {
            self.route = files_route(folder);
            let route = self.route.clone();
            let task = self.ui.on_route(&self.ctx, &route);
            self.run(task).await;
        }

        fn page(&self) -> Element<'_, Message> {
            let Route::Browse { folder, .. } = &self.route else {
                panic!("not on a browse page: {:?}", self.route);
            };
            self.ui.view(self.device(), folder.clone())
        }

        fn shows(&self, text: &str) -> bool {
            Simulator::new(self.page()).find(text).is_ok()
        }

        fn top(&self, text: &str) -> f32 {
            let mut ui = Simulator::new(self.page());
            ui.find(text).unwrap().visible_bounds().unwrap().y
        }

        async fn click(
            &mut self,
            target: impl iced_test::selector::Selector<
                Output: iced_test::selector::Bounded + Clone + Send + Sync + 'static,
            > + Send,
        ) {
            let messages: Vec<Message> = {
                let mut ui = Simulator::new(self.page());
                ui.click(target).expect("the target is on the page");
                ui.into_messages().collect()
            };
            for message in messages {
                self.send(message).await;
            }
        }

        /// Open `name`'s actions and choose `action`.
        async fn row_action(&mut self, name: &str, action: &str) {
            let folder = self.ui.open_folder().unwrap().to_owned();
            self.send(Message::Menu(join_remote_path(&folder, name)))
                .await;
            self.click(action).await;
        }

        fn take_requests(&mut self) -> Vec<ui::Message> {
            std::mem::take(&mut self.requests)
        }
    }

    #[tokio::test]
    async fn storage_then_folders_with_a_breadcrumb_back_up() {
        let mut browser = Browser::new();
        browser
            .send(Message::Browse {
                device_id: pixel().device_id,
            })
            .await;
        assert!(browser.shows("Files on Pixel"));
        assert!(browser.shows("All files"));
        assert!(browser.shows("SD card"));

        browser.click("All files").await;
        assert_eq!(browser.route, files_route(Some(INTERNAL)));
        // Folders first; hidden files stay hidden.
        assert!(browser.top("DCIM") < browser.top("notes.txt"));
        assert!(browser.shows("2.0 KB"));
        assert!(browser.shows("2026-09-24 14:03"));
        assert!(!browser.shows(".nomedia"));

        browser.click("DCIM").await;
        browser.click("Camera").await;
        assert!(browser.shows("This folder is empty. Drop files here to upload them."));

        // The breadcrumb names the root as the device does.
        browser.click("All files").await;
        assert!(browser.shows("notes.txt"));
        browser.click(widget::Id::from("Up")).await;
        assert!(browser.shows("SD card"));
        assert_eq!(browser.route, files_route(None));

        browser.click(widget::Id::from("Back")).await;
        assert_eq!(browser.route, Route::Device(pixel().device_id));
        assert!(
            browser.ui.listings.is_empty(),
            "leaving lets go of the files"
        );
    }

    #[tokio::test]
    async fn a_folder_opened_directly_still_names_its_root() {
        let mut browser = Browser::new();
        browser.go(Some(&format!("{INTERNAL}/DCIM"))).await;
        assert_eq!(
            browser.files.calls(),
            [
                Call::List(Some(format!("{INTERNAL}/DCIM"))),
                Call::List(None)
            ]
        );
        assert!(browser.shows("All files"));
        assert!(browser.shows("Camera"));
        browser.click(widget::Id::from("Up")).await;
        assert!(browser.shows("notes.txt"));
        // From a root, Up goes to the storage.
        browser.click(widget::Id::from("Up")).await;
        assert!(browser.shows("SD card"));
    }

    #[test]
    fn crumbs_follow_the_root_then_each_folder() {
        let roots = [directory(INTERNAL, Some("All files"))];
        assert_eq!(
            crumbs(Some(&format!("{INTERNAL}/DCIM/Camera")), &roots),
            [
                ("Storage".to_owned(), None),
                ("All files".into(), Some(INTERNAL.into())),
                ("DCIM".into(), Some(format!("{INTERNAL}/DCIM"))),
                ("Camera".into(), Some(format!("{INTERNAL}/DCIM/Camera"))),
            ]
        );
        // Outside every root, the path's own folders.
        assert_eq!(
            crumbs(Some("/sdcard/Music"), &roots),
            [
                ("Storage".to_owned(), None),
                ("sdcard".into(), Some("/sdcard".into())),
                ("Music".into(), Some("/sdcard/Music".into())),
            ]
        );
        // A root's name isn't mistaken for a prefix of another folder.
        assert_eq!(
            folder_name(&format!("{INTERNAL}0/x"), &roots),
            "x",
            "{INTERNAL}0 isn't inside {INTERNAL}"
        );
    }

    #[tokio::test]
    async fn hidden_files_can_be_shown() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        assert!(!browser.shows(".nomedia"));
        browser.click(widget::Id::from("Show hidden files")).await;
        assert!(browser.shows(".nomedia"));
        browser.click(widget::Id::from("Hide hidden files")).await;
        assert!(!browser.shows(".nomedia"));
    }

    #[tokio::test]
    async fn columns_sort_both_ways_with_folders_first() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        let order = |browser: &Browser| {
            let mut names = ["DCIM", "notes.txt", "photo.png"];
            names.sort_by(|a, b| browser.top(a).total_cmp(&browser.top(b)));
            names
        };
        assert_eq!(order(&browser), ["DCIM", "notes.txt", "photo.png"]);
        browser.click("Size").await;
        assert_eq!(order(&browser), ["DCIM", "photo.png", "notes.txt"]);
        browser.click("Size").await;
        assert_eq!(order(&browser), ["DCIM", "notes.txt", "photo.png"]);
        browser.click("Name").await;
        browser.click("Name").await;
        assert_eq!(order(&browser), ["DCIM", "photo.png", "notes.txt"]);
    }

    #[tokio::test]
    async fn modified_shows_only_on_a_wide_window() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        let narrow = Simulator::with_size(Default::default(), (500.0, 600.0), browser.page())
            .find("2026-09-24 14:03")
            .is_ok();
        assert!(!narrow);
        assert!(browser.shows("Modified"), "the default size is wide");
    }

    #[tokio::test]
    async fn opening_a_file_downloads_it() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        browser.click("notes.txt").await;
        assert!(
            browser
                .files
                .calls()
                .contains(&Call::Download(format!("{INTERNAL}/notes.txt")))
        );
        let [ui::Message::Toast { text, action, .. }] = &browser.take_requests()[..] else {
            panic!("a toast");
        };
        assert_eq!(text, "Downloading notes.txt");
        assert_eq!(action, &Some(("Transfers".into(), Route::Transfers)));
    }

    #[tokio::test]
    async fn opening_a_small_image_previews_it() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        let mut png = std::io::Cursor::new(Vec::new());
        ::image::RgbaImage::new(4, 3)
            .write_to(&mut png, ::image::ImageFormat::Png)
            .unwrap();
        *lock(&browser.files.content) = png.into_inner();

        browser.click("photo.png").await;
        assert!(
            browser
                .files
                .calls()
                .contains(&Call::Read(format!("{INTERNAL}/photo.png")))
        );
        let preview = browser.ui.preview.as_ref().expect("a preview");
        assert!(matches!(preview.image, Some(Ok(_))), "decoded");
        browser.click(widget::Id::from("Close")).await;
        assert!(browser.ui.preview.is_none());

        // Not an image after all.
        *lock(&browser.files.content) = b"not a png".to_vec();
        browser.row_action("photo.png", "Preview").await;
        assert!(browser.shows(CANT_SHOW));
        // Large images download instead.
        let mut large = file(&format!("{INTERNAL}/huge.jpg"), MAX_PREVIEW_BYTES + 1);
        assert!(!can_preview(&large));
        large.size = Some(MAX_PREVIEW_BYTES);
        assert!(can_preview(&large));
        assert!(!can_preview(&directory("/x.png", None)));
        assert!(!can_preview(&file("/.png", 1)));
    }

    /// The prompt the browser asked the shell for.
    fn prompt(requests: Vec<ui::Message>) -> Prompt {
        let [ui::Message::Prompt(prompt)] = <[_; 1]>::try_from(requests).unwrap() else {
            panic!("a prompt");
        };
        *prompt
    }

    #[tokio::test]
    async fn renaming_moves_the_file_within_its_folder() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        browser.row_action("notes.txt", "Rename").await;
        let Prompt {
            title,
            initial,
            selection,
            confirm_label,
            then,
            ..
        } = prompt(browser.take_requests());
        assert_eq!(
            (title.as_str(), confirm_label.as_str()),
            ("Rename", "Rename")
        );
        assert_eq!(initial, "notes.txt");
        assert_eq!(selection, Some(0..5), "the name without its extension");

        browser.send(browse(then("todo.txt".into()))).await;
        assert!(browser.files.calls().contains(&Call::Move(
            format!("{INTERNAL}/notes.txt"),
            format!("{INTERNAL}/todo.txt")
        )));
        // The folder is listed again to show the change.
        assert_eq!(browser.files.listed(Some(INTERNAL)), 2);

        // The same name changes nothing.
        browser.row_action("notes.txt", "Rename").await;
        let Prompt { then, .. } = prompt(browser.take_requests());
        browser.send(browse(then("notes.txt".into()))).await;
        assert_eq!(
            browser
                .files
                .calls()
                .iter()
                .filter(|call| matches!(call, Call::Move(..)))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn a_name_with_a_slash_is_refused_before_asking_the_device() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        browser.click(widget::Id::from("New folder")).await;
        let Prompt {
            validate,
            initial,
            selection,
            ..
        } = prompt(browser.take_requests());
        assert_eq!((initial.as_str(), selection), ("", None));
        assert_eq!(validate("a/b").as_deref(), Some("Names can’t contain “/”."));
        assert_eq!(validate(" ").as_deref(), Some("Enter a name."));
        assert_eq!(validate("..").as_deref(), Some("That name is reserved."));
        assert_eq!(validate("Trip"), None);
        assert!(
            !browser
                .files
                .calls()
                .iter()
                .any(|call| matches!(call, Call::CreateDirectory(_)))
        );
    }

    #[tokio::test]
    async fn new_folders_are_created_in_the_open_folder() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        browser.click(widget::Id::from("New folder")).await;
        let Prompt {
            title,
            confirm_label,
            then,
            ..
        } = prompt(browser.take_requests());
        assert_eq!(
            (title.as_str(), confirm_label.as_str()),
            ("New folder", "Create")
        );
        browser.send(browse(then("Trip".into()))).await;
        assert!(
            browser
                .files
                .calls()
                .contains(&Call::CreateDirectory(format!("{INTERNAL}/Trip")))
        );
        assert_eq!(browser.files.listed(Some(INTERNAL)), 2);
    }

    #[tokio::test]
    async fn deleting_asks_first_and_warns_about_folder_contents() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        browser.row_action("DCIM", "Delete").await;
        let [
            ui::Message::Confirm {
                title,
                body,
                confirm_label,
                then,
                ..
            },
        ] = &browser.take_requests()[..]
        else {
            panic!("a confirmation");
        };
        assert_eq!(title, "Delete DCIM?");
        assert!(body.contains("everything in it"), "{body}");
        assert_eq!(confirm_label, "Delete");
        browser.send(browse(then.clone())).await;
        assert!(
            browser
                .files
                .calls()
                .contains(&Call::Delete(format!("{INTERNAL}/DCIM")))
        );

        browser.row_action("notes.txt", "Delete").await;
        let [ui::Message::Confirm { body, .. }] = &browser.take_requests()[..] else {
            panic!("a confirmation");
        };
        assert_eq!(
            body,
            "The file will be deleted from the device. This can’t be undone."
        );
    }

    #[tokio::test]
    async fn a_failed_change_is_reported_and_the_folder_listed_again() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        *lock(&browser.files.fail_changes) = Some(|| BrowseError::Exists);
        browser.click(widget::Id::from("New folder")).await;
        let Prompt { then, .. } = prompt(browser.take_requests());
        browser.send(browse(then("DCIM".into()))).await;
        let [ui::Message::Toast { text, .. }] = &browser.take_requests()[..] else {
            panic!("a toast");
        };
        assert_eq!(text, "There is already a file or folder with that name.");
        assert_eq!(browser.files.listed(Some(INTERNAL)), 2);
    }

    #[tokio::test]
    async fn files_dropped_on_a_folder_are_uploaded_into_it() {
        let mut browser = Browser::new();
        let local = tempfile::tempdir().unwrap();
        let photo = local.path().join("photo.jpg");
        std::fs::write(&photo, "jpg").unwrap();
        let gone = local.path().join("gone.jpg");

        browser.go(None).await;
        let storage = browser.route.clone();
        assert!(
            browser.ui.drop_target(browser.device(), &storage).is_none(),
            "the storage isn't a folder: a drop there sends instead"
        );
        browser.go(Some(INTERNAL)).await;
        let here = browser.route.clone();
        let target = browser
            .ui
            .drop_target(browser.device(), &here)
            .expect("the folder takes drops");
        assert_eq!(target.label, "Drop to upload to All files");
        browser
            .send(browse((target.on_drop)(vec![photo.clone()])))
            .await;
        assert!(browser.files.calls().contains(&Call::Upload {
            folder: INTERNAL.into(),
            local: photo.clone(),
        }));
        assert_eq!(browser.files.listed(Some(INTERNAL)), 2);
        assert!(browser.take_requests().is_empty(), "nothing to report");

        // Failures are summed up once.
        browser
            .send(browse((target.on_drop)(vec![gone, photo])))
            .await;
        let [ui::Message::Toast { text, .. }] = &browser.take_requests()[..] else {
            panic!("a toast");
        };
        assert_eq!(text, "Couldn’t upload gone.jpg: The file couldn’t be read.");
    }

    #[tokio::test]
    async fn upload_files_picks_files_for_the_open_folder() {
        let mut browser = Browser::new();
        browser.go(Some(SD_CARD)).await;
        browser.click(widget::Id::from("Upload files")).await;
        let [ui::Message::PickFiles(PickFiles { title, then, .. })] = &browser.take_requests()[..]
        else {
            panic!("a picker");
        };
        assert_eq!(title, "Upload files to SD card");
        let Message::Upload { folder, paths, .. } = browse(then(vec!["/tmp/a.txt".into()])) else {
            panic!("the picked files are uploaded");
        };
        assert_eq!(folder, SD_CARD);
        assert_eq!(paths, [PathBuf::from("/tmp/a.txt")]);

        // Only inside a folder.
        browser.go(None).await;
        browser.click(widget::Id::from("Upload files")).await;
        browser.click(widget::Id::from("New folder")).await;
        assert!(browser.take_requests().is_empty());
    }

    #[tokio::test]
    async fn a_device_that_refuses_says_why_and_how_to_fix_it() {
        let mut browser = Browser::new();
        *lock(&browser.files.refuse) = Some(|| BrowseError::Unavailable {
            reason: Some("No storage locations configured".into()),
        });
        browser.go(None).await;
        let says = "The device isn’t sharing its files (No storage locations configured). In \
                    KDE Connect on the device, allow access to files in the Filesystem expose \
                    plugin.";
        assert!(browser.shows(says));

        *lock(&browser.files.refuse) = None;
        browser.click("Retry").await;
        assert!(browser.shows("All files"));
    }

    #[tokio::test]
    async fn browsing_needs_a_device_that_shares_its_files() {
        let browse = |device: &DeviceSnapshot| {
            let [action] = device_actions(device).try_into().unwrap();
            action
        };
        let action = browse(&pixel());
        assert_eq!(action.label, "Browse files");
        assert!(action.enabled);
        assert!(action.visible_in_tray);
        let incapable = browse(&testing::device("Pixel"));
        assert!(!incapable.enabled, "listed, but disabled");
        assert!(!incapable.visible_in_tray, "and not in the tray");

        let mut away = pixel();
        away.reachability = DeviceReachability::Discovered;
        assert!(!browse(&away).enabled);

        let mut browser = Browser::with_device(away);
        browser.go(None).await;
        assert!(browser.shows("Connect Pixel to browse its files."));
        assert!(browser.files.calls().is_empty(), "nothing asked");

        let mut browser = Browser::with_device(testing::device("Pixel"));
        browser.go(None).await;
        assert!(browser.shows("Pixel doesn’t share its files."));
    }

    #[tokio::test]
    async fn the_files_are_listed_again_once_the_device_is_back() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        let event = |device: DeviceSnapshot| CoreEvent {
            sequence: 1,
            timestamp: 1,
            event: EventData::DeviceUpdated(device),
        };
        let mut away = pixel();
        away.reachability = DeviceReachability::Discovered;
        for device in [away, pixel()] {
            let event = event(device);
            browser.ctx.store_mut().apply_event(&event.event);
            let task = browser.ui.on_event(&browser.ctx, &event);
            browser.run(task).await;
        }
        assert_eq!(browser.files.listed(Some(INTERNAL)), 2);
        assert_eq!(browser.files.listed(None), 2);

        // Nothing changed: nothing listed.
        let event = event(pixel());
        let task = browser.ui.on_event(&browser.ctx, &event);
        browser.run(task).await;
        assert_eq!(browser.files.listed(Some(INTERNAL)), 2);
    }

    #[tokio::test]
    async fn only_the_newest_listing_of_a_folder_counts() {
        let mut browser = Browser::new();
        browser.go(Some(INTERNAL)).await;
        let stale = Message::Listed {
            device_id: pixel().device_id,
            folder: Some(INTERNAL.into()),
            request: 0,
            result: Err("stale".into()),
        };
        browser.send(stale).await;
        assert!(browser.shows("notes.txt"));
    }

    /// The browse codes, worded as the Flutter app worded them.
    #[test]
    fn codes_read_like_the_flutter_app() {
        for (code, message) in [
            ("file_not_found", "That file or folder no longer exists."),
            (
                "file_exists",
                "There is already a file or folder with that name.",
            ),
            ("file_permission_denied", "The device doesn’t allow that."),
            ("not_a_directory", "That isn’t a folder."),
            ("is_a_directory", "Folders can’t be downloaded, only files."),
            ("invalid_path", "That name or location can’t be used."),
            ("files_failed", "The device’s files couldn’t be reached."),
            ("files_timed_out", "The device took too long to answer."),
            (
                "files_host_key_mismatch",
                "The device’s file server didn’t prove it is the paired device, so \
                 MyConnect didn’t connect to it.",
            ),
        ] {
            assert_eq!(describe(code, None), message, "{code}");
        }
    }

    #[test]
    fn unavailable_says_why_when_the_device_does() {
        assert_eq!(
            describe_error(&BrowseError::Unavailable {
                reason: Some("No storage access".into())
            }),
            "The device isn’t sharing its files (No storage access). In KDE Connect on \
             the device, allow access to files in the Filesystem expose plugin."
        );
        assert_eq!(
            describe_error(&BrowseError::Unavailable { reason: None }),
            "The device isn’t sharing its files. In KDE Connect on the device, allow \
             access to files in the Filesystem expose plugin."
        );
    }

    #[test]
    fn core_errors_read_as_the_core_words_them() {
        assert_eq!(
            describe_error(&BrowseError::Core(CoreError::DeviceNotConnected)),
            "The device is not connected right now."
        );
    }

    #[tokio::test]
    async fn snapshot_files() {
        let mut browser = Browser::new();
        browser.go(None).await;
        testing::snapshot("files-storage", (720.0, 420.0), || browser.page());
        browser.go(Some(&format!("{INTERNAL}/DCIM/Camera"))).await;
        testing::snapshot("files-empty", (720.0, 420.0), || browser.page());
        browser.go(Some(INTERNAL)).await;
        testing::snapshot("files-folder", (720.0, 420.0), || browser.page());
        browser
            .send(Message::Menu(format!("{INTERNAL}/photo.png")))
            .await;
        testing::snapshot("files-folder-narrow", (440.0, 420.0), || browser.page());

        let mut png = std::io::Cursor::new(Vec::new());
        ::image::RgbaImage::from_fn(64, 48, |x, y| {
            ::image::Rgba([(x * 4) as u8, (y * 5) as u8, 160, 255])
        })
        .write_to(&mut png, ::image::ImageFormat::Png)
        .unwrap();
        *lock(&browser.files.content) = png.into_inner();
        browser.click("photo.png").await;
        testing::snapshot("files-preview", (720.0, 520.0), || browser.page());

        let mut browser = Browser::with_device(testing::device("Pixel"));
        browser.go(None).await;
        testing::snapshot("files-not-shared", (440.0, 320.0), || browser.page());
    }
}
