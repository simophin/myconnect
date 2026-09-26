//! The app's translations: Fluent files in `i18n/<lang>/ferry.ftl`,
//! embedded in the binary, and [`fl!`], which looks a message up in the
//! chosen language.
//!
//! `fl!` checks at compile time that the key exists in the en-US file and
//! that it is given exactly the arguments that message uses:
//! `fl!("drop-send-to-header", count = 3)`. The language is chosen once,
//! at start ([`select_system_language`], from `launch::run`); until then,
//! and in tests, [`LOADER`] holds en-US, the fallback for any key a
//! translation lacks. Numbers in messages and dates are in the user's
//! locale ([`format`]). `FERRY_LANG=en-XA` shows a pseudo-locale
//! generated from en-US ([`pseudo`]). See `docs/PLAN_I18N.md`.

pub mod format;
pub mod pseudo;

use std::{ops::Deref, sync::LazyLock};

use i18n_embed::{
    DesktopLanguageRequester, I18nEmbedError, LanguageLoader,
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
pub static LOADER: Loader = Loader(LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("the en-US messages are embedded and parse");
    // Unit tests compare text with plain strings.
    loader.set_use_isolating(!cfg!(test));
    format_numbers(&loader);
    loader
}));

/// [`LOADER`]'s type: the app's one loader, except that in unit tests a
/// thread inside [`in_pseudo_locale`] sees the pseudo-locale's.
pub struct Loader(LazyLock<FluentLanguageLoader>);

impl Deref for Loader {
    type Target = FluentLanguageLoader;

    fn deref(&self) -> &FluentLanguageLoader {
        #[cfg(test)]
        if IN_PSEUDO_LOCALE.get() {
            return &PSEUDO_LOADER;
        }
        &self.0
    }
}

#[cfg(test)]
thread_local! {
    static IN_PSEUDO_LOCALE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// en-XA, with isolation marks, as the app would show it.
#[cfg(test)]
static PSEUDO_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    load(&loader, &[pseudo::language()]).expect("en-XA is made from en-US, which parses");
    loader
});

/// Run `f` with `fl!` on this thread looking messages up in en-XA, for
/// snapshots: other tests running at the same time still see en-US.
#[cfg(test)]
pub fn in_pseudo_locale<T>(f: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            IN_PSEUDO_LOCALE.set(false);
        }
    }
    IN_PSEUDO_LOCALE.set(true);
    let _reset = Reset;
    f()
}

/// Write `loader`'s numbers in the user's locale. Like isolation, this is
/// lost whenever the loader's languages are (re)loaded.
fn format_numbers(loader: &FluentLanguageLoader) {
    loader.with_bundles_mut(|bundle| bundle.set_formatter(Some(format::format_fluent_value)));
}

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
    match load(&LOADER, requested) {
        Ok(languages) => {
            tracing::info!(?requested, ?languages, "chose the UI's languages");
            format::set_locale(requested, languages.first().unwrap_or(&en_us()));
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

/// Load into `loader` the translations that best match `requested`, the
/// pseudo-locale only if it is asked for by name, and have it write
/// numbers in the user's locale.
fn load(
    loader: &FluentLanguageLoader,
    requested: &[LanguageIdentifier],
) -> Result<Vec<LanguageIdentifier>, I18nEmbedError> {
    let languages = if pseudo::is_requested(requested) {
        i18n_embed::select(loader, &pseudo::WithPseudo(Localizations), requested)?
    } else {
        i18n_embed::select(loader, &Localizations, requested)?
    };
    format_numbers(loader);
    Ok(languages)
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

    /// The packaging scripts (`packaging/i18n.sh`) copy `package-*`
    /// messages as they are, a line each, so they must be plain text.
    #[test]
    fn package_messages_are_plain_text_on_one_line() {
        use fluent_syntax::ast::PatternElement;

        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n");
        let mut checked = 0;
        for entry in fs::read_dir(&directory).expect("the i18n directory") {
            let path = entry.expect("an i18n entry").path().join("ferry.ftl");
            let source = fs::read_to_string(&path).expect("a locale's ferry.ftl");
            let resource: Resource<&str> =
                fluent_syntax::parser::parse(source.as_str()).expect("the .ftl parses");
            for entry in resource.body {
                let Entry::Message(message) = entry else {
                    continue;
                };
                if !message.id.name.starts_with("package-") {
                    continue;
                }
                let plain = match message.value.as_ref().map(|value| &value.elements[..]) {
                    Some([PatternElement::TextElement { value }]) => !value.contains('\n'),
                    _ => false,
                };
                assert!(
                    plain && message.attributes.is_empty(),
                    "{}: {} must be plain text on one line",
                    path.display(),
                    message.id.name
                );
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn tests_see_en_us_without_isolation_marks() {
        assert_eq!(LOADER.current_languages(), [en_us()]);
        assert_eq!(fl!("drop-send-to-header", count = 1), "Send 1 file to:");
        assert_eq!(fl!("drop-send-to-header", count = 3), "Send 3 files to:");
    }

    #[test]
    fn the_pseudo_locale_is_only_for_the_thread_that_asks() {
        let pseudo = in_pseudo_locale(|| fl!("settings-title"));
        assert_eq!(pseudo, "[Šééţţîîñĝš]");
        assert_eq!(fl!("settings-title"), "Settings");
        let other = std::thread::spawn(|| in_pseudo_locale(|| ()));
        other.join().unwrap();
        assert_eq!(fl!("settings-title"), "Settings");
    }

    #[test]
    fn en_xa_is_loaded_only_when_asked_for_by_name() {
        // Other loaders, so the shared one stays as the other tests expect.
        let tags = |tags: &[&str]| -> Vec<LanguageIdentifier> {
            tags.iter().map(|tag| tag.parse().unwrap()).collect()
        };
        let loader: FluentLanguageLoader = fluent_language_loader!();
        let chosen = load(&loader, &tags(&["en-XA"])).unwrap();
        assert_eq!(chosen, tags(&["en-XA", "en-US"]));
        loader.set_use_isolating(false);
        assert_eq!(
            loader.get_args("drop-send-to-header", [("count", 1234)].into()),
            "[Šééñď 1,234 fîîļééš ţöö:]"
        );

        let loader: FluentLanguageLoader = fluent_language_loader!();
        assert_eq!(load(&loader, &tags(&["en-GB"])).unwrap(), [en_us()]);
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
