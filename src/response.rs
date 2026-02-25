use std::collections::HashMap;

use chrono::Utc;
use serde::Serialize;

use crate::error::AppError;

#[derive(Serialize)]
pub struct Response {
    pub ok: bool,
    pub schema_version: &'static str,
    pub command: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    pub next_actions: Vec<NextAction>,
}

#[derive(Serialize)]
pub struct ErrorDetail {
    pub message: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

#[derive(Serialize)]
pub struct NextAction {
    pub command: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, ParamSpec>>,
}

#[derive(Serialize)]
pub struct ParamSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

impl Response {
    fn now() -> String {
        Utc::now().to_rfc3339()
    }

    pub fn success(
        command: &str,
        result: serde_json::Value,
        next_actions: Vec<NextAction>,
    ) -> Self {
        Self {
            ok: true,
            schema_version: "wokhei.v1",
            command: command.to_string(),
            timestamp: Self::now(),
            result: Some(result),
            error: None,
            fix: None,
            next_actions,
        }
    }

    pub fn error(command: &str, err: &AppError, next_actions: Vec<NextAction>) -> Self {
        Self {
            ok: false,
            schema_version: "wokhei.v1",
            command: command.to_string(),
            timestamp: Self::now(),
            result: None,
            error: Some(ErrorDetail {
                message: err.to_string(),
                code: err.code().to_string(),
                retryable: Some(err.retryable()),
            }),
            fix: Some(err.fix()),
            next_actions,
        }
    }

    pub fn clap_error(message: String) -> Self {
        Self {
            ok: false,
            schema_version: "wokhei.v1",
            command: "unknown".to_string(),
            timestamp: Self::now(),
            result: None,
            error: Some(ErrorDetail {
                message,
                code: "INVALID_ARGS".to_string(),
                retryable: Some(false),
            }),
            fix: Some("Run `wokhei` with no arguments for the command tree".to_string()),
            next_actions: vec![NextAction {
                command: "wokhei".to_string(),
                description: "Show command tree".to_string(),
                params: None,
            }],
        }
    }

    pub fn panic_error(message: String) -> Self {
        Self {
            ok: false,
            schema_version: "wokhei.v1",
            command: "unknown".to_string(),
            timestamp: Self::now(),
            result: None,
            error: Some(ErrorDetail {
                message,
                code: "INTERNAL_ERROR".to_string(),
                retryable: Some(false),
            }),
            fix: Some("This is a bug — please report it".to_string()),
            next_actions: vec![],
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Response serialization should never fail")
    }
}

#[allow(dead_code)]
impl NextAction {
    pub fn simple(command: &str, description: &str) -> Self {
        Self {
            command: command.to_string(),
            description: description.to_string(),
            params: None,
        }
    }

    pub fn with_params(
        command: &str,
        description: &str,
        params: HashMap<String, ParamSpec>,
    ) -> Self {
        Self {
            command: command.to_string(),
            description: description.to_string(),
            params: Some(params),
        }
    }
}

#[allow(dead_code)]
impl ParamSpec {
    pub fn value(val: &str) -> Self {
        Self {
            description: None,
            value: Some(val.to_string()),
            default: None,
            enum_values: None,
        }
    }

    pub fn described(desc: &str) -> Self {
        Self {
            description: Some(desc.to_string()),
            value: None,
            default: None,
            enum_values: None,
        }
    }

    pub fn with_default(desc: &str, default: &str) -> Self {
        Self {
            description: Some(desc.to_string()),
            value: None,
            default: Some(default.to_string()),
            enum_values: None,
        }
    }
}
