use std::{fs, path::Path};
use xifty_core::{SourceRef, XiftyError};

#[derive(Debug, Clone)]
pub struct SourceBytes {
    pub source: SourceRef,
    bytes: Vec<u8>,
}

impl SourceBytes {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, XiftyError> {
        let path = path.as_ref();
        let bytes = fs::read(path)?;
        let source = SourceRef {
            path: path.to_path_buf(),
            size_bytes: bytes.len() as u64,
        };
        Ok(Self { source, bytes })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    data: &'a [u8],
    base_offset: u64,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], base_offset: u64) -> Self {
        Self { data, base_offset }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.data
    }

    pub fn slice(&self, start: usize, len: usize) -> Result<&'a [u8], XiftyError> {
        self.data
            .get(start..start + len)
            .ok_or_else(|| XiftyError::Parse {
                message: format!("out of bounds slice start={start} len={len}"),
            })
    }

    pub fn subslice(&self, start: usize, len: usize) -> Result<Self, XiftyError> {
        Ok(Self {
            data: self.slice(start, len)?,
            base_offset: self.base_offset + start as u64,
        })
    }

    pub fn absolute_offset(&self, local_offset: usize) -> u64 {
        self.base_offset + local_offset as u64
    }

    pub fn read_u8(&self, offset: usize) -> Result<u8, XiftyError> {
        self.data
            .get(offset)
            .copied()
            .ok_or_else(|| XiftyError::Parse {
                message: format!("u8 read out of bounds at {offset}"),
            })
    }

    pub fn read_u16(&self, offset: usize, endian: Endian) -> Result<u16, XiftyError> {
        let bytes = self.slice(offset, 2)?;
        Ok(match endian {
            Endian::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
            Endian::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
        })
    }

    pub fn read_u32(&self, offset: usize, endian: Endian) -> Result<u32, XiftyError> {
        let bytes = self.slice(offset, 4)?;
        Ok(match endian {
            Endian::Little => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            Endian::Big => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        })
    }

    pub fn read_i32(&self, offset: usize, endian: Endian) -> Result<i32, XiftyError> {
        let value = self.read_u32(offset, endian)?;
        Ok(value as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_reads_endian_values() {
        let cursor = Cursor::new(&[0x01, 0x02, 0x03, 0x04], 8);
        assert_eq!(cursor.read_u16(0, Endian::Big).unwrap(), 0x0102);
        assert_eq!(cursor.read_u16(0, Endian::Little).unwrap(), 0x0201);
        assert_eq!(cursor.read_u32(0, Endian::Big).unwrap(), 0x01020304);
        assert_eq!(cursor.absolute_offset(2), 10);
    }
}
