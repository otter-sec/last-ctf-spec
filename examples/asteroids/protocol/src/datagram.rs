// what a QUIC datagram is sure to carry
const MAX_DATAGRAM: usize = 1100;
const HEADER: usize = 6;

pub fn fragment(id: u32, message: &[u8]) -> Vec<Vec<u8>> {
    let parts: Vec<&[u8]> = message.chunks(MAX_DATAGRAM - HEADER).collect();
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let mut datagram = id.to_le_bytes().to_vec();
            datagram.extend_from_slice(&[index as u8, parts.len() as u8]);
            datagram.extend_from_slice(part);
            datagram
        })
        .collect()
}

#[derive(Default)]
pub struct Reassembler {
    id: u32,
    parts: Vec<Option<Vec<u8>>>,
}

impl Reassembler {
    pub fn push(&mut self, datagram: &[u8]) -> Option<Vec<u8>> {
        let (&[a, b, c, d, index, count], part) = datagram.split_first_chunk()?;
        let id = u32::from_le_bytes([a, b, c, d]);
        if id < self.id {
            return None;
        }
        if id > self.id || self.parts.is_empty() {
            self.id = id;
            self.parts = vec![None; count as usize];
        }
        *self.parts.get_mut(index as usize)? = Some(part.to_vec());
        if self.parts.iter().any(Option::is_none) {
            return None;
        }
        self.id = id.wrapping_add(1);
        Some(self.parts.drain(..).flatten().flatten().collect())
    }
}
