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
    // Stop at each dispatch so inherited metadata is copied only for the event
    // being delivered, rather than for every event in a network chunk.
    pub(crate) fn feed(&mut self, bytes: &mut &[u8]) -> Result<Option<SseEvent>> {
        while let Some((&b, rest)) = bytes.split_first() {
            *bytes = rest;
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
                    return Ok(Some(event));
                }
            } else {
                self.line.push(b);
            }
        }
        Ok(None)
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
            let chunk = chunk?;
            let mut remaining = chunk.as_ref();
            while let Some(event) = parser.feed(&mut remaining)? {
                if event.data == "[DONE]" {return;}
                yield event;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_events(parser: &mut Parser, mut bytes: &[u8]) -> Result<Vec<SseEvent>> {
        let mut events = Vec::new();
        while let Some(event) = parser.feed(&mut bytes)? {
            events.push(event);
        }
        Ok(events)
    }

    #[test]
    fn every_fragment_boundary_utf8_crlf_and_multiline() {
        let data = "\u{feff}: ping\r\nevent: delta\r\nid: 4\r\nretry: 25\r\ndata: {\"x\":\"🦀\"}\r\ndata: second\r\n\r\n".as_bytes();
        for boundary in 0..=data.len() {
            let mut p = Parser::new(4096);
            let mut events = collect_events(&mut p, &data[..boundary]).unwrap();
            events.extend(collect_events(&mut p, &data[boundary..]).unwrap());
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
        let events = collect_events(&mut p, b"id: 1\rdata:\r\rdata: x\r\r").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "");
        assert_eq!(events[1].id.as_deref(), Some("1"));
    }
    #[test]
    fn inherited_ids_are_delivered_without_batching() {
        let mut parser = Parser::new(128);
        let id = "x".repeat(120);
        assert!(parser
            .feed(&mut format!("id: {id}\n\n").as_bytes())
            .unwrap()
            .is_none());
        let batch = "data:\n\n".repeat(128);
        let mut remaining = batch.as_bytes();
        for pending in (0..128).rev() {
            let event = parser.feed(&mut remaining).unwrap().unwrap();
            assert_eq!(event.id.as_deref(), Some(id.as_str()));
            assert_eq!(event.data, "");
            // The next event's ID cannot have been copied yet.
            assert_eq!(remaining, &batch.as_bytes()[batch.len() - pending * 7..]);
        }
        assert!(parser.feed(&mut remaining).unwrap().is_none());
    }
    #[test]
    fn valid_events_are_delivered_before_invalid_or_oversized_tails() {
        for tail in [
            b"data: \xff\n\n".to_vec(),
            format!("data: {}\n\n", "x".repeat(128)).into_bytes(),
        ] {
            let mut parser = Parser::new(128);
            let mut input = b"id: inherited\n\ndata: first\n\n".to_vec();
            input.extend_from_slice(&tail);
            let mut remaining = input.as_slice();
            let event = parser.feed(&mut remaining).unwrap().unwrap();
            assert_eq!(event.id.as_deref(), Some("inherited"));
            assert_eq!(event.data, "first");
            assert_eq!(remaining, tail.as_slice());
            assert!(parser.feed(&mut remaining).is_err());
        }
    }
    #[test]
    fn metadata_updates_preserve_inheritance_and_clearing() {
        let events = collect_events(
            &mut Parser::new(128),
            b"event: unused\rid: first\rretry: 25\r\rdata:\r\rid: bad\0id\rretry: invalid\rdata: second\r\rid: replacement\rretry: 18446744073709551616\rdata: third\r\rid:\rdata: fourth\r\r",
        )
        .unwrap();
        assert_eq!(events.len(), 4);
        for event in &events[..2] {
            assert_eq!(event.id.as_deref(), Some("first"));
            assert_eq!(event.retry, Some(25));
            assert_eq!(event.event, None);
        }
        assert_eq!(events[0].data, "");
        assert_eq!(events[1].data, "second");
        assert_eq!(events[2].id.as_deref(), Some("replacement"));
        assert_eq!(events[2].retry, None);
        assert_eq!(events[3].id.as_deref(), Some(""));
        assert_eq!(events[3].retry, None);
    }
    #[test]
    fn unterminated_final_events_are_discarded() {
        let events = collect_events(
            &mut Parser::new(128),
            b"data: complete\n\ndata: incomplete\n",
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "complete");
    }
    #[test]
    fn rejects_large_and_invalid_frames() {
        assert!(Parser::new(2).feed(&mut &b"data"[..]).is_err());
        assert!(Parser::new(100).feed(&mut &b"data: \xff\n\n"[..]).is_err());
    }
}
