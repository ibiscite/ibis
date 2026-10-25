//! Narrow, request-local ICU bridge for the pinned processor (ADR 0027).
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use icu_casemap::CaseMapper;
use icu_collator::{
    Collator, CollatorPreferences,
    options::{AlternateHandling, CaseLevel, CollatorOptions, MaxVariable, Strength},
    preferences::CollationNumericOrdering,
};
use icu_locale_core::Locale;
use rquickjs::{Ctx, Exception, Function, function::Func};
use writeable::Writeable;

const STRING_LIMIT: usize = 1024 * 1024;
const WORK_LIMIT: usize = 64 * STRING_LIMIT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Failure {
    Invalid,
    Unsupported,
    Limit,
    Cancelled,
    TimedOut,
    Engine,
}

pub(super) struct Guard {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    failure: Cell<Option<Failure>>,
    calls: Cell<usize>,
    bytes: Cell<usize>,
}

impl Guard {
    pub(super) fn new(deadline: Instant, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            deadline,
            cancelled,
            failure: Cell::new(None),
            calls: Cell::new(0),
            bytes: Cell::new(0),
        }
    }

    pub(super) fn fail(&self, failure: Failure) {
        if self.failure.get().is_none() {
            self.failure.set(Some(failure));
        }
    }

    pub(super) fn check(&self) -> Result<(), Failure> {
        if self.cancelled.load(Ordering::Relaxed) {
            self.fail(Failure::Cancelled);
        }
        if Instant::now() >= self.deadline {
            self.fail(Failure::TimedOut);
        }
        self.failure.get().map_or(Ok(()), Err)
    }

    fn debit(&self, bytes: usize, call: bool) -> Result<(), Failure> {
        self.check()?;
        let calls = self.calls.get().checked_add(usize::from(call)).ok_or(Failure::Limit)?;
        let bytes = self.bytes.get().checked_add(bytes).ok_or(Failure::Limit)?;
        if calls > 100_000 || bytes > WORK_LIMIT {
            return Err(Failure::Limit);
        }
        self.calls.set(calls);
        self.bytes.set(bytes);
        Ok(())
    }
}

fn guarded<T>(ctx: &Ctx<'_>, guard: &Guard, run: impl FnOnce() -> Result<T, Failure>) -> rquickjs::Result<T> {
    let result = guard
        .check()
        .and_then(|()| run())
        .and_then(|value| guard.check().map(|()| value));
    result.map_err(|failure| {
        guard.fail(failure);
        Exception::throw_message(ctx, "citation formatter operation failed")
    })
}

struct BoundedText<'a> {
    text: String,
    guard: &'a Guard,
}

impl fmt::Write for BoundedText<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self
            .text
            .len()
            .checked_add(value.len())
            .is_none_or(|len| len > STRING_LIMIT)
        {
            self.guard.fail(Failure::Limit);
            return Err(fmt::Error);
        }
        if let Err(failure) = self.guard.debit(value.len(), false) {
            self.guard.fail(failure);
            return Err(fmt::Error);
        }
        if self.text.try_reserve(value.len()).is_err() {
            self.guard.fail(Failure::Limit);
            return Err(fmt::Error);
        }
        self.text.push_str(value);
        Ok(())
    }
}

pub(super) fn install<'js>(ctx: &Ctx<'js>, guard: Rc<Guard>) -> rquickjs::Result<()> {
    let cache = RefCell::new(HashMap::new());
    let compare_guard = guard.clone();
    let compare = Func::from(
        move |ctx: Ctx<'js>,
              a: rquickjs::String<'js>,
              b: rquickjs::String<'js>,
              tag: rquickjs::String<'js>,
              bytes: usize,
              base: bool| {
            guarded(&ctx, &compare_guard, || {
                compare_guard.debit(bytes, true)?;
                let a = a.to_string().map_err(|_| Failure::Invalid)?;
                let b = b.to_string().map_err(|_| Failure::Invalid)?;
                let tag = tag.to_string().map_err(|_| Failure::Invalid)?;
                let key = (tag, base);
                let mut cache = cache.try_borrow_mut().map_err(|_| Failure::Engine)?;
                if !cache.contains_key(&key) {
                    if cache.len() >= 32 {
                        return Err(Failure::Limit);
                    }
                    let locale: Locale = key.0.parse().map_err(|_| Failure::Unsupported)?;
                    let mut prefs: CollatorPreferences = locale.into();
                    let mut options = CollatorOptions::default();
                    options.strength = Some(if base { Strength::Primary } else { Strength::Tertiary });
                    options.case_level = Some(CaseLevel::Off);
                    if base {
                        prefs.numeric_ordering = Some(CollationNumericOrdering::True);
                        options.alternate_handling = Some(AlternateHandling::Shifted);
                        options.max_variable = Some(MaxVariable::Punctuation);
                    }
                    cache.insert(
                        key.clone(),
                        Collator::try_new(prefs, options).map_err(|_| Failure::Unsupported)?,
                    );
                }
                let collator = cache.get(&key).ok_or(Failure::Engine)?;
                Ok(match collator.compare(&a, &b) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                })
            })
        },
    );
    let case_guard = guard.clone();
    let casing = Func::from(
        move |ctx: Ctx<'js>, value: rquickjs::String<'js>, tag: rquickjs::String<'js>, bytes: usize, upper: bool| {
            guarded(&ctx, &case_guard, || {
                case_guard.debit(bytes, true)?;
                let value = value.to_string().map_err(|_| Failure::Invalid)?;
                let tag = tag.to_string().map_err(|_| Failure::Invalid)?;
                let locale: Locale = tag.parse().map_err(|_| Failure::Unsupported)?;
                let mapper = CaseMapper::new();
                let mut output = BoundedText {
                    text: String::new(),
                    guard: &case_guard,
                };
                let written = if upper {
                    mapper.uppercase(&value, &locale.id).write_to(&mut output)
                } else {
                    mapper.lowercase(&value, &locale.id).write_to(&mut output)
                };
                written.map_err(|_| case_guard.failure.get().unwrap_or(Failure::Engine))?;
                Ok(output.text)
            })
        },
    );
    let enter_guard = guard.clone();
    let enter = Func::from(move |ctx: Ctx<'js>| guarded(&ctx, &enter_guard, || Ok(())));
    let reject = Func::from(move |ctx: Ctx<'js>, kind: u8| -> rquickjs::Result<()> {
        guarded(&ctx, &guard, || {
            Err(match kind {
                1 => Failure::Unsupported,
                2 => Failure::Limit,
                _ => Failure::Invalid,
            })
        })
    });
    // Native closures are captured, never exposed as callable globals. JS measures
    // valid UTF-8 before any rquickjs conversion can allocate a Rust String.
    let factory: Function = ctx.eval(BRIDGE)?;
    factory.call((enter, reject, compare, casing))
}

const BRIDGE: &str = r#"
(function(enter, reject, compare, casing) {
    'use strict';
    const code = Function.prototype.call.bind(String.prototype.charCodeAt);
    const isArray = Array.isArray, descriptors = Object.getOwnPropertyDescriptors;
    const keys = Reflect.ownKeys, prototype = Object.getPrototypeOf, own = Object.hasOwn;
    const plain = Object.prototype, array = Array.prototype;
    const allowed = new Set(['en-US','en','de-DE','sv-SE','tr','az','ja','zh','ar','he','el','lt','th']);
    const has = Function.prototype.call.bind(Set.prototype.has);
    function size(s, limit = 1048576) {
        if (typeof s !== 'string') return reject(0);
        if (s.length > limit) return reject(2);
        let bytes = 0;
        for (let i = 0; i < s.length; i++) {
            const c = code(s, i);
            if (c >= 0xd800 && c <= 0xdbff) {
                const next = code(s, ++i);
                if (!(next >= 0xdc00 && next <= 0xdfff)) return reject(0);
                bytes += 4;
            } else if (c >= 0xdc00 && c <= 0xdfff) return reject(0);
            else bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
            if (bytes > limit) return reject(2);
        }
        return bytes;
    }
    function locale(value) {
        if (value === undefined) return 'en-US';
        const list = isArray(value) ? value : [value];
        if (list.length > 32 || prototype(list) !== array) return reject(1);
        const d = descriptors(list);
        if (keys(d).length !== list.length + 1) return reject(1);
        for (let i = 0; i < list.length; i++) {
            if (!own(d, i) || !own(d[i], 'value') || typeof d[i].value !== 'string') return reject(1);
            if (d[i].value.length > 128 || !has(allowed, d[i].value)) return reject(1);
        }
        return list.length ? d[0].value : 'en-US';
    }
    function options(value) {
        if (value === undefined) return false;
        if (value === null || typeof value !== 'object' || (prototype(value) !== plain && prototype(value) !== null)) return reject(1);
        const d = descriptors(value);
        if (keys(d).length !== 3 || !own(d, 'sensitivity') || !own(d, 'ignorePunctuation') || !own(d, 'numeric')) return reject(1);
        if (!own(d.sensitivity, 'value') || d.sensitivity.value !== 'base'
            || !own(d.ignorePunctuation, 'value') || d.ignorePunctuation.value !== true
            || !own(d.numeric, 'value') || d.numeric.value !== true) return reject(1);
        return true;
    }
    String.prototype.localeCompare = function(other, locales, opts) {
        enter();
        try {
            const bytes = size(this) + size(other), tag = locale(locales), base = options(opts);
            return compare(this, other, tag, bytes + tag.length, base);
        } catch (_) { return reject(0); }
    };
    for (const upper of [false, true]) {
        String.prototype[upper ? 'toLocaleUpperCase' : 'toLocaleLowerCase'] = function(locales) {
            enter();
            try {
                const bytes = size(this), tag = locale(locales);
                return casing(this, tag, bytes + tag.length, upper);
            } catch (_) { return reject(0); }
        };
    }
    globalThis.__ibisText = function(value) {
        enter();
        try { size(value); enter(); return value; } catch (_) { return reject(0); }
    };
})
"#;
