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
            })
            .map_err(|_| control.error().unwrap_or(WorkError::Public(FormatError::Runtime)))?;
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
            .context
            .with(|ctx| {
                let function: rquickjs::Function = ctx.globals().get("__ibisFormat")?;
                let value = ctx.json_parse(prepared.json.as_bytes())?;
                function.call::<_, String>((value,))
            })
            .map_err(|error| {
                control.error().unwrap_or({
                    WorkError::Public(if matches!(error, rquickjs::Error::Allocation) {
                        FormatError::ResourceLimit
                    } else {
                        FormatError::Runtime
                    })
                })
            })?;
        if let Some(error) = control.error() {
            return Err(error);
        }
        if json.len() > limits.output_bytes {
            return Err(FormatError::ResourceLimit.into());
        }
        output::result(&json, prepared, &self.superscripts).map_err(Into::into)
    }
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
    #[test]
    fn interrupts_unbounded_script_and_fresh_runtime_recovers() {
        let limits = Limits::default();
        let mut control = control();
        let session = Session::new(&limits, &control).unwrap();
        control.deadline = Instant::now() + Duration::from_millis(5);
        install_interrupt(&session.runtime, &control);
        assert!(session.context.with(|ctx| ctx.eval::<(), _>("while(true){} ")).is_err());
        assert!(matches!(
            control.error(),
            Some(WorkError::Public(FormatError::TimedOut))
        ));
        drop(session);
        let fresh = Session::new(&limits, &super::tests::control()).unwrap();
        assert_eq!(fresh.context.with(|ctx| ctx.eval::<i32, _>("1+1")).unwrap(), 2);
    }
    #[test]
    fn processor_empty_form_throws_instead_of_returning_a_copyable_sentinel() {
        let limits = Limits::default();
        let control = control();
        let mut session = Session::new(&limits, &control).unwrap();
        let prepared=Prepared {json:serde_json::json!({"style":"apa","items":[{"id":"ibis-0","type":"book","title":"Synthetic title","author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]}}],"cites":[{"id":"ibis-0","author-only":true,"suppress-author":true}],"bibliography":false,"keep_order":false,"locale":null}).to_string(),outcomes:vec![ibis_core::contract::ItemOutcome::Rendered {item:0}],losses:vec![],notices:vec![],empty:false};
        assert!(matches!(
            session.run(&prepared, &limits, &control),
            Err(WorkError::Public(FormatError::Runtime))
        ));
    }

    #[test]
    fn memory_and_stack_limits_refuse_before_native_failure() {
        let limits = Limits::default();
        let control = control();
        let session = Session::new(&limits, &control).unwrap();
        let used = session.runtime.memory_usage().malloc_size as usize;
        session.runtime.set_memory_limit(used + 1024 * 1024);
        assert!(
            session
                .context
                .with(|ctx| ctx.eval::<(), _>("new ArrayBuffer(8*1024*1024)"))
                .is_err()
        );
        assert!(
            session
                .context
                .with(|ctx| ctx.eval::<(), _>("(function recurse(){recurse()})() "))
                .is_err()
        );
        assert!(control.error().is_none());
    }
}
