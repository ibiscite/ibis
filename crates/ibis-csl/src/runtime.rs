use crate::{FormatError, Limits, input::Prepared, output, output::Formatted, service::Control};
use rquickjs::{Context, Function, Module, Runtime};
use std::collections::BTreeMap;

const PROCESSOR: &str = include_str!("../vendor/citeproc.mjs");
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
    ) -> Result<Formatted, FormatError> {
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
                        "__ibisLocaleXml",
                        Function::new(ctx.clone(), |tag: String| {
                            crate::catalog::locale_xml(&tag).ok().flatten()
                        })?,
                    )?;
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
    ) -> Result<Formatted, FormatError> {
        if prepared.empty {
            return Ok(output::refused_formatted(prepared));
        }
        install_interrupt(&self.runtime, control);
        let json = self
            .render(&prepared.json, &prepared.xml)
            .map_err(|error| control.error().unwrap_or(error.public()))?;
        if let Some(error) = control.error() {
            return Err(error);
        }
        if json.len() > limits.output_bytes {
            return Err(FormatError::ResourceLimit);
        }
        let formatted = if prepared.document {
            output::document(&json, prepared.outcomes.len()).map(Formatted::Document)
        } else {
            output::result(&json, prepared, &self.superscripts).map(|result| Formatted::Citation(Box::new(result)))
        };
        formatted.inspect_err(|_| {
            #[cfg(test)]
            eprintln!("CSL failure phase=output class=validation");
        })
    }
    fn render(&self, json: &str, xml: &str) -> Result<String, JsFailure> {
        self.context.with(|ctx| {
            let rendered = (|| {
                let function: Function = ctx.globals().get("__ibisFormat")?;
                let value = ctx.json_parse(json.as_bytes())?;
                function.call::<_, String>((value, xml))
            })();
            rendered.map_err(|error| failure(&ctx, "render", &error))
        })
    }
}
#[cfg(test)]
impl Session {
    /// A test harness's own trusted script, in this exact runtime and context.
    #[cfg(test)]
    pub(crate) fn with_context<R>(&self, control: &Control, run: impl FnOnce(rquickjs::Ctx<'_>) -> R) -> R {
        install_interrupt(&self.runtime, control);
        self.context.with(run)
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
    fn apa() -> std::sync::Arc<str> {
        crate::catalog::resolve("apa").unwrap().xml.clone()
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
            assert!(matches!(control.error(), Some(FormatError::TimedOut)));
            drop(session);
            let fresh = Session::new(&limits, &super::tests::control()).unwrap();
            assert_eq!(fresh.context.with(|ctx| ctx.eval::<i32, _>("1+1")).unwrap(), 2);
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
                    xml: apa(),
                    language: "en-US".into(),
                    outcomes: vec![],
                    losses: vec![],
                    notices: vec![],
                    warnings: vec![],
                    empty: false,
                    document: false,
                };
                assert!(matches!(
                    session.run(&prepared, &limits, &control),
                    Err(FormatError::ResourceLimit)
                ));
            }
        });
    }
}
