//! Minimal LSP stdio framing (`Content-Length` headers).

use serde_json::Value;
use std::io::{self, Read, Write};

pub const CONTENT_LENGTH: &str = "Content-Length";
pub const HEADER_SEP: &[u8] = b"\r\n\r\n";

pub struct LspReader<R> {
    reader: R,
}

impl<R: Read> LspReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub fn read_message(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut header_buf = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            match self.reader.read(&mut byte)? {
                0 => return Ok(None),
                _ => header_buf.push(byte[0]),
            }
            if header_buf.ends_with(HEADER_SEP) {
                break;
            }
        }

        let content_length = parse_content_length(&header_buf);
        let mut content = vec![0u8; content_length];
        self.reader.read_exact(&mut content)?;

        let mut message = header_buf;
        message.extend_from_slice(&content);
        Ok(Some(message))
    }
}

pub fn parse_content_length(header: &[u8]) -> usize {
    String::from_utf8_lossy(header)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case(CONTENT_LENGTH) {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

pub fn lsp_body(raw: &[u8]) -> Option<&[u8]> {
    let sep = raw.windows(4).position(|w| w == HEADER_SEP)?;
    Some(&raw[sep + 4..])
}

pub fn parse_lsp_content(raw: &[u8]) -> Option<Value> {
    serde_json::from_slice(lsp_body(raw)?).ok()
}

pub fn write_raw(raw: &[u8]) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(raw);
    let _ = out.flush();
}

pub fn frame_json(value: &Value) -> Option<Vec<u8>> {
    let json = serde_json::to_string(value).ok()?;
    let framed = format!("{CONTENT_LENGTH}: {}\r\n\r\n{json}", json.len());
    Some(framed.into_bytes())
}
