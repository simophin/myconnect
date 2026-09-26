//! The app's translations: Fluent files in `i18n/<lang>/ferry.ftl`,
//! embedded in the binary, and [`fl!`], which looks a message up in the
//! chosen language.
//!
//! `fl!` checks at compile time that the key exists in the en-US file and
//! that it is given exactly the arguments that message uses:
//! `fl!("drop-send-to-header", count = 3)`. The language is chosen once,
//! at start ([`select_system_language`], from `launch::run`); until then,
//! and in tests, [`LOADER`] holds en-US, the fallback for any key a
//! translation lacks. See `docs/PLAN_I18N.md`.

use std::sync::LazyLock;

use i18n_embed::{
    DesktopLanguageRequester, LanguageLoader,
    fluent::{FluentLanguageLoader, fluent_language_loader},
    unic_langid::LanguageIdentifier,
};
use rust_embed::RustEmbed;

/// Forces the language, for testing: a BCP 47 tag such as `de` or `zh-CN`.
pub const LANGUAGE_VARIABLE: &str = "FERRY_LANG";

/// Every locale's `.ftl` file, under `i18n/`.
#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

/// The messages in the chosen language, falling back to en-US.
pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("the en-US messages are embedded and parse");
    // Unit tests compare text with plain strings.
    loader.set_use_isolating(!cfg!(test));
    loader
});

/// A message from the app's `.ftl` files, in the chosen language:
/// `fl!("key")`, or `fl!("key", name = value, ...)`.
macro_rules! fl {
    ($($message:tt)+) => {
        ::i18n_embed_fl::fl!($crate::ui::i18n::LOADER, $($message)+)
    };
}
pub(crate) use fl;

/// Choose the language the app shows: `FERRY_LANG` if it is set, else the
/// system's preferred languages, falling back to en-US for what isn't
/// translated. Returns the languages loaded, most preferred first.
pub fn select_system_language() -> Vec<LanguageIdentifier> {
    let requested = match forced_language() {
        Some(language) => vec![language],
        None => DesktopLanguageRequester::requested_languages(),
    };
    select(&requested)
}

/// Show the app in en-US without Fluent's bidi isolation marks around
/// arguments, so that tests can look for "To Pixel · 3.0 MB" whatever the
/// machine's language. For tests that run the whole program
/// (`tests/ui_e2e.rs`); unit tests get this without asking.
pub fn use_test_language() {
    select(&[en_us()]);
    LOADER.set_use_isolating(false);
}

/// The language `FERRY_LANG` asks for, if it is set and valid.
fn forced_language() -> Option<LanguageIdentifier> {
    let value = std::env::var(LANGUAGE_VARIABLE).ok()?;
    match value.parse() {
        Ok(language) => Some(language),
        Err(error) => {
            tracing::warn!("ignoring {LANGUAGE_VARIABLE}={value:?}: {error}");
            None
        }
    }
}

fn select(requested: &[LanguageIdentifier]) -> Vec<LanguageIdentifier> {
    match i18n_embed::select(&*LOADER, &Localizations, requested) {
        Ok(languages) => {
            tracing::info!(?requested, ?languages, "chose the UI's languages");
            languages
        }
        // Only if an embedded file doesn't parse; the loader keeps what it
        // had, which is at least en-US.
        Err(error) => {
            tracing::warn!("couldn't load the UI's translations: {error}");
            LOADER.current_languages()
        }
    }
}

fn en_us() -> LanguageIdentifier {
    LOADER.fallback_language().clone()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::Path};

    use fluent_syntax::ast::{Entry, Resource};

    use super::*;

    /// Each message's id, each term's (`-name`), and each of their
    /// attributes (`id.attribute`), in `source`, a whole `.ftl` file.
    /// Panics if it doesn't parse.
    fn keys(language: &str, source: &str) -> BTreeSet<String> {
        let resource: Resource<&str> = fluent_syntax::parser::parse(source)
            .unwrap_or_else(|(_, errors)| panic!("i18n/{language}/ferry.ftl: {errors:?}"));
        let mut keys = BTreeSet::new();
        for entry in resource.body {
            let (id, attributes) = match entry {
                Entry::Message(message) => (message.id.name.to_owned(), message.attributes),
                Entry::Term(term) => (format!("-{}", term.id.name), term.attributes),
                _ => continue,
            };
            for attribute in attributes {
                keys.insert(format!("{id}.{}", attribute.id.name));
            }
            assert!(keys.insert(id.clone()), "{language}: {id} is defined twice");
        }
        keys
    }

    #[test]
    fn every_locale_parses_and_has_exactly_en_us_keys() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n");
        let read = |language: &str| {
            fs::read_to_string(directory.join(language).join("ferry.ftl"))
                .unwrap_or_else(|error| panic!("i18n/{language}/ferry.ftl: {error}"))
        };
        let english = keys("en-US", &read("en-US"));
        assert!(!english.is_empty());
        for entry in fs::read_dir(&directory).expect("the i18n directory") {
            let entry = entry.expect("an i18n entry");
            let language = entry.file_name().to_string_lossy().into_owned();
            assert!(
                entry.path().is_dir(),
                "i18n/{language}: only locale directories belong here"
            );
            language
                .parse::<LanguageIdentifier>()
                .unwrap_or_else(|error| panic!("i18n/{language}: not a language tag: {error}"));
            let found = keys(&language, &read(&language));
            let missing: Vec<_> = english.difference(&found).collect();
            let extra: Vec<_> = found.difference(&english).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "i18n/{language}/ferry.ftl: missing {missing:?}, not in en-US {extra:?}"
            );
        }
    }

    #[test]
    fn tests_see_en_us_without_isolation_marks() {
        assert_eq!(LOADER.current_languages(), [en_us()]);
        assert_eq!(fl!("drop-send-to-header", count = 1), "Send 1 file to:");
        assert_eq!(fl!("drop-send-to-header", count = 3), "Send 3 files to:");
    }

    #[test]
    fn an_unknown_language_falls_back_to_en_us() {
        // Another loader, so the shared one stays as the other tests expect.
        let loader: FluentLanguageLoader = fluent_language_loader!();
        let requested: LanguageIdentifier = "tlh".parse().unwrap();
        let chosen = i18n_embed::select(&loader, &Localizations, &[requested]).unwrap();
        assert_eq!(chosen, [en_us()]);
        loader.set_use_isolating(false);
        assert_eq!(
            loader.get_args("drop-send-to-header", [("count", 2)].into()),
            "Send 2 files to:"
        );
    }
}
