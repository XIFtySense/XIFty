use std::{fmt, fs, path::Path, sync::Arc};

use memmap2::Mmap;
use xifty_core::{SourceRef, XiftyError};

/// Backing storage for [`SourceBytes`].
///
/// Either an in-memory `Vec<u8>` (used by callers that already have the bytes,
/// e.g. browser/WASM contexts) or a memory-mapped region from a path. The
/// mmap variant lets the OS page bytes in on demand and evict clean pages
/// under memory pressure, which keeps multi-gigabyte inputs (e.g. 4 GB DJI
/// drone MP4s) under the AWS Lambda 3 GB ceiling — `std::fs::read` would
/// allocate the entire file into the heap up front and OOM.
enum SourceInner {
    Owned(Vec<u8>),
    Mapped(Mmap),
}

impl SourceInner {
    fn bytes(&self) -> &[u8] {
        match self {
            SourceInner::Owned(v) => v.as_slice(),
            SourceInner::Mapped(m) => &m[..],
        }
    }
}

#[derive(Clone)]
pub struct SourceBytes {
    pub source: SourceRef,
    inner: Arc<SourceInner>,
}

impl SourceBytes {
    /// Wrap an already-loaded byte buffer. Used when bytes are produced
    /// outside the filesystem (browser uploads, WASM, in-memory fixtures).
    pub fn new(path: impl AsRef<Path>, bytes: Vec<u8>) -> Self {
        let path = path.as_ref();
        let source = SourceRef {
            path: path.to_path_buf(),
            size_bytes: bytes.len() as u64,
        };
        Self {
            source,
            inner: Arc::new(SourceInner::Owned(bytes)),
        }
    }

    /// Open `path` and back the `SourceBytes` with a memory map instead of
    /// reading the whole file into a `Vec<u8>`. Falls back to an empty owned
    /// buffer for zero-length files (some platforms reject mmap of length 0).
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, XiftyError> {
        let path = path.as_ref();
        let file = fs::File::open(path)?;
        let metadata = file.metadata()?;
        let size_bytes = metadata.len();

        let inner = if size_bytes == 0 {
            SourceInner::Owned(Vec::new())
        } else {
            // SAFETY: mmap is unsafe because external mutation of the mapped
            // file (truncation, writes by another process) would race with
            // readers. XIFty treats `SourceBytes` as a read-only view of the
            // input file for the duration of extraction; callers do not mutate
            // the file while a `SourceBytes` is alive.
            match unsafe { Mmap::map(&file) } {
                Ok(mmap) => SourceInner::Mapped(mmap),
                // Some platforms / filesystems refuse to mmap empty or
                // pseudo-files even when metadata reports a non-zero size.
                // Fall back to a plain read so we still produce a valid view.
                Err(_) => SourceInner::Owned(fs::read(path)?),
            }
        };

        let actual_len = inner.bytes().len() as u64;
        let source = SourceRef {
            path: path.to_path_buf(),
            size_bytes: actual_len,
        };
        Ok(Self {
            source,
            inner: Arc::new(inner),
        })
    }

    pub fn bytes(&self) -> &[u8] {
        self.inner.bytes()
    }
}

impl fmt::Debug for SourceBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match &*self.inner {
            SourceInner::Owned(_) => "Owned",
            SourceInner::Mapped(_) => "Mapped",
        };
        f.debug_struct("SourceBytes")
            .field("path", &self.source.path)
            .field("size_bytes", &self.source.size_bytes)
            .field("backing", &kind)
            .finish()
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

    #[test]
    fn source_bytes_new_uses_owned_storage() {
        let bytes = vec![1u8, 2, 3, 4, 5];
        let source = SourceBytes::new("/tmp/in-memory.bin", bytes.clone());
        assert_eq!(source.bytes(), bytes.as_slice());
        assert_eq!(source.source.size_bytes, 5);
        assert!(matches!(&*source.inner, SourceInner::Owned(_)));
    }

    #[test]
    fn source_bytes_from_path_maps_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("xifty-source-mmap-{}.bin", std::process::id()));
        let payload: Vec<u8> = (0..=255u8).collect();
        std::fs::write(&path, &payload).unwrap();

        let source = SourceBytes::from_path(&path).unwrap();
        assert_eq!(source.bytes(), payload.as_slice());
        assert_eq!(source.source.size_bytes, payload.len() as u64);
        assert!(matches!(&*source.inner, SourceInner::Mapped(_)));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn source_bytes_from_path_handles_empty_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("xifty-source-empty-{}.bin", std::process::id()));
        std::fs::write(&path, b"").unwrap();

        let source = SourceBytes::from_path(&path).unwrap();
        assert!(source.bytes().is_empty());
        assert_eq!(source.source.size_bytes, 0);
        // Empty files fall back to owned storage because some platforms
        // refuse to mmap a zero-length region.
        assert!(matches!(&*source.inner, SourceInner::Owned(_)));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn source_bytes_clone_shares_backing_storage() {
        let bytes = vec![9u8; 32];
        let a = SourceBytes::new("/tmp/clone.bin", bytes);
        let b = a.clone();
        // Same Arc pointee — clones do not duplicate the underlying buffer.
        assert!(std::ptr::eq(a.bytes().as_ptr(), b.bytes().as_ptr()));
    }

    #[test]
    fn source_bytes_debug_redacts_contents() {
        let source = SourceBytes::new("/tmp/dbg.bin", vec![0u8; 4]);
        let rendered = format!("{source:?}");
        assert!(rendered.contains("SourceBytes"));
        assert!(rendered.contains("size_bytes"));
        assert!(rendered.contains("Owned"));
    }
}
