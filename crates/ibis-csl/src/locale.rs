//! ICU supplies the locale operations QuickJS deliberately does not implement.
use std::cell::RefCell;
use std::collections::BTreeMap;

use icu_casemap::CaseMapper;
use icu_collator::options::{AlternateHandling, CaseLevel, CollatorOptions, MaxVariable, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::Locale;
use rquickjs::{Ctx, Function};

const MAX_STRING_BYTES: usize = 256 * 1024;
const MAX_COLLATORS: usize = 16;

fn invalid() -> rquickjs::Error {
    rquickjs::Error::new_from_js_message("value", "locale operation", "invalid or oversized locale input")
}

pub(crate) fn install(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let cache = RefCell::new(BTreeMap::<(String, String, bool, i32), CollatorBorrowed<'static>>::new());
    ctx.globals().set(
        "__ibisCompare",
        Function::new(
            ctx.clone(),
            move |a: String, b: String, locale: String, sensitivity: String, ignore_punctuation: bool, numeric: i32| {
                if a.len() > MAX_STRING_BYTES || b.len() > MAX_STRING_BYTES || locale.len() > 128 {
                    return Err(invalid());
                }
                let key = (locale.clone(), sensitivity.clone(), ignore_punctuation, numeric);
                let compare = |collator: &CollatorBorrowed<'static>| match collator.compare(&a, &b) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                };
                if let Some(collator) = cache.borrow().get(&key) {
                    return Ok(compare(collator));
                }
                let mut prefs: CollatorPreferences = Locale::try_from_str(&locale).map_err(|_| invalid())?.into();
                prefs.numeric_ordering = match numeric {
                    1 => Some(CollationNumericOrdering::True),
                    0 => Some(CollationNumericOrdering::False),
                    _ => prefs.numeric_ordering,
                };
                let mut options = CollatorOptions::default();
                options.strength = Some(match sensitivity.as_str() {
                    "base" | "case" => Strength::Primary,
                    "accent" => Strength::Secondary,
                    "variant" => Strength::Tertiary,
                    _ => return Err(invalid()),
                });
                options.case_level = Some(if sensitivity == "case" {
                    CaseLevel::On
                } else {
                    CaseLevel::Off
                });
                options.alternate_handling = Some(if ignore_punctuation {
                    AlternateHandling::Shifted
                } else {
                    AlternateHandling::NonIgnorable
                });
                options.max_variable = Some(MaxVariable::Punctuation);
                let collator = Collator::try_new(prefs, options).map_err(|_| invalid())?;
                let result = compare(&collator);
                if cache.borrow().len() < MAX_COLLATORS {
                    cache.borrow_mut().insert(key, collator);
                }
                Ok(result)
            },
        )?,
    )?;
    ctx.globals().set(
        "__ibisCase",
        Function::new(ctx.clone(), |input: String, locale: String, upper: bool| {
            if input.len() > MAX_STRING_BYTES || locale.len() > 128 {
                return Err(invalid());
            }
            let locale = Locale::try_from_str(&locale).map_err(|_| invalid())?;
            let mapper = CaseMapper::new();
            Ok(if upper {
                mapper.uppercase_to_string(&input, &locale.id).into_owned()
            } else {
                mapper.lowercase_to_string(&input, &locale.id).into_owned()
            })
        })?,
    )?;
    ctx.eval::<(), _>(include_str!("locale.js"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use rquickjs::{Context, Runtime};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        locale: String,
        input: String,
        lower: String,
        upper: String,
    }

    #[test]
    fn locale_case_matches_pinned_node_output() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../tests/locale-cases.json")).unwrap();
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            install(&ctx).unwrap();
            for case in cases {
                ctx.globals().set("input", case.input).unwrap();
                ctx.globals().set("locale", case.locale).unwrap();
                let lower: String = ctx.eval("input.toLocaleLowerCase(locale)").unwrap();
                let upper: String = ctx.eval("input.toLocaleUpperCase(locale)").unwrap();
                assert_eq!(lower, case.lower);
                assert_eq!(upper, case.upper);
            }
            assert_eq!(
                ctx.eval::<i32, _>("'2'.localeCompare('10','en-US',{numeric:true})")
                    .unwrap(),
                -1
            );
            assert_eq!(
                ctx.eval::<i32, _>("'Ä-b'.localeCompare('ab','en-US',{sensitivity:'base',ignorePunctuation:true})")
                    .unwrap(),
                0
            );
        });
    }
}
