//! The app's language switching at run time, as the `language` setting
//! makes it (`ui::i18n::follow_setting`). A test binary of its own, since
//! it changes the process's one loader, which unit tests share.

use ferry::ui::i18n::{self, LOADER};

#[test]
fn the_language_setting_switches_the_loaded_translation() {
    let title = || LOADER.get("settings-title");
    let header = || LOADER.get_args("drop-send-to-header", [("count", 1234)].into());

    assert!(
        !i18n::follow_setting(Some("de")),
        "nothing follows the setting before a language was chosen at start"
    );
    i18n::use_test_language();
    assert_eq!(title(), "Settings");

    assert!(i18n::follow_setting(Some("de")));
    assert_eq!(title(), "Einstellungen");
    // Numbers in German, and still no isolation marks, as the test asked.
    assert_eq!(header(), "1.234 Dateien senden an:");
    assert!(!i18n::follow_setting(Some("de")), "already German");

    assert!(i18n::follow_setting(Some("zh-CN")));
    assert_eq!(title(), "设置");
    assert!(i18n::follow_setting(Some("en-XA")));
    assert_eq!(title(), "[Šééţţîîñĝš]");

    // Back to the system's, which is en-US for tests.
    assert!(i18n::follow_setting(None));
    assert_eq!(title(), "Settings");
    assert_eq!(header(), "Send 1,234 files to:");
}
