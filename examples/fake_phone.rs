//! Run the fake KDE Connect for Android from the browse tests, to try file
//! browsing in the app without a phone.
//!
//! ```sh
//! cargo run --example fake_phone -- <DATA_DIR> <STORAGE_DIR> <DESKTOP_ID> [NAME]
//! ```
//!
//! It listens for loopback discovery announcements on UDP
//! 127.255.255.255:1716, or on the port in `FERRY_DISCOVERY_PORT` when the
//! desktop was given `--discovery-port` (shared with other local instances
//! on that port, and off the LAN), dials only the desktop with `DESKTOP_ID`, accepts its
//! pairing request, and serves `STORAGE_DIR` as the phone's storage: its
//! `internal` folder as "Internal storage" and its `sdcard` folder as
//! "SD card". Start the desktop with loopback discovery so its
//! announcements reach the phone.

#[path = "../tests/support/fake_phone.rs"]
mod fake_phone;

use std::{net::SocketAddr, path::PathBuf};

use fake_phone::{BrowseReply, FakePhone, FakePhoneConfig, PHONE_NAME};
use ferry::transport::lan::{DISCOVERY_PORT, LOOPBACK_BROADCAST};

#[tokio::main]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (data_dir, storage, desktop_id, name) = match arguments.as_slice() {
        [data_dir, storage, desktop_id] => (data_dir, storage, desktop_id, PHONE_NAME),
        [data_dir, storage, desktop_id, name] => (data_dir, storage, desktop_id, name.as_str()),
        _ => {
            eprintln!("usage: fake_phone <DATA_DIR> <STORAGE_DIR> <DESKTOP_ID> [NAME]");
            std::process::exit(2);
        }
    };
    let discovery_port = match std::env::var("FERRY_DISCOVERY_PORT") {
        Ok(port) => port.parse().expect("FERRY_DISCOVERY_PORT is a port number"),
        Err(_) => DISCOVERY_PORT,
    };
    let storage = PathBuf::from(storage);
    for root in ["internal", "sdcard"] {
        std::fs::create_dir_all(storage.join(root)).expect("storage folders can be created");
    }

    let phone = FakePhone::start(FakePhoneConfig {
        name: name.to_owned(),
        data_dir: data_dir.into(),
        storage,
        reply: BrowseReply::Serve(vec![
            ("/internal".into(), "Internal storage".into()),
            ("/sdcard".into(), "SD card".into()),
        ]),
        wrong_host_key: false,
        desktop_id: Some(desktop_id.clone()),
        discovery_bind: SocketAddr::from((LOOPBACK_BROADCAST, discovery_port)),
    })
    .await;
    println!("Fake phone {} waiting for {desktop_id}", phone.device_id);
    tokio::signal::ctrl_c()
        .await
        .expect("Ctrl-C can be awaited");
    phone.stop().await;
}
