//! Versioned JSON responses and stable process outcomes. Diagnostics are codes,
//! never an arbitrary error string that might disclose source data.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Literal wire constants keep contradictory envelopes unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version;
impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if u8::deserialize(deserializer)? == 1 {
            Ok(Self)
        } else {
            Err(de::Error::custom("unsupported ibis envelope version"))
        }
    }
}
impl JsonSchema for Version {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EnvelopeVersion".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "integer", "const": 1})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status<const OK: bool>;
impl<const OK: bool> Serialize for Status<OK> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(OK)
    }
}
impl<'de, const OK: bool> Deserialize<'de> for Status<OK> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if bool::deserialize(deserializer)? == OK {
            Ok(Self)
        } else {
            Err(de::Error::custom("contradictory ibis envelope status"))
        }
    }
}
impl<const OK: bool> JsonSchema for Status<OK> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        if OK {
            "SuccessStatus".into()
        } else {
            "FailureStatus".into()
        }
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "boolean", "const": OK})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Success<T> {
    ibis: Version,
    ok: Status<true>,
    pub data: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    ibis: Version,
    ok: Status<false>,
    pub error: EngineError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Envelope<T> {
    Success(Success<T>),
    Failure(Failure),
}

impl<T> Envelope<T> {
    pub fn success(data: T) -> Self {
        Self::Success(Success {
            ibis: Version,
            ok: Status,
            data,
        })
    }
    pub fn failure(code: ErrorCode) -> Self {
        Self::Failure(Failure {
            ibis: Version,
            ok: Status,
            error: EngineError { code },
        })
    }
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Success(value) => Some(&value.data),
            Self::Failure(_) => None,
        }
    }
    pub fn error(&self) -> Option<&EngineError> {
        match self {
            Self::Failure(value) => Some(&value.error),
            Self::Success(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    Usage,
    BadInput,
    NotFound,
    SourceUnavailable,
    ResourceLimit,
    NewerFormat,
    Internal,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineError {
    pub code: ErrorCode,
}

/// Values are the public process contract; JSON-RPC uses ErrorCode instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
#[repr(u8)]
pub enum ExitCode {
    Ok = 0,
    Findings = 1,
    Usage = 2,
    BadInput = 3,
    NotFound = 4,
    SourceUnavailable = 5,
    NewerFormat = 6,
    Internal = 70,
    Cancelled = 130,
}
impl ExitCode {
    pub const ALL: [Self; 9] = [
        Self::Ok,
        Self::Findings,
        Self::Usage,
        Self::BadInput,
        Self::NotFound,
        Self::SourceUnavailable,
        Self::NewerFormat,
        Self::Internal,
        Self::Cancelled,
    ];
    pub const fn value(self) -> u8 {
        self as u8
    }
}
impl ErrorCode {
    pub const ALL: [Self; 8] = [
        Self::Usage,
        Self::BadInput,
        Self::NotFound,
        Self::SourceUnavailable,
        Self::ResourceLimit,
        Self::NewerFormat,
        Self::Internal,
        Self::Cancelled,
    ];
    pub const fn exit_code(self) -> ExitCode {
        match self {
            Self::Usage => ExitCode::Usage,
            Self::BadInput => ExitCode::BadInput,
            Self::NotFound => ExitCode::NotFound,
            Self::SourceUnavailable | Self::ResourceLimit => ExitCode::SourceUnavailable,
            Self::NewerFormat => ExitCode::NewerFormat,
            Self::Internal => ExitCode::Internal,
            Self::Cancelled => ExitCode::Cancelled,
        }
    }
}

impl From<ExitCode> for u8 {
    fn from(code: ExitCode) -> Self {
        code.value()
    }
}
impl TryFrom<u8> for ExitCode {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|code| code.value() == value)
            .ok_or("unknown ibis exit code")
    }
}
impl JsonSchema for ExitCode {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ExitCode".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "integer", "enum": [0,1,2,3,4,5,6,70,130]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn exact_envelope_and_error_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        for data in [Value::Null, json!([]), json!({"items": []}), json!("文字 العربية")] {
            let envelope = Envelope::success(data.clone());
            let wire = serde_json::to_value(&envelope)?;
            assert_eq!(wire, json!({"ibis":1,"ok":true,"data":data}));
            assert_eq!(serde_json::from_value::<Envelope<Value>>(wire)?, envelope);
        }
        for code in ErrorCode::ALL {
            let envelope = Envelope::<Value>::failure(code);
            assert_eq!(
                serde_json::from_value::<Envelope<Value>>(serde_json::to_value(&envelope)?)?,
                envelope
            );
        }
        for code in ExitCode::ALL {
            assert_eq!(serde_json::from_value::<ExitCode>(serde_json::to_value(code)?)?, code);
        }
        assert!(serde_json::from_value::<ExitCode>(json!(7)).is_err());
        assert_eq!(ExitCode::ALL.map(ExitCode::value), [0, 1, 2, 3, 4, 5, 6, 70, 130]);
        assert_eq!(
            ErrorCode::ALL.map(|code| code.exit_code().value()),
            [2, 3, 4, 5, 5, 6, 70, 130]
        );
        Ok(())
    }

    #[test]
    fn contradictory_and_future_envelopes_are_refused() {
        for wire in [
            json!({"ibis":2,"ok":true,"data":[]}),
            json!({"ibis":1,"ok":false,"data":[]}),
            json!({"ibis":1,"ok":true,"error":{"code":"internal"}}),
            json!({"ibis":1,"ok":true,"data":[],"error":{"code":"internal"}}),
            json!({"ibis":1,"ok":false,"error":{"code":"bad-input","raw":"untrusted text"}}),
            json!({"ibis":1,"ok":false,"error":{"code":"unknown"}}),
            json!({"ibis":1,"ok":true}),
        ] {
            assert!(serde_json::from_value::<Envelope<Value>>(wire).is_err());
        }
    }
}
