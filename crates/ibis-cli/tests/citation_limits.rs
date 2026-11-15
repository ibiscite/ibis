//! Contract 1 keeps each input's machine codes while sharing the limit's words.
use ibis_core::contract::GoldenCase;
use ibis_engine::{Cancellation, Engine, Method, catalogue, mcp, protocol, request::KeyCitationRequest, rpc};
use serde_json::{Value, json};
use std::{io::Cursor, sync::Arc};

#[test]
fn over_limit_inputs_preserve_contract_one_codes_across_front_ends() -> Result<(), Box<dyn std::error::Error>> {
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let count = ibis_core::syntax::MAX_CITATION_ITEMS + 1;
    let opaque = case.request.items[0].item.opaque_reference();
    case.request.items = vec![case.request.items[0].clone(); count];
    let records = json!(vec![json!({"id":"invented", "type":"book"}); count]);
    let engine = Engine::from_snapshot(Arc::new(case.library));
    let output = json!({"kind":"preset", "name":"pandoc"});

    for (input, exit, code, rpc_code, mcp_code) in [
        ("records", 5, "resource-limit", -32000, "resource-limit"),
        ("items", 5, "resource-limit", -32000, "invalid-too-many-items"),
        ("keys", 3, "bad-input", -32602, "invalid-too-many-items"),
    ] {
        let (mut args, params, bytes, mcp_arguments) = match input {
            "records" => (
                vec!["ibis".into(), "cite".into(), "--records".into()],
                json!({"records":records, "output":output}),
                serde_json::to_vec(&records)?,
                json!({"records":records, "format":"pandoc"}),
            ),
            "items" => (
                vec!["ibis".into(), "cite".into(), "--request".into()],
                serde_json::to_value(&case.request)?,
                serde_json::to_vec(&case.request)?,
                json!({"items":vec![json!({"ref":opaque}); count], "format":"pandoc"}),
            ),
            _ => {
                let mut args = vec!["ibis".into(), "cite".into()];
                args.extend(vec!["Example2026".into(); count]);
                (
                    args,
                    json!({"keys":vec![json!({"key":"Example2026"}); count], "output":output}),
                    vec![],
                    json!({"items":vec![json!({"ref":"Example2026"}); count], "format":"pandoc"}),
                )
            }
        };
        args.extend(["--json".into(), "--contract".into(), "1".into()]);
        let (mut stdout, mut stderr) = (vec![], vec![]);
        let actual_exit = ibis::run_with(
            engine.clone(),
            args,
            Cursor::new(bytes),
            &mut stdout,
            &mut stderr,
            ibis::OutputMode::Plain,
        );
        assert_eq!(actual_exit, exit, "{input}");
        let cli: Value = serde_json::from_slice(&stdout)?;
        assert_eq!(cli["ibis"], 1);
        assert_eq!(cli["error"]["code"], code, "{input}");
        assert_eq!(
            stderr, b"ibis: A citation can hold at most 1000 references.\n",
            "{input}"
        );

        let method = if input == "records" {
            Method::Records
        } else {
            Method::Cite
        };
        let error = if input == "keys" {
            let request: KeyCitationRequest = serde_json::from_value(params.clone())?;
            engine.cite_keys(&request, &Cancellation::default()).err()
        } else {
            engine.dispatch(method, &params, &Cancellation::default()).err()
        }
        .ok_or("over-limit input must fail")?;
        let rpc = if input == "keys" {
            // KeyCitationRequest has no RPC method; pin the public RPC error adapter.
            rpc::failure(None, error)
        } else {
            let mut frame = vec![];
            protocol::write_frame(
                &mut frame,
                &json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params}),
            )?;
            let mut reply = vec![];
            ibis_engine::server::serve(engine.clone(), Cursor::new(frame), &mut reply)?;
            protocol::read_response_frame(&mut Cursor::new(reply))?.ok_or("RPC reply")?
        };
        assert_eq!(rpc["error"]["code"], rpc_code, "{input}");
        assert_eq!(rpc["error"]["data"]["code"], code, "{input}");
        assert_eq!(
            catalogue::error(&error, catalogue::Audience::Cli),
            "A citation can hold at most 1000 references."
        );

        let reply = mcp::handle(
            &engine,
            &mut mcp::Session::default(),
            &json!({
                "jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{
                    "name":"cite", "arguments":mcp_arguments,
                    "_meta":{"io.modelcontextprotocol/protocolVersion":mcp::PROTOCOL_VERSION,
                             "io.modelcontextprotocol/clientCapabilities":{}}
                }
            }),
        )
        .ok_or("MCP reply")?;
        assert_eq!(reply["result"]["isError"], true);
        assert_eq!(
            reply["result"]["structuredContent"]["error"]["code"], mcp_code,
            "{input}"
        );
        let mcp_class = if input == "records" {
            "resource-limit"
        } else {
            "bad-input"
        };
        assert_eq!(
            reply["result"]["content"][0]["text"],
            format!("error: {mcp_class}. A citation can hold at most 1000 references.")
        );
    }
    Ok(())
}
