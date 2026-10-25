//! Test-only characterization; engine HTML is data, never trusted markup (ADR 0027).
use std::{
    collections::HashMap,
    io::{self, Write},
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use rquickjs::{Context, Function, Runtime, Value as JsValue, function::Func};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{cite::CiteItem, model::RecordId, settings::CitationStyle};
use intl::{Failure, Guard};

mod intl;
mod tests;

const LIMIT: usize = 1024 * 1024;
const PROCESSOR: &str = include_str!("../../../apps/desktop/vendor/csl/citeproc.mjs");
const LOCALE: &str = include_str!("../../../apps/desktop/vendor/csl/locales/en-US.xml");

#[derive(Serialize)]
struct Work {
    identity: RecordId,
    csl: Value,
}
#[derive(Serialize)]
struct Occurrence {
    identity: RecordId,
    intent: CiteItem,
}
#[derive(Clone, Copy)]
enum Output {
    Text,
    Html,
    Rtf,
}

struct Input<'a> {
    bytes: Vec<u8>,
    guard: &'a Guard,
}
impl Write for Input<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.guard
            .check()
            .map_err(|_| io::Error::other("citation request stopped"))?;
        if self.bytes.len().checked_add(bytes.len()).is_none_or(|len| len > LIMIT) {
            return Err(io::Error::other("citation input limit"));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| io::Error::other("citation input limit"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(value: &impl Serialize, guard: &Guard) -> Result<String, Failure> {
    let mut input = Input {
        bytes: Vec::new(),
        guard,
    };
    if serde_json::to_writer(&mut input, value).is_err() {
        guard.check()?;
        return Err(Failure::Limit);
    }
    String::from_utf8(input.bytes).map_err(|_| Failure::Invalid)
}

fn validate(value: &Value, guard: &Guard, visited: &mut usize) -> Result<(), Failure> {
    if !value.is_object() || !value.get("type").is_some_and(Value::is_string) {
        return Err(Failure::Invalid);
    }
    let mut pending = vec![(value, 0)];
    while let Some((value, depth)) = pending.pop() {
        guard.check()?;
        *visited += 1;
        if depth > 128 || *visited > 65_536 {
            return Err(Failure::Limit);
        }
        match value {
            Value::Object(fields) => {
                if fields
                    .keys()
                    .any(|key| matches!(key.as_str(), "__proto__" | "constructor" | "prototype"))
                {
                    return Err(Failure::Invalid);
                }
                if pending.len().checked_add(fields.len()).is_none_or(|len| len > 65_536) {
                    return Err(Failure::Limit);
                }
                pending.extend(fields.values().map(|value| (value, depth + 1)));
            }
            Value::Array(values) => {
                if pending.len().checked_add(values.len()).is_none_or(|len| len > 65_536) {
                    return Err(Failure::Limit);
                }
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(())
}

fn format(
    works: &[Work],
    cites: &[Occurrence],
    style: CitationStyle,
    output: Output,
    bibliography: bool,
    cancelled: Arc<AtomicBool>,
) -> Result<String, Failure> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let guard = Rc::new(Guard::new(deadline, cancelled));
    guard.check()?;
    if works.len() > 1000 || cites.len() > 1000 {
        return Err(Failure::Limit);
    }
    if works.is_empty() || (!bibliography && cites.is_empty()) {
        return Err(Failure::Invalid);
    }
    let mut visited = 0;
    for work in works {
        validate(&work.csl, &guard, &mut visited)?;
    }
    encode(&(works, cites), &guard)?; // Bound serialization before cloning any metadata.
    let mut ids = HashMap::new();
    let mut items = Vec::new();
    for work in works {
        guard.check()?;
        if let Some(&(_, previous)) = ids.get(&work.identity) {
            if previous != &work.csl {
                return Err(Failure::Invalid);
            }
            continue;
        }
        let index = items.len();
        let mut item = work.csl.clone();
        item["id"] = json!(format!("work-{index}"));
        items.push(item);
        ids.insert(&work.identity, (index, &work.csl));
    }
    let occurrences = cites
        .iter()
        .map(|cite| {
            guard.check()?;
            let index = ids.get(&cite.identity).ok_or(Failure::Invalid)?.0;
            let intent = &cite.intent;
            let mut item = json!({"id":format!("work-{index}"), "suppress-author":intent.suppress_author});
            if let Some(locator) = &intent.locator {
                if locator.value.trim().is_empty() {
                    return Err(Failure::Invalid);
                }
                item["locator"] = json!(locator.value);
                item["label"] = json!(locator.label);
            }
            if let Some(prefix) = &intent.prefix {
                item["prefix"] = json!(prefix);
            }
            if let Some(suffix) = &intent.suffix {
                item["suffix"] = json!(suffix);
            }
            Ok(item)
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let output = match output {
        Output::Text => "text",
        Output::Html => "html",
        Output::Rtf => "rtf",
    };
    let payload = encode(
        &json!({"items":items,"cites":occurrences,"format":output,"bibliography":bibliography}),
        &guard,
    )?;
    let style = match style {
        CitationStyle::Apa => include_str!("../../../apps/desktop/vendor/csl/styles/apa.csl"),
        CitationStyle::ChicagoAuthorDate => {
            include_str!("../../../apps/desktop/vendor/csl/styles/chicago-author-date.csl")
        }
        CitationStyle::HarvardCiteThemRight => {
            include_str!("../../../apps/desktop/vendor/csl/styles/harvard-cite-them-right.csl")
        }
        CitationStyle::Ieee => include_str!("../../../apps/desktop/vendor/csl/styles/ieee.csl"),
        CitationStyle::ModernLanguageAssociation => {
            include_str!("../../../apps/desktop/vendor/csl/styles/modern-language-association.csl")
        }
    };
    execute(&payload, style, ADAPTER, guard)
}

fn execute(payload: &str, style: &str, script: &str, guard: Rc<Guard>) -> Result<String, Failure> {
    guard.check()?;
    let runtime = Runtime::new().map_err(|_| Failure::Engine)?;
    runtime.set_memory_limit(64 * LIMIT);
    runtime.set_max_stack_size(LIMIT);
    let interrupt = guard.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || interrupt.check().is_err())));
    let context = Context::full(&runtime).map_err(|_| Failure::Engine)?;
    let result = context.with(|ctx| -> rquickjs::Result<String> {
        intl::install(&ctx, guard.clone())?;
        ctx.eval::<(), _>(
            PROCESSOR
                .strip_suffix("\nexport default CSL;\n")
                .ok_or(rquickjs::Error::Unknown)?,
        )?;
        ctx.globals().set("ibisInput", payload)?;
        ctx.globals().set("ibisStyle", style)?;
        ctx.globals().set("ibisLocale", LOCALE)?;
        let diagnostics = guard.clone();
        ctx.globals()
            .set("ibisDiagnostic", Func::from(move || diagnostics.fail(Failure::Engine)))?;
        ctx.eval::<(), _>("CSL.debug = function () { ibisDiagnostic(); };")?;
        let value: JsValue = ctx.eval(script)?;
        let checked: Function = ctx.globals().get("__ibisText")?;
        let value: rquickjs::String = checked.call((value,))?;
        let value = value.to_cstring()?;
        if value.len() > LIMIT {
            guard.fail(Failure::Limit);
            return Err(rquickjs::Error::Unknown);
        }
        Ok(value.as_str().to_owned())
    });
    guard.check()?; // Native failures remain sticky even when citeproc catches them.
    result.map_err(|_| Failure::Engine)
}

const ADAPTER: &str = r#"
(() => {
    const request = JSON.parse(ibisInput);
    const items = new Map(request.items.map(item => [item.id, item]));
    const engine = new CSL.Engine({
        retrieveItem: id => items.get(id),
        retrieveLocale: tag => tag === 'en-US' || tag === 'en' ? ibisLocale : false,
    }, ibisStyle, 'en-US');
    engine.setOutputFormat(request.format);
    engine.updateItems([]);
    engine.updateItems([...items.keys()]);
    const bibliography = engine.makeBibliography();
    let output = request.bibliography
        ? bibliography[1].map(entry => entry.replace(/\s*\n\s*/g, ' ').trim()).join('\n')
        : engine.makeCitationCluster(request.cites);
    if (request.format === 'rtf') {
        if (request.bibliography) output = output.split('\n').join('\\par\n');
        output = '{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\\f0\n' + output + '\n}';
    }
    return output;
})()
"#;
