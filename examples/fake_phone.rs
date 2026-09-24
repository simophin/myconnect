//! Run the fake KDE Connect for Android from the browse tests, to try file
//! browsing in the app without a phone.
//!
//! ```sh
//! cargo run --example fake_phone -- <DATA_DIR> <STORAGE_DIR> <DESKTOP_ID>
//! ```
//!
//! It listens for discovery announcements on UDP 1716 (shared with other
//! local instances), dials only the desktop with `DESKTOP_ID`, accepts its
//! pairing request, and serves `STORAGE_DIR` as the phone's storage: its
//! `internal` folder as "Internal storage" and its `sdcard` folder as
//! "SD card". Start the desktop with loopback discovery so its
//! announcements reach the phone.

#[path = "../tests/support/fake_phone.rs"]
mod fake_phone;

use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use fake_phone::{BrowseReply, FakePhone, FakePhoneConfig};

#[tokio::main]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, storage, desktop_id] = arguments.as_slice() else {
        eprintln!("usage: fake_phone <DATA_DIR> <STORAGE_DIR> <DESKTOP_ID>");
        std::process::exit(2);
    };
    let storage = PathBuf::from(storage);
    for root in ["internal", "sdcard"] {
        std::fs::create_dir_all(storage.join(root)).expect("storage folders can be created");
    }

    let phone = FakePhone::start(FakePhoneConfig {
        data_dir: data_dir.into(),
        storage,
        reply: BrowseReply::Serve(vec![
            ("/internal".into(), "Internal storage".into()),
            ("/sdcard".into(), "SD card".into()),
        ]),
        wrong_host_key: false,
        desktop_id: Some(desktop_id.clone()),
        discovery_bind: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 1716)),
    })
    .await;
    println!("Fake phone {} waiting for {desktop_id}", phone.device_id);
    tokio::signal::ctrl_c()
        .await
        .expect("Ctrl-C can be awaited");
    phone.stop().await;
}
