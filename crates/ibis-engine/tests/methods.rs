#![cfg(test)]
use ibis_core::contract::ErrorCode;
use ibis_engine::Method;

#[test]
fn one_method_list_drives_all_front_ends() {
    let names = [
        "libraries",
        "search",
        "show",
        "cite",
        "records",
        "project",
        "entry",
        "outline",
        "read",
        "find-in",
        "share",
        "styles",
        "outputs",
        "schema",
        "call",
    ];
    assert_eq!(Method::ALL.map(Method::as_str), names);
    for (method, name) in Method::ALL.into_iter().zip(names) {
        assert_eq!(Method::parse(name), Ok(method));
    }
    assert_eq!(Method::parse("unknown"), Err(ErrorCode::Usage));
    assert_eq!(Method::parse(""), Err(ErrorCode::Usage));
}

mod typed_calls {
    use ibis_core::contract::{ErrorCode, Snapshot};
    use ibis_engine::request::*;
    use ibis_engine::{Cancellation, DispatchResult, Engine, Method, Row, methods};
    use serde_json::{Value, json};
    use std::sync::Arc;

    fn engine() -> Engine {
        Engine::from_snapshot(Arc::new(Snapshot::new(vec![]).unwrap()))
    }

    /// A typed call and the JSON dispatch of the same request answer alike:
    /// the same data, exit code, copy text and findings, or the same refusal.
    fn alike<M: Row>(engine: &Engine, params: M::Params, json: Value, cancel: &Cancellation) -> Option<ErrorCode> {
        let typed = engine
            .call::<M>(params, cancel)
            .and_then(DispatchResult::wire)
            .map(|answer| (answer.data, answer.exit_code, answer.text, answer.source_findings));
        let dispatched = engine
            .dispatch(M::METHOD, &json, cancel)
            .map(|answer| (answer.data, answer.exit_code, answer.text, answer.source_findings));
        match (typed, dispatched) {
            (Ok(typed), Ok(dispatched)) => {
                assert_eq!(typed, dispatched, "{}", M::NAME);
                None
            }
            (Err(typed), Err(dispatched)) => {
                assert_eq!(typed.code, dispatched.code, "{}", M::NAME);
                Some(typed.code)
            }
            (typed, dispatched) => panic!("{}: {typed:?} against {dispatched:?}", M::NAME),
        }
    }

    #[test]
    fn a_typed_call_answers_as_json_dispatch_does() {
        let engine = engine();
        let cancel = Cancellation::default();
        let styles = StylesRequest {
            query: Some("apa".into()),
            limit: 5,
            offset: 0,
        };
        assert_eq!(
            alike::<methods::Styles>(&engine, styles, json!({"query": "apa", "limit": 5}), &cancel),
            None
        );
        assert_eq!(
            alike::<methods::Outputs>(&engine, OutputsRequest {}, Value::Null, &cancel),
            None
        );
        assert_eq!(
            alike::<methods::Schema>(
                &engine,
                SchemaRequest {
                    method: Some(Method::FindIn)
                },
                json!({"method": "find-in"}),
                &cancel
            ),
            None
        );
        assert_eq!(
            alike::<methods::Libraries>(&engine, LibrariesRequest::List {}, Value::Null, &cancel),
            None
        );
        let search = SearchRequest {
            query: "lanterns".into(),
            limit: 10,
            libraries: None,
            interpret_locator: false,
        };
        assert_eq!(
            alike::<methods::Search>(&engine, search, json!({"query": "lanterns"}), &cancel),
            None
        );
    }

    #[test]
    fn typed_and_json_calls_refuse_alike() {
        let engine = engine();
        let cancel = Cancellation::default();
        let too_many = StylesRequest {
            limit: 0,
            ..StylesRequest::default()
        };
        assert_eq!(
            alike::<methods::Styles>(&engine, too_many, json!({"limit": 0}), &cancel),
            Some(ErrorCode::BadInput)
        );
        let missing = ShowRequest {
            item: ItemSelector::Key("vale2024lanterns".into()),
        };
        assert_eq!(
            alike::<methods::Show>(
                &engine,
                missing,
                json!({"item": {"by": "key", "value": "vale2024lanterns"}}),
                &cancel
            ),
            Some(ErrorCode::NotFound)
        );
        let itself = CallRequest {
            method: Method::Call,
            params: Value::Null,
        };
        assert_eq!(
            alike::<methods::Call>(&engine, itself, json!({"method": "call"}), &cancel),
            Some(ErrorCode::BadInput)
        );
        let nested = CallRequest {
            method: Method::Styles,
            params: json!({"limit": "many"}),
        };
        assert_eq!(
            alike::<methods::Call>(
                &engine,
                nested,
                json!({"method": "styles", "params": {"limit": "many"}}),
                &cancel
            ),
            Some(ErrorCode::BadInput)
        );
        // Leaving the parameters out lists the libraries; an empty object names no action.
        assert_eq!(
            engine
                .dispatch(Method::Libraries, &json!({}), &cancel)
                .unwrap_err()
                .code,
            ErrorCode::BadInput
        );
    }

    #[test]
    fn a_cancelled_call_does_no_work_either_way() {
        let engine = engine();
        let cancel = Cancellation::default();
        cancel.cancel();
        assert_eq!(
            alike::<methods::Outputs>(&engine, OutputsRequest {}, Value::Null, &cancel),
            Some(ErrorCode::Cancelled)
        );
        assert_eq!(
            alike::<methods::Libraries>(&engine, LibrariesRequest::List {}, Value::Null, &cancel),
            Some(ErrorCode::Cancelled)
        );
    }
}
