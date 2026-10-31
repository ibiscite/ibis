use crate::{FormatError, Limits, input::Prepared, output, service::Control};
use rquickjs::{Context, Module, Runtime};
use std::collections::BTreeMap;

const PROCESSOR: &str = include_str!("../vendor/citeproc.mjs");
const EN_US: &str = include_str!("../vendor/locales/en-US.xml");
const STYLE_XML: [(&str, &str); 5] = [
    ("apa", include_str!("../vendor/styles/apa.csl")),
    (
        "chicago-author-date",
        include_str!("../vendor/styles/chicago-author-date.csl"),
    ),
    (
        "harvard-cite-them-right",
        include_str!("../vendor/styles/harvard-cite-them-right.csl"),
    ),
    ("ieee", include_str!("../vendor/styles/ieee.csl")),
    (
        "modern-language-association",
        include_str!("../vendor/styles/modern-language-association.csl"),
    ),
];

pub(crate) fn style_xml() -> &'static [(&'static str, &'static str)] {
    &STYLE_XML
}

pub(crate) struct Session {
    context: Context,
    runtime: Runtime,
    superscripts: BTreeMap<char, String>,
}
impl Session {
    pub(crate) fn format(
        session: &mut Option<Self>,
        prepared: &Prepared,
        limits: &Limits,
        control: &Control,
    ) -> Result<ibis_core::contract::CitationResult, FormatError> {
        let reused = session.is_some();
        if session.is_none() {
            *session = Some(Self::new(limits, control)?);
        }
        let result = session
            .as_mut()
            .ok_or(FormatError::Runtime)?
            .run(prepared, limits, control);
        if reused && matches!(result, Err(FormatError::ResourceLimit)) && control.error().is_none() {
            // A warm session may have less headroom than a fresh one. Retry once
            // within the original cancellation/deadline and the same hard limits.
            *session = None;
            *session = Some(Self::new(limits, control)?);
            return session
                .as_mut()
                .ok_or(FormatError::Runtime)?
                .run(prepared, limits, control);
        }
        result
    }

    pub(crate) fn new(limits: &Limits, control: &Control) -> Result<Self, FormatError> {
        let runtime = Runtime::new().map_err(|_| FormatError::ResourceLimit)?;
        runtime.set_memory_limit(limits.memory_bytes);
        runtime.set_max_stack_size(limits.stack_bytes);
        install_interrupt(&runtime, control);
        let context = Context::full(&runtime).map_err(|_| FormatError::ResourceLimit)?;
        let superscripts = context
            .with(|ctx| {
                let initialized = (|| {
                    crate::locale::install(&ctx)?;
                    let (module, promise) = Module::declare(ctx.clone(), "citeproc", PROCESSOR)?.eval()?;
                    promise.finish::<()>()?;
                    ctx.globals()
                        .set("CSL", module.get::<_, rquickjs::Object>("default")?)?;
                    ctx.globals().set(
                        "__ibisStyles",
                        ctx.json_parse(serde_json::to_string(&STYLE_XML).map_err(|_| rquickjs::Error::Unknown)?)?,
                    )?;
                    ctx.globals().set("__ibisLocale", EN_US)?;
                    ctx.eval::<(), _>(include_str!("bridge.js"))?;
                    // Snapshot trusted presentation data before any reference is supplied.
                    let table: String = ctx.eval("JSON.stringify(CSL.SUPERSCRIPTS)")?;
                    serde_json::from_str::<BTreeMap<char, String>>(&table).map_err(|_| rquickjs::Error::Unknown)
                })();
                initialized.map_err(|error| failure(&ctx, "bootstrap", &error))
            })
            .map_err(|error| control.error().unwrap_or(error.public()))?;
        Ok(Self {
            context,
            runtime,
            superscripts,
        })
    }
    pub(crate) fn run(
        &mut self,
        prepared: &Prepared,
        limits: &Limits,
        control: &Control,
    ) -> Result<ibis_core::contract::CitationResult, FormatError> {
        if prepared.empty {
            return Ok(output::refused(prepared));
        }
        install_interrupt(&self.runtime, control);
        let json = self
            .render(&prepared.json)
            .map_err(|error| control.error().unwrap_or(error.public()))?;
        if let Some(error) = control.error() {
            return Err(error);
        }
        if json.len() > limits.output_bytes {
            return Err(FormatError::ResourceLimit);
        }
        output::result(&json, prepared, &self.superscripts).inspect_err(|_| {
            #[cfg(test)]
            eprintln!("CSL failure phase=output class=validation");
        })
    }
    fn render(&self, json: &str) -> Result<String, JsFailure> {
        self.context.with(|ctx| {
            let rendered = (|| {
                let function: rquickjs::Function = ctx.globals().get("__ibisFormat")?;
                let value = ctx.json_parse(json.as_bytes())?;
                function.call::<_, String>((value,))
            })();
            rendered.map_err(|error| failure(&ctx, "render", &error))
        })
    }
}
/// Only fixed classes cross the JavaScript boundary. Exception content is discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsFailure {
    StackLimit,
    MemoryLimit,
    Interrupted,
    BridgeRefusal,
    TypeError,
    Other,
}
impl JsFailure {
    fn public(self) -> FormatError {
        match self {
            Self::StackLimit | Self::MemoryLimit => FormatError::ResourceLimit,
            Self::Interrupted => FormatError::Cancelled,
            Self::BridgeRefusal | Self::TypeError | Self::Other => FormatError::Runtime,
        }
    }
    #[cfg(test)]
    fn label(self) -> &'static str {
        match self {
            Self::StackLimit => "stack-limit",
            Self::MemoryLimit => "memory-limit",
            Self::Interrupted => "interrupted",
            Self::BridgeRefusal => "bridge-refusal",
            Self::TypeError => "type-error",
            Self::Other => "exception",
        }
    }
}

fn failure(ctx: &rquickjs::Ctx<'_>, phase: &'static str, error: &rquickjs::Error) -> JsFailure {
    let class = if matches!(error, rquickjs::Error::Allocation) {
        JsFailure::MemoryLimit
    } else if matches!(error, rquickjs::Error::Exception) {
        let exception = ctx.catch();
        // QuickJS throws null if even constructing its out-of-memory Error fails.
        if exception.is_null() {
            JsFailure::MemoryLimit
        } else {
            let object = exception.as_object();
            let name = object.and_then(|value| value.get::<_, String>("name").ok());
            let message = object.and_then(|value| value.get::<_, String>("message").ok());
            match (name.as_deref(), message.as_deref()) {
                (Some("RangeError"), Some("Maximum call stack size exceeded")) => JsFailure::StackLimit,
                (Some("InternalError"), Some(message)) if message.starts_with("stack overflow (op=") => {
                    JsFailure::StackLimit
                }
                (Some("InternalError"), Some("out of memory" | "out of memory in regexp execution")) => {
                    JsFailure::MemoryLimit
                }
                (Some("InternalError"), Some("interrupted")) => JsFailure::Interrupted,
                (Some("Error"), Some("citation formatting failed")) => JsFailure::BridgeRefusal,
                (Some("TypeError"), _) => JsFailure::TypeError,
                _ => JsFailure::Other,
            }
        }
    } else {
        JsFailure::Other
    };
    #[cfg(test)]
    eprintln!("CSL failure phase={phase} class={}", class.label());
    #[cfg(not(test))]
    let _ = phase;
    class
}

fn install_interrupt(runtime: &Runtime, control: &Control) {
    let control = control.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || control.error().is_some())));
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::{Duration, Instant};
    fn control() -> Control {
        Control {
            cancel: Arc::new(AtomicBool::new(false)),
            stopping: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_secs(5),
        }
    }
    fn synthetic(count: usize, bibliography: bool) -> Prepared {
        let items: Vec<_> = (0..count).map(|index| serde_json::json!({"id":format!("ibis-{index}"), "type":"book", "title":format!("Synthetic title {index}"), "author":[{"family":if index==0 {"Taylor".into()} else {format!("Invented{index}")},"given":"Avery"}], "issued":{"date-parts":[[2026]]}, "publisher":"Imaginary Press"})).collect();
        let cites: Vec<_> = (0..count)
            .map(|index| serde_json::json!({"id":format!("ibis-{index}")}))
            .collect();
        Prepared {json:serde_json::json!({"style":"apa","items":items,"cites":cites,"bibliography":bibliography,"keep_order":false,"locale":null}).to_string(), outcomes:(0..count).map(|item|ibis_core::contract::ItemOutcome::Rendered {item}).collect(),losses:vec![],notices:vec![],warnings:vec![],empty:false}
    }

    fn on_worker_stack(work: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(crate::WORKER_STACK_BYTES)
            .spawn(work)
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn bibliography_strips_numbers_through_shared_macros_without_changing_citations() {
        on_worker_stack(|| {
            let session = Session::new(&Limits::default(), &control()).unwrap();
            session.context.with(|ctx| {
                ctx.eval::<(), _>(r#"
                    __ibisStyles.push(["synthetic-macro-number", `<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><category citation-format="numeric"/></info>
                      <macro name="n"><text variable="citation-number" prefix="[" suffix="]"/></macro>
                      <macro name="entry"><text macro="n"/><text variable="title"/></macro>
                      <citation><layout><text macro="n"/></layout></citation>
                      <bibliography><layout><text macro="entry"/><text macro="n"/></layout></bibliography></style>`]);
                "#).unwrap();
                // Rebuild the adapter's style map with the synthetic fixture.
                ctx.eval::<(), _>(include_str!("bridge.js")).unwrap();
            });
            for bibliography in [true, false, true] {
                let request = serde_json::json!({"style":"synthetic-macro-number","numeric":true,"items":[{"id":"ibis-0","type":"book","title":"Synthetic signals"}],"cites":[{"id":"ibis-0"}],"bibliography":bibliography,"keep_order":false,"locale":null});
                let output: serde_json::Value =
                    serde_json::from_str(&session.render(&request.to_string()).unwrap()).unwrap();
                assert_eq!(output["status"], "rendered");
                let expected = if bibliography { "Synthetic signals" } else { "[1]" };
                assert_eq!(output["text"], expected);
                assert_eq!(output["html"].as_str().unwrap().contains("[1]"), !bibliography);
                assert_eq!(output["rtf"].as_str().unwrap().contains("[1]"), !bibliography);
            }
        });
    }

    #[test]
    fn year_only_checks_each_items_rendered_year_and_refuses_locator_only_branches() {
        on_worker_stack(|| {
            let limits = Limits::default();
            let control = control();
            let mut session = Session::new(&limits, &control).unwrap();
            session.context.with(|ctx| {
                ctx.eval::<(), _>(r#"
                    __ibisStyles.push(["synthetic-conditional-year", `<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><citation><layout delimiter="; "><choose>
                      <if type="book"><date variable="issued"><date-part name="year"/></date></if>
                      <else><label variable="locator" form="short" suffix=" "/><text variable="locator"/></else>
                    </choose></layout></citation></style>`]);
                "#).unwrap();
                ctx.eval::<(), _>(include_str!("bridge.js")).unwrap();
            });
            let mut prepared = synthetic(1, false);
            let mut request: serde_json::Value = serde_json::from_str(&prepared.json).unwrap();
            request["style"] = serde_json::json!("synthetic-conditional-year");
            request["cites"][0]["suppress-author"] = serde_json::json!(true);
            request["cites"][0]["year-only"] = serde_json::json!(true);
            request["cites"][0]["locator"] = serde_json::json!("42");
            // A matching branch still prints the actual year.
            let output: serde_json::Value =
                serde_json::from_str(&session.render(&request.to_string()).unwrap()).unwrap();
            assert_eq!(output["text"], "2026");
            request["items"][0]["type"] = serde_json::json!("article-journal");
            for locator in ["42", "2026"] {
                request["cites"][0]["locator"] = serde_json::json!(locator);
                prepared.json = request.to_string();
                let result = session.run(&prepared, &limits, &control).unwrap();
                assert!(result.text.is_empty());
                assert_eq!(
                    result.outcomes,
                    vec![ibis_core::contract::ItemOutcome::Refused {
                        item: 0,
                        error: ibis_core::contract::CitationError::UnsupportedForm,
                    }]
                );
            }
            // A neighbouring item's year must not cover the missing year.
            request["items"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"id":"ibis-1","type":"book","issued":{"date-parts":[[2026]]}}));
            request["cites"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"id":"ibis-1","suppress-author":true,"year-only":true}));
            let output: serde_json::Value =
                serde_json::from_str(&session.render(&request.to_string()).unwrap()).unwrap();
            assert_eq!(output["status"], "refused");
        });
    }

    #[test]
    fn year_only_accepts_the_styles_short_year_and_no_date_term() {
        on_worker_stack(|| {
            let session = Session::new(&Limits::default(), &control()).unwrap();
            session.context.with(|ctx| {
                ctx.eval::<(), _>(r#"
                    __ibisStyles.push(["synthetic-short-year", `<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text">
                      <locale xml:lang="en-US"><terms><term name="no date" form="short">u.d.</term></terms></locale>
                      <citation><layout><choose>
                        <if variable="issued"><date variable="issued"><date-part name="year" form="short"/></date></if>
                        <else><text term="no date" form="short" strip-periods="true" text-case="uppercase"/></else>
                      </choose></layout></citation></style>`]);
                "#).unwrap();
                ctx.eval::<(), _>(include_str!("bridge.js")).unwrap();
            });
            let mut request: serde_json::Value = serde_json::from_str(&synthetic(1, false).json).unwrap();
            request["style"] = serde_json::json!("synthetic-short-year");
            request["cites"][0]["suppress-author"] = serde_json::json!(true);
            request["cites"][0]["year-only"] = serde_json::json!(true);
            for dated in [true, false, true] {
                if dated {
                    request["items"][0]["issued"] = serde_json::json!({"date-parts":[[2026]]});
                } else {
                    request["items"][0].as_object_mut().unwrap().remove("issued");
                }
                let output: serde_json::Value =
                    serde_json::from_str(&session.render(&request.to_string()).unwrap()).unwrap();
                assert_eq!(output["status"], "rendered");
                assert_eq!(output["text"], if dated { "26" } else { "UD" });
            }
        });
    }

    #[test]
    fn interrupts_unbounded_script_and_fresh_runtime_recovers() {
        on_worker_stack(|| {
            let limits = Limits::default();
            let mut control = control();
            let session = Session::new(&limits, &control).unwrap();
            control.deadline = Instant::now() + Duration::from_millis(5);
            install_interrupt(&session.runtime, &control);
            let interrupted = session.context.with(|ctx| {
                let error = ctx.eval::<(), _>("while(true){} ").unwrap_err();
                failure(&ctx, "interrupt-test", &error)
            });
            assert_eq!(interrupted, JsFailure::Interrupted);
            assert!(matches!(control.error(), Some(FormatError::TimedOut)));
            drop(session);
            let fresh = Session::new(&limits, &super::tests::control()).unwrap();
            assert_eq!(fresh.context.with(|ctx| ctx.eval::<i32, _>("1+1")).unwrap(), 2);
        });
    }

    #[test]
    fn warm_resource_limit_retries_once_on_a_fresh_session() {
        on_worker_stack(|| {
            let limits = Limits::default();
            let control = control();
            let warm = Session::new(&limits, &control).unwrap();
            warm.context.with(|ctx| {
                ctx.eval::<(), _>("globalThis.__ibisFormat = () => { const error = Error('out of memory'); error.name = 'InternalError'; throw error; };").unwrap();
            });
            let mut session = Some(warm);
            assert!(
                Session::format(&mut session, &synthetic(1, false), &limits, &control)
                    .unwrap()
                    .copy_content(1)
                    .is_some()
            );

            // The fresh attempt keeps the same finite output budget; it cannot
            // turn a genuinely oversized result into an unbounded retry loop.
            let tiny = Limits {
                output_bytes: 1,
                ..limits
            };
            assert!(matches!(
                Session::format(&mut session, &synthetic(1, false), &tiny, &control),
                Err(FormatError::ResourceLimit)
            ));
        });
    }

    #[test]
    fn empty_member_returns_a_typed_refusal_instead_of_processor_placeholder() {
        on_worker_stack(|| {
            let limits = Limits::default();
            let control = control();
            let mut session = Session::new(&limits, &control).unwrap();
            let prepared=Prepared {json:serde_json::json!({"style":"apa","items":[{"id":"ibis-0","type":"book","title":"Synthetic title","author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]}}],"cites":[{"id":"ibis-0","author-only":true,"suppress-author":true}],"bibliography":false,"keep_order":false,"locale":null}).to_string(),outcomes:vec![ibis_core::contract::ItemOutcome::Rendered {item:0}],losses:vec![],notices:vec![],warnings:vec![],empty:false};
            let refused = session.run(&prepared, &limits, &control).unwrap();
            assert_eq!(
                refused.outcomes,
                vec![ibis_core::contract::ItemOutcome::Refused {
                    item: 0,
                    error: ibis_core::contract::CitationError::UnsupportedForm
                }]
            );
            assert!(refused.copy_content(1).is_none());
        });
    }

    #[test]
    fn measured_software_stack_budget_on_synthetic_apa() {
        use std::io::Write;
        on_worker_stack(|| {
            let mut minimum = 0;
            let mut default_succeeded = false;
            for kib in [64, 128, 256, 384, 512, 768, 1024, 2048] {
                let limits = Limits {
                    stack_bytes: kib * 1024,
                    ..Limits::default()
                };
                assert!(limits.valid());
                let outcome = match Session::new(&limits, &control()) {
                    Ok(session) => match session.render(&synthetic(1, false).json) {
                        Ok(json) => {
                            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
                            assert!(value["text"] == "(Taylor, 2026)", "synthetic probe output changed");
                            if minimum == 0 {
                                minimum = kib;
                            }
                            default_succeeded |= limits.stack_bytes == Limits::default().stack_bytes;
                            "success"
                        }
                        Err(error) => error.label(),
                    },
                    Err(_) => "bootstrap-refused",
                };
                writeln!(
                    std::io::stdout(),
                    "CSL stack probe {}: budget-kib={kib} result={outcome}",
                    std::env::consts::OS
                )
                .unwrap();
            }
            // Use a separate minimal context: the recursion probe has no formatter or user data.
            let runtime = Runtime::new().unwrap();
            runtime.set_max_stack_size(Limits::default().stack_bytes);
            install_interrupt(&runtime, &control());
            let context = Context::full(&runtime).unwrap();
            let depth = context.with(|ctx| {
                let error = ctx
                    .eval::<(), _>("globalThis.__ibisProbeDepth=0;(function recur(){__ibisProbeDepth++;recur()})()")
                    .unwrap_err();
                assert_eq!(failure(&ctx, "recursion-probe", &error), JsFailure::StackLimit);
                ctx.globals().get::<_, usize>("__ibisProbeDepth").unwrap()
            });
            writeln!(
                std::io::stdout(),
                "CSL stack probe {}: minimum-success-kib={minimum} default-recursion-depth={depth}",
                std::env::consts::OS
            )
            .unwrap();
            assert!(
                minimum > 0,
                "no validated software stack budget formatted synthetic APA"
            );
            assert!(default_succeeded, "default stack budget did not format synthetic APA");
        });

        // One representative large request, with finite software/native headroom.
        on_worker_stack(|| {
            let limits = Limits::default();
            let mut control = control();
            control.deadline = Instant::now() + Duration::from_secs(30);
            let mut session = Session::new(&limits, &control).unwrap();
            assert!(
                session
                    .run(&synthetic(100, true), &limits, &control)
                    .unwrap()
                    .copy_content(100)
                    .is_some()
            );
        });
    }

    #[test]
    fn trusted_processor_error_forwarding_preserves_resource_classes() {
        on_worker_stack(|| {
            let session = Session::new(&Limits::default(), &control()).unwrap();
            let class = session.context.with(|ctx| {
                let error = ctx.eval::<(), _>("null.invalid()").unwrap_err();
                failure(&ctx, "type-test", &error)
            });
            assert_eq!(class, JsFailure::TypeError);
            for (message, expected) in [
                (
                    "@macro attribute: RangeError: Maximum call stack size exceeded",
                    JsFailure::StackLimit,
                ),
                ("@macro attribute: InternalError: out of memory", JsFailure::MemoryLimit),
                (
                    "Synthetic title RangeError: Maximum call stack size exceeded",
                    JsFailure::BridgeRefusal,
                ),
                (
                    "@macro attribute: RangeError: Maximum call stack size exceeded and more",
                    JsFailure::BridgeRefusal,
                ),
            ] {
                let actual = session.context.with(|ctx| {
                    ctx.globals().set("__ibisProbeMessage", message).unwrap();
                    let error = ctx.eval::<(), _>("CSL.error(__ibisProbeMessage)").unwrap_err();
                    failure(&ctx, "forwarding-test", &error)
                });
                assert_eq!(actual, expected);
            }
        });
    }

    #[test]
    fn javascript_resource_exhaustion_has_a_resource_error() {
        on_worker_stack(|| {
            for script in [
                "globalThis.__ibisFormat=()=>{new ArrayBuffer(8*1024*1024);return 'unreachable';}",
                "globalThis.__ibisFormat=()=>{function recurse(){recurse()}recurse();}",
            ] {
                let limits = Limits::default();
                let control = control();
                let mut session = Session::new(&limits, &control).unwrap();
                session.context.with(|ctx| ctx.eval::<(), _>(script)).unwrap();
                let used = session.runtime.memory_usage().malloc_size as usize;
                session.runtime.set_memory_limit(used + 1024 * 1024);
                let prepared = Prepared {
                    json: "{}".into(),
                    outcomes: vec![],
                    losses: vec![],
                    notices: vec![],
                    warnings: vec![],
                    empty: false,
                };
                assert!(matches!(
                    session.run(&prepared, &limits, &control),
                    Err(FormatError::ResourceLimit)
                ));
            }
        });
    }
}
