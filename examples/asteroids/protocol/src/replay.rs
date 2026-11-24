use crate::messages::{ServerMessage, decode};

const MAGIC: &[u8; 8] = b"ASTRPLY5";

pub struct ReplayWriter(Vec<u8>);

impl Default for ReplayWriter {
    fn default() -> Self {
        Self(MAGIC.to_vec())
    }
}

impl ReplayWriter {
    pub fn push(&mut self, frame: &[u8]) {
        self.0.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        self.0.extend_from_slice(frame);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

pub fn read_replay(bytes: &[u8]) -> Option<Vec<ServerMessage>> {
    let mut rest = bytes.strip_prefix(MAGIC)?;
    let mut messages = Vec::new();
    while !rest.is_empty() {
        let (len, tail) = rest.split_first_chunk::<4>()?;
        let (frame, tail) = tail.split_at_checked(u32::from_le_bytes(*len) as usize)?;
        messages.push(decode(frame)?);
        rest = tail;
    }
    Some(messages)
}
