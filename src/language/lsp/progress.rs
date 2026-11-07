//! Parse server progress notifications.

use serde_json::Value;

/// The lifecycle phase of a `$/progress` report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressKind {
    Begin,
    Report,
    End,
}

/// One server progress report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LspProgress {
    /// JSON-serialized token, used as the app-side key.
    pub token: String,
    pub kind: ProgressKind,
    pub title: String,
    pub message: String,
    pub percentage: Option<u8>,
}

/// Parse `$/progress` params.
pub fn parse(params: &Value) -> Option<LspProgress> {
    let token = params.get("token")?;
    let value = params.get("value")?;
    let kind = match value.get("kind").and_then(Value::as_str)? {
        "begin" => ProgressKind::Begin,
        "report" => ProgressKind::Report,
        "end" => ProgressKind::End,
        _ => return None,
    };
    Some(LspProgress {
        token: token.to_string(),
        kind,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        percentage: value
            .get("percentage")
            .and_then(Value::as_u64)
            .map(|value| value.min(100) as u8),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_begin_report_and_end() {
        let begin = parse(&json!({
            "token": "build",
            "value": { "kind": "begin", "title": "Indexing", "message": "starting" }
        }))
        .expect("begin");
        assert_eq!(begin.kind, ProgressKind::Begin);
        assert_eq!(begin.title, "Indexing");
        assert_eq!(begin.token, "\"build\"");

        let report = parse(&json!({
            "token": 7,
            "value": { "kind": "report", "message": "half", "percentage": 42 }
        }))
        .expect("report");
        assert_eq!(report.kind, ProgressKind::Report);
        assert_eq!(report.percentage, Some(42));

        let end = parse(&json!({ "token": 7, "value": { "kind": "end" } })).expect("end");
        assert_eq!(end.kind, ProgressKind::End);
    }

    #[test]
    fn ignores_malformed_progress() {
        assert!(parse(&json!({ "token": "x" })).is_none());
        assert!(parse(&json!({ "token": "x", "value": { "kind": "other" } })).is_none());
    }
}
