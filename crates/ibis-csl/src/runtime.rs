use crate::{
    FormatError, Limits,
    input::Prepared,
    output,
    service::{Control, WorkError},
};
use rquickjs::{Context, Module, Runtime};
use std::collections::BTreeMap;

const PROCESSOR: &str = include_str!("../../../apps/desktop/vendor/csl/citeproc.mjs");
const EN_US: &str = include_str!("../../../apps/desktop/vendor/csl/locales/en-US.xml");
const STYLE_XML: [(&str, &str); 5] = [
    ("apa", include_str!("../../../apps/desktop/vendor/csl/styles/apa.csl")),
    (
        "chicago-author-date",
        include_str!("../../../apps/desktop/vendor/csl/styles/chicago-author-date.csl"),
    ),
    (
        "harvard-cite-them-right",
        include_str!("../../../apps/desktop/vendor/csl/styles/harvard-cite-them-right.csl"),
    ),
    ("ieee", include_str!("../../../apps/desktop/vendor/csl/styles/ieee.csl")),
    (
        "modern-language-association",
        include_str!("../../../apps/desktop/vendor/csl/styles/modern-language-association.csl"),
    ),
];

pub(crate) struct Session {
    context: Context,
    runtime: Runtime,
    superscripts: BTreeMap<char, String>,
}
impl Session {
    pub(crate) fn new(limits: &Limits, control: &Control) -> Result<Self, WorkError> {
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
            .map_err(|error| control.error().unwrap_or(WorkError::Public(error.public())))?;
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
    ) -> Result<ibis_core::contract::CitationResult, WorkError> {
        if prepared.empty {
            return Ok(output::refused(prepared));
        }
        install_interrupt(&self.runtime, control);
        let json = self
            .render(&prepared.json)
            .map_err(|error| control.error().unwrap_or(WorkError::Public(error.public())))?;
        if let Some(error) = control.error() {
            return Err(error);
        }
        if json.len() > limits.output_bytes {
            return Err(FormatError::ResourceLimit.into());
        }
        output::result(&json, prepared, &self.superscripts)
            .inspect_err(|_| {
                #[cfg(test)]
                eprintln!("CSL failure phase=output class=validation");
            })
            .map_err(Into::into)
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
                (
                    Some("Error"),
                    Some(
                        "citation formatting failed"
                        | "style could not format the bibliography"
                        | "style could not format the citation",
                    ),
                ) => JsFailure::BridgeRefusal,
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
            interactive: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_secs(5),
            background: false,
        }
    }
    fn synthetic(count: usize, bibliography: bool) -> Prepared {
        let items: Vec<_> = (0..count).map(|index| serde_json::json!({"id":format!("ibis-{index}"), "type":"book", "title":format!("Synthetic title {index}"), "author":[{"family":if index==0 {"Taylor".into()} else {format!("Invented{index}")},"given":"Avery"}], "issued":{"date-parts":[[2026]]}, "publisher":"Imaginary Press"})).collect();
        let cites: Vec<_> = (0..count)
            .map(|index| serde_json::json!({"id":format!("ibis-{index}")}))
            .collect();
        Prepared {json:serde_json::json!({"style":"apa","items":items,"cites":cites,"bibliography":bibliography,"keep_order":false,"locale":null}).to_string(), outcomes:(0..count).map(|item|ibis_core::contract::ItemOutcome::Rendered {item}).collect(),losses:vec![],notices:vec![],empty:false}
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
            assert!(matches!(
                control.error(),
                Some(WorkError::Public(FormatError::TimedOut))
            ));
            drop(session);
            let fresh = Session::new(&limits, &super::tests::control()).unwrap();
            assert_eq!(fresh.context.with(|ctx| ctx.eval::<i32, _>("1+1")).unwrap(), 2);
        });
    }

    #[test]
    fn processor_empty_form_throws_instead_of_returning_a_copyable_sentinel() {
        on_worker_stack(|| {
            let limits = Limits::default();
            let control = control();
            let session = Session::new(&limits, &control).unwrap();
            let prepared=Prepared {json:serde_json::json!({"style":"apa","items":[{"id":"ibis-0","type":"book","title":"Synthetic title","author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]}}],"cites":[{"id":"ibis-0","author-only":true,"suppress-author":true}],"bibliography":false,"keep_order":false,"locale":null}).to_string(),outcomes:vec![ibis_core::contract::ItemOutcome::Rendered {item:0}],losses:vec![],notices:vec![],empty:false};
            assert_eq!(session.render(&prepared.json), Err(JsFailure::BridgeRefusal));
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
                    empty: false,
                };
                assert!(matches!(
                    session.run(&prepared, &limits, &control),
                    Err(WorkError::Public(FormatError::ResourceLimit))
                ));
            }
        });
    }
}
