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
        "check",
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

    /// An engine reading one invented library file, kept while the folder lives.
    fn opened(name: &str, content: &str) -> (tempfile::TempDir, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        engine.open_file_observed(&path, &Cancellation::default()).unwrap();
        (dir, engine)
    }

    fn library(bib: &str) -> (tempfile::TempDir, Engine) {
        opened("invented.bib", bib)
    }

    #[test]
    fn a_citation_with_copy_text_answers_alike() {
        let (_dir, engine) =
            library("@book{vale2024lanterns,title={Invented Lanterns},author={Vale, Ada},year={2024}}");
        let cancel = Cancellation::default();
        let hits = engine
            .dispatch(Method::Search, &json!({"query": "lanterns"}), &cancel)
            .unwrap();
        let item = hits.data["hits"][0]["item"]["object"]["data"]["id"].clone();
        let request = json!({"items": [{"item": item}], "output": {"kind": "key", "syntax": "pandoc"}});
        assert_eq!(
            alike::<methods::Cite>(
                &engine,
                serde_json::from_value(request.clone()).unwrap(),
                request.clone(),
                &cancel
            ),
            None
        );
        let typed = engine
            .call::<methods::Cite>(serde_json::from_value(request).unwrap(), &cancel)
            .unwrap();
        assert_eq!(typed.text.as_deref(), Some("[@vale2024lanterns]"));
        assert_eq!(typed.exit_code, ibis_core::contract::ExitCode::Ok);
        assert!(!typed.source_findings);
    }

    #[test]
    fn findings_in_a_damaged_library_answer_alike() {
        let (_dir, engine) = library("@misc{one,title={Invented Lantern}}\n@misc{two,title={Missing brace}\n");
        let cancel = Cancellation::default();
        let search = SearchRequest {
            query: "lantern".into(),
            limit: 10,
            libraries: None,
            interpret_locator: false,
        };
        assert_eq!(
            alike::<methods::Search>(&engine, search.clone(), json!({"query": "lantern"}), &cancel),
            None
        );
        let typed = engine.call::<methods::Search>(search, &cancel).unwrap();
        assert_eq!(typed.exit_code, ibis_core::contract::ExitCode::Findings);
        assert!(typed.source_findings);
    }

    #[test]
    fn an_oversized_answer_is_refused_alike() {
        // About 20 MiB of titles, past the 16 MiB a result may hold.
        let title = "Invented ".repeat(56 * 1024);
        let records: Vec<Value> = (0..40)
            .map(|n| json!({"id": format!("r{n}"), "type": "book", "title": title}))
            .collect();
        let (_dir, engine) = opened("invented.json", &Value::Array(records).to_string());
        let cancel = Cancellation::default();
        let search = SearchRequest {
            query: "invented".into(),
            limit: 1000,
            libraries: None,
            interpret_locator: false,
        };
        assert_eq!(
            alike::<methods::Search>(&engine, search, json!({"query": "invented", "limit": 1000}), &cancel),
            Some(ErrorCode::ResourceLimit)
        );
    }

    /// A method's declared need is what its answer depends on: `entry` copies
    /// an original entry only a BibTeX file keeps, while `outline` answers for
    /// any record, attachments or not.
    #[test]
    fn declared_needs_match_what_answers() {
        use ibis_core::contract::ExitCode;
        use ibis_engine::SourceCapability::AttachmentText;
        let cancel = Cancellation::default();
        for (name, content, keeps_entries, has_attachment_text) in [
            ("invented.bib", "@book{one,title={Invented Lanterns}}", true, true),
            (
                "invented.json",
                r#"[{"id":"one","type":"book","title":"Invented Lanterns","citation-key":"one"}]"#,
                false,
                true,
            ),
            (
                "invented.ris",
                "TY  - BOOK\nTI  - Invented Lanterns\nER  - \n",
                false,
                false,
            ),
        ] {
            let (_dir, engine) = opened(name, content);
            let capabilities = engine.libraries().unwrap().libraries[0].capabilities.clone();
            assert_eq!(capabilities.contains(&AttachmentText), has_attachment_text, "{name}");
            let hits = engine
                .dispatch(Method::Search, &json!({"query": "lanterns"}), &cancel)
                .unwrap();
            let id = &hits.data["hits"][0]["item"]["object"]["data"]["id"];
            assert!(id.is_object(), "{name}: {}", hits.data);
            let one = json!({"item": {"by": "id", "value": id}});
            let has = |method: Method| capabilities.contains(&method.needs().capability().unwrap());
            assert_eq!(has(Method::Entry), keeps_entries, "{name}");
            let entry = engine.dispatch(Method::Entry, &one, &cancel).unwrap();
            let expected = if keeps_entries {
                ExitCode::Ok
            } else {
                ExitCode::Findings
            };
            assert_eq!(entry.exit_code, expected, "{name}");
            if !keeps_entries {
                assert_eq!(entry.data["outcome"], "refused", "{name}");
                assert_eq!(entry.data["reason"], "no-original", "{name}");
            }
            assert!(has(Method::Outline), "{name}");
            let outline = engine.dispatch(Method::Outline, &one, &cancel).unwrap();
            assert_eq!(outline.exit_code, ExitCode::Ok, "{name}");
            assert_eq!(outline.data["attachments"], json!([]), "{name}");
        }
    }
}
