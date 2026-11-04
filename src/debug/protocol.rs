//! Debug Adapter Protocol framing.
//!
//! DAP messages are JSON objects preceded by `Content-Length` headers, the same
//! framing as LSP. This module owns the byte-level part so the session code only
//! deals with values.

use std::io::{BufRead, Write};

use serde_json::Value;

/// The largest message body Kemori will accept from an adapter.
const MAX_BODY: usize = 8 * 1024 * 1024;

/// Write one DAP message.
pub fn write_message(writer: &mut impl Write, value: &Value) -> std::io::Result<()> {
    let body = value.to_string();
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()
}

/// Read one DAP message, or `None` at a clean end of stream.
///
/// Header lines are matched case-insensitively and an oversized body is
/// rejected instead of allocating without bound.
pub fn read_message(reader: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut length = None;
    let mut header_bytes = 0usize;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        header_bytes += read;
        if header_bytes > 8192 {
            return Err(std::io::Error::other("DAP header is too large"));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let length = length.ok_or_else(|| std::io::Error::other("DAP message has no length"))?;
    if length > MAX_BODY {
        return Err(std::io::Error::other("DAP message is too large"));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|err| std::io::Error::other(format!("invalid DAP JSON: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_round_trip() {
        let value = json!({ "seq": 1, "type": "request", "command": "initialize" });
        let mut bytes = Vec::new();
        write_message(&mut bytes, &value).unwrap();
        let mut reader = std::io::BufReader::new(bytes.as_slice());
        assert_eq!(read_message(&mut reader).unwrap(), Some(value));
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn accepts_lowercase_headers() {
        let body = r#"{"type":"event","event":"initialized"}"#;
        let raw = format!("content-length: {}\r\n\r\n{body}", body.len());
        let mut reader = std::io::BufReader::new(raw.as_bytes());
        assert_eq!(
            read_message(&mut reader).unwrap().unwrap()["event"],
            "initialized"
        );
    }

    #[test]
    fn rejects_a_message_without_a_length() {
        let mut reader = std::io::BufReader::new(b"X: 1\r\n\r\n{}".as_slice());
        assert!(read_message(&mut reader).is_err());
    }
}
