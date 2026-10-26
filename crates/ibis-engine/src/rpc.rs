//! JSON-RPC message semantics. Only fixed diagnostics leave the dispatcher.
use crate::{Method, protocol};
use ibis_core::contract::{EngineError, ErrorCode};
use serde_json::{Number, Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequestId {
    Null,
    Number(Number),
    String(String),
}
impl RequestId {
    pub fn parse(value: &Value) -> Option<Self> {
        match value {
            Value::Null => Some(Self::Null),
            Value::String(value) if value.len() <= 1024 => Some(Self::String(value.clone())),
            Value::Number(value) => {
                let mut canonical = value.clone();
                if !value.is_i64() && !value.is_u64() {
                    let number = value.as_f64()?;
                    if number.fract() == 0.0 {
                        if number >= i64::MIN as f64 && number < i64::MAX as f64 {
                            canonical = Number::from(number as i64);
                        } else if number >= 0.0 && number < u64::MAX as f64 {
                            canonical = Number::from(number as u64);
                        }
                    }
                }
                Some(Self::Number(canonical))
            }
            _ => None,
        }
    }
    pub fn value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Number(value) => Value::Number(value.clone()),
            Self::String(value) => Value::String(value.clone()),
        }
    }
}

pub struct Request<'a> {
    pub id: Option<RequestId>,
    pub method: Method,
    pub params: &'a Value,
}
pub enum Message<'a> {
    Request(Request<'a>),
    Cancel(RequestId),
    Reply(Value),
    Ignore,
}

/// Borrowing avoids taking recursive ownership of values rejected at this boundary.
pub fn parse_message(message: &Value) -> Message<'_> {
    if protocol::validate_shape(message).is_err() {
        return Message::Reply(invalid_request(None));
    }
    let Some(object) = message.as_object() else {
        return Message::Reply(invalid_request(None));
    };
    let id = match object.get("id") {
        None => None,
        Some(value) => match RequestId::parse(value) {
            Some(id) => Some(id),
            None => return Message::Reply(invalid_request(None)),
        },
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Message::Reply(invalid_request(id.as_ref()));
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Message::Reply(invalid_request(id.as_ref()));
    };
    let params = object.get("params").unwrap_or(&Value::Null);
    if !(params.is_null() || params.is_object() || params.is_array()) {
        return if id.is_some() {
            Message::Reply(failure(
                id.as_ref(),
                EngineError {
                    code: ErrorCode::BadInput,
                },
            ))
        } else {
            Message::Ignore
        };
    }
    if method == "$/cancelRequest" {
        if id.is_some() {
            return Message::Reply(invalid_request(id.as_ref()));
        }
        return params
            .get("id")
            .and_then(RequestId::parse)
            .map_or(Message::Ignore, Message::Cancel);
    }
    match Method::parse(method) {
        Ok(method) => Message::Request(Request { id, method, params }),
        Err(_) if id.is_none() => Message::Ignore,
        Err(_) => Message::Reply(failure(id.as_ref(), EngineError { code: ErrorCode::Usage })),
    }
}

pub fn success(id: &RequestId, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id.value(),"result":result})
}
pub fn failure(id: Option<&RequestId>, error: EngineError) -> Value {
    let (code, message) = match error.code {
        ErrorCode::Usage => (-32601, "Method not found"),
        ErrorCode::BadInput => (-32602, "Invalid parameters"),
        ErrorCode::NotFound => (-32004, "Item or library not found"),
        ErrorCode::SourceUnavailable => (-32005, "Source unavailable"),
        ErrorCode::NewerFormat => (-32006, "Newer format is not supported"),
        ErrorCode::Internal => (-32603, "Internal error"),
        ErrorCode::Cancelled => (-32800, "Request cancelled"),
    };
    error_response(id, code, message, error.code)
}
pub fn invalid_request(id: Option<&RequestId>) -> Value {
    error_response(id, -32600, "Invalid request", ErrorCode::BadInput)
}
pub fn parse_error() -> Value {
    error_response(None, -32700, "Parse error", ErrorCode::BadInput)
}
pub fn busy(id: &RequestId) -> Value {
    error_response(Some(id), -32000, "Server busy", ErrorCode::SourceUnavailable)
}
fn error_response(id: Option<&RequestId>, code: i32, message: &'static str, ibis_code: ErrorCode) -> Value {
    json!({"jsonrpc":"2.0","id":id.map(RequestId::value).unwrap_or(Value::Null),"error":{"code":code,"message":message,"data":{"code":ibis_code}}})
}
