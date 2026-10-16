use crate::{Error, Result};
use futures_core::Stream;
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use std::pin::Pin;

/// A cancellable asynchronous SSE stream. Drop it to close the connection.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<SseEvent>> + Send>>;

/// A server-sent event. Unknown provider events are preserved verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub id: Option<String>,
    pub data: String,
    /// Provider's reconnect hint, in milliseconds. Streams are never reconnected automatically.
    pub retry: Option<u64>,
}
impl SseEvent {
    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_str(&self.data)?)
    }
}

pub(crate) struct Parser {
    line: Vec<u8>,
    data: Vec<String>,
    kind: Option<String>,
    id: Option<String>,
    retry: Option<u64>,
    skip_lf: bool,
    first_line: bool,
    size: usize,
    limit: usize,
}
impl Parser {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            line: Vec::new(),
            data: Vec::new(),
            kind: None,
            id: None,
            retry: None,
            skip_lf: false,
            first_line: true,
            size: 0,
            limit,
        }
    }
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>> {
        let mut events = Vec::new();
        for &b in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if b == b'\n' {
                    continue;
                }
            }
            self.size += 1;
            if self.size > self.limit {
                return Err(Error::Protocol(
                    "SSE event exceeded configured size limit".into(),
                ));
            }
            if b == b'\r' || b == b'\n' {
                self.skip_lf = b == b'\r';
                if let Some(event) = self.finish_line()? {
                    events.push(event);
                }
            } else {
                self.line.push(b);
            }
        }
        Ok(events)
    }
    fn finish_line(&mut self) -> Result<Option<SseEvent>> {
        let bytes = std::mem::take(&mut self.line);
        let raw = String::from_utf8(bytes)
            .map_err(|_| Error::Protocol("SSE contains invalid UTF-8".into()))?;
        let line = if self.first_line {
            self.first_line = false;
            raw.trim_start_matches('\u{feff}')
        } else {
            &raw
        };
        if line.is_empty() {
            self.size = 0;
            let kind = self.kind.take();
            if self.data.is_empty() {
                return Ok(None);
            }
            return Ok(Some(SseEvent {
                event: kind,
                id: self.id.clone(),
                data: std::mem::take(&mut self.data).join("\n"),
                retry: self.retry,
            }));
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => self.data.push(value.into()),
            "event" => self.kind = Some(value.into()),
            "id" if !value.contains('\0') => self.id = Some(value.into()),
            "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                self.retry = value.parse().ok()
            }
            _ => {}
        }
        Ok(None)
    }
    // The SSE protocol discards events without a terminating blank line.
}

pub(crate) fn response_stream(response: reqwest::Response, limit: usize) -> EventStream {
    Box::pin(async_stream::try_stream! {
        let mut parser = Parser::new(limit);
        let mut source = response.bytes_stream();
        while let Some(chunk) = source.next().await {
            for event in parser.feed(&chunk?)? {
                if event.data == "[DONE]" {return;}
                yield event;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_fragment_boundary_utf8_crlf_and_multiline() {
        let data = "\u{feff}: ping\r\nevent: delta\r\nid: 4\r\nretry: 25\r\ndata: {\"x\":\"🦀\"}\r\ndata: second\r\n\r\n".as_bytes();
        for boundary in 0..=data.len() {
            let mut p = Parser::new(4096);
            let mut events = p.feed(&data[..boundary]).unwrap();
            events.extend(p.feed(&data[boundary..]).unwrap());
            assert_eq!(
                events,
                vec![SseEvent {
                    event: Some("delta".into()),
                    id: Some("4".into()),
                    retry: Some(25),
                    data: "{\"x\":\"🦀\"}\nsecond".into()
                }]
            );
        }
    }
    #[test]
    fn blank_data_last_id_and_bare_cr() {
        let mut p = Parser::new(4096);
        let events = p.feed(b"id: 1\rdata:\r\rdata: x\r\r").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "");
        assert_eq!(events[1].id.as_deref(), Some("1"));
    }
    #[test]
    fn rejects_large_and_invalid_frames() {
        assert!(Parser::new(2).feed(b"data").is_err());
        assert!(Parser::new(100).feed(b"data: \xff\n\n").is_err());
    }
}
