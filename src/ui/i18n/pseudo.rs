//! `en-XA`, a pseudo-locale for finding what isn't translated and what
//! doesn't fit: every en-US message, accented, about 40% longer and in
//! brackets, "[Šééţţîîñĝš]". A string that shows up plain was never
//! extracted; one whose closing bracket is cut off doesn't have room to
//! grow. It is generated from en-US whenever it is loaded, so it is never
//! out of date and nobody edits it.
//!
//! It is offered only when asked for by name (`FERRY_LANG=en-XA`), so a
//! system asking for some other English can't end up in it.

use std::borrow::Cow;

use fluent_syntax::{
    ast::{Entry, Expression, InlineExpression, Pattern, PatternElement},
    parser, serializer,
};
use i18n_embed::{I18nAssets, unic_langid::LanguageIdentifier};

/// The pseudo-locale's tag.
pub fn language() -> LanguageIdentifier {
    "en-XA".parse().expect("en-XA is a valid tag")
}

/// Whether `requested` asks for the pseudo-locale.
pub(super) fn is_requested(requested: &[LanguageIdentifier]) -> bool {
    requested.contains(&language())
}

/// Another set of translations plus `en-XA`, made from its en-US file.
pub(super) struct WithPseudo<A>(pub A);

impl<A: I18nAssets> WithPseudo<A> {
    fn file() -> String {
        format!("{}/{}", language(), FILE_NAME)
    }
}

/// Every locale's file name, in its directory.
const FILE_NAME: &str = "ferry.ftl";

impl<A: I18nAssets> I18nAssets for WithPseudo<A> {
    fn get_files(&self, file_path: &str) -> Vec<Cow<'_, [u8]>> {
        if file_path != Self::file() {
            return self.0.get_files(file_path);
        }
        self.0
            .get_files(&format!("en-US/{FILE_NAME}"))
            .iter()
            .map(|english| Cow::Owned(pseudolocalize(&String::from_utf8_lossy(english)).into()))
            .collect()
    }

    fn filenames_iter(&self) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(self.0.filenames_iter().chain([Self::file()]))
    }
}

/// `source`, a whole `.ftl` file, with every message's text accented and
/// lengthened, and each message and attribute in brackets. Variable names,
/// selectors and variant keys are left alone, so it has the same messages
/// taking the same arguments.
pub fn pseudolocalize(source: &str) -> String {
    let mut resource = parser::parse(source.to_owned()).unwrap_or_else(|(resource, errors)| {
        tracing::warn!(?errors, "the en-US messages don't all parse");
        resource
    });
    for entry in &mut resource.body {
        match entry {
            Entry::Message(message) => {
                if let Some(value) = &mut message.value {
                    bracket(value);
                }
                for attribute in &mut message.attributes {
                    bracket(&mut attribute.value);
                }
            }
            // A term is part of the messages that use it, which have
            // their own brackets.
            Entry::Term(term) => {
                accent_pattern(&mut term.value);
                for attribute in &mut term.attributes {
                    accent_pattern(&mut attribute.value);
                }
            }
            _ => {}
        }
    }
    serializer::serialize(&resource)
}

/// Accent `pattern` and put it in brackets. They are string literals,
/// `{ "[" }`, as a `[` starting a line of a multiline pattern would read
/// as a variant key.
fn bracket(pattern: &mut Pattern<String>) {
    accent_pattern(pattern);
    let literal = |value: &str| PatternElement::Placeable {
        expression: Expression::Inline(InlineExpression::StringLiteral {
            value: value.to_owned(),
        }),
    };
    pattern.elements.insert(0, literal("["));
    pattern.elements.push(literal("]"));
}

fn accent_pattern(pattern: &mut Pattern<String>) {
    for element in &mut pattern.elements {
        match element {
            PatternElement::TextElement { value } => *value = accent(value),
            PatternElement::Placeable { expression } => accent_expression(expression),
        }
    }
}

fn accent_expression(expression: &mut Expression<String>) {
    match expression {
        Expression::Select { variants, .. } => {
            for variant in variants {
                accent_pattern(&mut variant.value);
            }
        }
        Expression::Inline(InlineExpression::Placeable { expression }) => {
            accent_expression(expression);
        }
        Expression::Inline(_) => {}
    }
}

/// `text` with its letters accented and each vowel doubled, which makes
/// English about a third longer; with the brackets, short labels grow more.
/// The accented letters are all in Latin-1 and Latin Extended-A, which the
/// app's font has.
fn accent(text: &str) -> String {
    let mut accented = String::with_capacity(text.len() * 2);
    for letter in text.chars() {
        accented.push(accented_letter(letter));
        if matches!(letter.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u') {
            accented.push(accented_letter(letter.to_ascii_lowercase()));
        }
    }
    accented
}

fn accented_letter(letter: char) -> char {
    match letter {
        'a' => 'å',
        'c' => 'ç',
        'd' => 'ď',
        'e' => 'é',
        'g' => 'ĝ',
        'h' => 'ĥ',
        'i' => 'î',
        'j' => 'ĵ',
        'k' => 'ķ',
        'l' => 'ļ',
        'n' => 'ñ',
        'o' => 'ö',
        'r' => 'ŕ',
        's' => 'š',
        't' => 'ţ',
        'u' => 'û',
        'w' => 'ŵ',
        'y' => 'ý',
        'z' => 'ž',
        'A' => 'Å',
        'C' => 'Ç',
        'D' => 'Ď',
        'E' => 'É',
        'G' => 'Ĝ',
        'H' => 'Ĥ',
        'I' => 'Î',
        'J' => 'Ĵ',
        'K' => 'Ķ',
        'L' => 'Ļ',
        'N' => 'Ñ',
        'O' => 'Ö',
        'R' => 'Ŕ',
        'S' => 'Š',
        'T' => 'Ţ',
        'U' => 'Û',
        'W' => 'Ŵ',
        'Y' => 'Ý',
        'Z' => 'Ž',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::i18n::Localizations;

    #[test]
    fn accents_and_lengthens_text_but_not_arguments_or_keys() {
        let source = "\
greeting = Hello, { $name }!
files =
    { $count ->
        [one] Send { $count } file
       *[other] Send { $count } files
    }
button = Save
    .tooltip = Save it
";
        let pseudo = pseudolocalize(source);
        let resource = fluent_bundle::FluentResource::try_new(pseudo.clone())
            .unwrap_or_else(|(_, errors)| panic!("{pseudo}\n{errors:?}"));
        let mut bundle = fluent_bundle::FluentBundle::new(vec![language()]);
        bundle.set_use_isolating(false);
        bundle.add_resource(resource).unwrap();
        let format =
            |id: &str, attribute: Option<&str>, args: Option<&fluent_bundle::FluentArgs>| {
                let message = bundle.get_message(id).unwrap();
                let pattern = match attribute {
                    Some(name) => message.get_attribute(name).unwrap().value(),
                    None => message.value().unwrap(),
                };
                bundle
                    .format_pattern(pattern, args, &mut vec![])
                    .into_owned()
            };
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("name", "Pixel");
        assert_eq!(format("greeting", None, Some(&args)), "[Ĥééļļöö, Pixel!]");
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("count", 1);
        assert_eq!(format("files", None, Some(&args)), "[Šééñď 1 fîîļéé]");
        args.set("count", 3);
        assert_eq!(format("files", None, Some(&args)), "[Šééñď 3 fîîļééš]");
        assert_eq!(format("button", Some("tooltip"), None), "[Šååvéé îîţ]");
    }

    #[test]
    fn has_every_en_us_message_and_is_much_longer() {
        let english = Localizations::get("en-US/ferry.ftl").unwrap().data;
        let english = std::str::from_utf8(&english).unwrap();
        let pseudo = pseudolocalize(english);
        let ids = |source: &str| -> Vec<String> {
            let resource = parser::parse(source).unwrap_or_else(|(_, errors)| panic!("{errors:?}"));
            resource
                .body
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Message(message) => Some(message.id.name.to_owned()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(ids(&pseudo), ids(english));
        // Vowels are about 30% of en-US's text; the brackets bring it to
        // about 40%.
        assert!(text_length(&pseudo) * 100 > text_length(english) * 128);
    }

    /// How many characters of text `source`'s messages have, not counting
    /// placeables.
    fn text_length(source: &str) -> usize {
        fn pattern(value: &Pattern<&str>) -> usize {
            value
                .elements
                .iter()
                .map(|element| match element {
                    PatternElement::TextElement { value } => value.chars().count(),
                    PatternElement::Placeable {
                        expression: Expression::Select { variants, .. },
                    } => variants.iter().map(|variant| pattern(&variant.value)).sum(),
                    PatternElement::Placeable { .. } => 0,
                })
                .sum()
        }
        let resource = parser::parse(source).unwrap_or_else(|(_, errors)| panic!("{errors:?}"));
        resource
            .body
            .iter()
            .map(|entry| match entry {
                Entry::Message(message) => {
                    message.value.as_ref().map_or(0, pattern)
                        + message
                            .attributes
                            .iter()
                            .map(|attribute| pattern(&attribute.value))
                            .sum::<usize>()
                }
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn is_offered_only_when_asked_for_by_name() {
        let tags = |tags: &[&str]| -> Vec<LanguageIdentifier> {
            tags.iter().map(|tag| tag.parse().unwrap()).collect()
        };
        assert!(!is_requested(&tags(&["en-GB", "en"])));
        assert!(is_requested(&tags(&["en-GB", "en-XA"])));
        let offered: Vec<_> = WithPseudo(Localizations).filenames_iter().collect();
        assert!(offered.contains(&"en-XA/ferry.ftl".to_owned()));
        assert!(offered.contains(&"en-US/ferry.ftl".to_owned()));
    }
}
