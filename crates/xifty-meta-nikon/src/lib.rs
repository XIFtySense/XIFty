//! Nikon MakerNote decoder (`namespace = "nikon"`).
//!
//! Status: **bounded** — the decoder parses the top-level Nikon MakerNote
//! sub-IFD inside a parent TIFF buffer, surfacing a small set of plain ASCII
//! tags (Quality, WhiteBalance, Sharpness, FocusMode, FlashSetting) plus
//! LensType (0x0083 BYTE bitfield).
//!
//! ## MakerNote header variants
//!
//! Nikon writes at least three MakerNote layouts. This decoder recognises:
//!
//! 1. **TIFF-in-TIFF v2/v3** (D2X+ era and modern bodies): the MakerNote
//!    payload begins with `b"Nikon\x00\x02"` + a 1-byte minor version + two
//!    NUL bytes, followed by a fresh TIFF header (`II*\0` or `MM\0*`) at
//!    payload offset +10. All inner-IFD offsets are relative to that inner
//!    TIFF header (NOT to the outer file). The decoder rebases all reads
//!    onto a sub-cursor anchored at the inner TIFF header so absolute file
//!    offsets stay correct.
//! 2. **Legacy v1** (older bodies): payload begins with `b"Nikon\x00\x01\x00"`
//!    followed by a sub-IFD at payload offset +8. Inner offsets are
//!    MakerNote-relative; the decoder rebases on the MakerNote start.
//!
//! Anything else returns an empty `Vec` — silent failure mirrors the Sony
//! decoder's behaviour and the broader "skip-and-continue" rule from issue
//! [#64](../../docs/specs/closed-issues.md).
//!
//! ## Encrypted regions
//!
//! Newer Nikon bodies (D2X+) ship a partially-encrypted MakerNote: the
//! wrapper IFD parses, but several payload regions are obfuscated by a
//! serial-number-keyed XOR scheme. **XIFty does NOT attempt decryption at
//! v1.** Encrypted payloads are surfaced via [`encrypted_regions`] which
//! walks the same MakerNote IFD and returns one [`EncryptedRegion`] per
//! recognised encrypted tag without decoding anything. The CLI wraps each
//! region in a non-fatal `nikon_makernote_encrypted_region` Issue,
//! mirroring the empty-decode pattern used for ICC / IPTC / XMP.
//!
//! The current encrypted-tag list is narrowed to the **confirmed pair**:
//!
//! * `0x0091` (ShotInfo) — encrypted on D2X and later bodies.
//! * `0x0097` (LensData) — encrypted when its first byte (the version
//!   preamble) is `>= 0x02` (LensData v0200 / v0201 / v0204 / v0400+).
//!   Earlier LensData versions (v0100, v0101) ship plain-text and are
//!   intentionally NOT flagged as encrypted; the decoder simply skips them
//!   too (no plain decoder is wired up for them today).
//!
//! Other tags sometimes cited in the wild as "encrypted" (e.g. `0x0098`,
//! `0x00A8`) are intentionally NOT in the v1 list because a survey of
//! ExifTool's `Image::ExifTool::Nikon` source could not confirm them as
//! standalone encrypted tag IDs (LensData encryption is gated on the
//! `0x0097` version byte, not a separate `0x0098` tag). Treating a plain
//! tag as encrypted would silently hide data behind a Warning Issue, so we
//! err on the conservative side; missing a real encrypted tag at worst
//! surfaces garbage as an unrecognised payload, which is preferable.

use xifty_container_tiff::TiffContainer;
use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

/// Nikon TIFF-in-TIFF v2/v3 MakerNote header. Followed by a 1-byte minor
/// version and two NUL bytes, then a fresh TIFF header at payload offset +10.
const NIKON_TIFF_HEADER: &[u8] = b"Nikon\x00\x02";

/// Nikon legacy v1 MakerNote header. Followed by a sub-IFD at payload
/// offset +8. Inner offsets are MakerNote-relative.
const NIKON_LEGACY_HEADER: &[u8] = b"Nikon\x00\x01\x00";

/// One MakerNote IFD entry, copied into a struct for ergonomic dispatch.
#[derive(Debug, Clone, Copy)]
struct MakerEntry {
    tag_id: u16,
    type_id: u16,
    count: u32,
    value_or_offset: u32,
}

/// One encrypted MakerNote region. Surfaced by [`encrypted_regions`] so the
/// CLI can emit a non-fatal `nikon_makernote_encrypted_region` Issue per
/// recognised encrypted tag.
///
/// `absolute_offset` is in **outer-file coordinates** — i.e. the byte offset
/// inside the parent TIFF buffer (`bytes`) at which the encrypted payload
/// starts. Callers can compare it directly against other byte offsets
/// surfaced by `xifty-container-tiff`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedRegion {
    pub tag_id: u16,
    pub absolute_offset: u64,
}

/// Decode a Nikon MakerNote sub-IFD inside a parent TIFF buffer.
///
/// Signature mirrors `xifty_meta_sony::decode_from_tiff` exactly so the CLI
/// can call into the Nikon decoder using the same idiom as the Sony /
/// Canon / Apple decoders.
///
/// Returns an empty `Vec` when EXIF `Make` is absent / not Nikon, when the
/// MakerNote tag is not present, or when the MakerNote payload does not
/// match a recognised Nikon header. Encrypted regions are NOT decoded;
/// callers wanting to surface them should call [`encrypted_regions`] in
/// addition.
pub fn decode_from_tiff(
    bytes: &[u8],
    base_offset: u64,
    container_name: &str,
    tiff: &TiffContainer,
    exif_entries: &[MetadataEntry],
) -> Vec<MetadataEntry> {
    let make = exif_entries
        .iter()
        .find(|entry| entry.namespace == "exif" && entry.tag_name == "Make")
        .and_then(|entry| match &entry.value {
            TypedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if !matches!(
        make,
        Some(value) if value.trim().to_ascii_uppercase().starts_with("NIKON")
    ) {
        return Vec::new();
    }

    let Some(parsed) = parse_maker_note(bytes, base_offset, tiff) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in &parsed.entries {
        if is_encrypted_tag(
            entry.tag_id,
            encrypted_version_byte(entry.tag_id, &parsed, &entry),
        ) {
            // Skip encrypted tags entirely — encrypted_regions surfaces them.
            continue;
        }
        decode_plain_entry(
            &mut out,
            container_name,
            &parsed.cursor,
            parsed.endian,
            entry,
        );
    }
    out
}

/// Walk a Nikon MakerNote sub-IFD and return one [`EncryptedRegion`] per
/// encrypted tag, **without decoding** any encrypted bytes.
///
/// Returns an empty `Vec` on any failure (no MakerNote, unrecognised header,
/// truncated payload, etc.) — silent failure matches [`decode_from_tiff`].
///
/// `absolute_offset` is in outer-file coordinates: the byte position inside
/// `bytes` where the encrypted payload starts (computed via the cursor's
/// inner-base bookkeeping when the TIFF-in-TIFF header is present).
pub fn encrypted_regions(
    bytes: &[u8],
    base_offset: u64,
    tiff: &TiffContainer,
) -> Vec<EncryptedRegion> {
    let Some(parsed) = parse_maker_note(bytes, base_offset, tiff) else {
        return Vec::new();
    };
    let mut regions = Vec::new();
    for entry in &parsed.entries {
        let version_byte = encrypted_version_byte(entry.tag_id, &parsed, entry);
        if !is_encrypted_tag(entry.tag_id, version_byte) {
            continue;
        }
        let Some(absolute_offset) = entry_payload_absolute_offset(&parsed, entry) else {
            continue;
        };
        regions.push(EncryptedRegion {
            tag_id: entry.tag_id,
            absolute_offset,
        });
    }
    regions
}

/// Internal: a parsed MakerNote payload anchored on a sub-cursor whose
/// `base_offset` matches the inner TIFF header (TIFF-in-TIFF v2/v3) or the
/// MakerNote start (legacy v1). All inner-IFD offsets are interpreted
/// against this cursor.
struct ParsedMakerNote<'a> {
    cursor: Cursor<'a>,
    endian: Endian,
    entries: Vec<MakerEntry>,
    /// Absolute byte offset into the outer file `bytes` corresponding to
    /// `cursor` local offset 0. Used to compute outer-file coordinates for
    /// payloads that live out-of-line.
    inner_absolute_base: u64,
}

fn parse_maker_note<'a>(
    bytes: &'a [u8],
    base_offset: u64,
    tiff: &TiffContainer,
) -> Option<ParsedMakerNote<'a>> {
    let maker_note = tiff.entries.iter().find(|entry| entry.tag_id == 0x927C)?;
    let start = maker_note.value_or_offset as usize;
    let end = start.checked_add(maker_note.count as usize)?;
    let maker_bytes = bytes.get(start..end)?;

    if maker_bytes.starts_with(NIKON_TIFF_HEADER) {
        // TIFF-in-TIFF v2/v3: 6-byte signature + 1 byte version + 2 NUL
        // bytes + an inner TIFF header at payload offset +10. All inner
        // offsets are relative to the inner TIFF header.
        if maker_bytes.len() < 18 {
            return None;
        }
        let inner_tiff_local = 10usize;
        let inner_tiff = maker_bytes.get(inner_tiff_local..)?;
        let endian = match inner_tiff.get(0..2)? {
            b"II" => Endian::Little,
            b"MM" => Endian::Big,
            _ => return None,
        };
        let magic = read_u16(inner_tiff, 2, endian)?;
        if magic != 42 {
            return None;
        }
        let first_ifd = read_u32(inner_tiff, 4, endian)? as usize;
        let inner_absolute_base = base_offset + start as u64 + inner_tiff_local as u64;
        let cursor = Cursor::new(inner_tiff, inner_absolute_base);
        let entries = parse_ifd_entries(&cursor, endian, first_ifd);
        Some(ParsedMakerNote {
            cursor,
            endian,
            entries,
            inner_absolute_base,
        })
    } else if maker_bytes.starts_with(NIKON_LEGACY_HEADER) {
        // Legacy v1: header + sub-IFD at payload offset +8. Endianness
        // follows the parent TIFF (no inner endianness marker).
        if maker_bytes.len() < 10 {
            return None;
        }
        let inner_absolute_base = base_offset + start as u64;
        let cursor = Cursor::new(maker_bytes, inner_absolute_base);
        let entries = parse_ifd_entries(&cursor, tiff.endian, 8);
        Some(ParsedMakerNote {
            cursor,
            endian: tiff.endian,
            entries,
            inner_absolute_base,
        })
    } else {
        None
    }
}

fn parse_ifd_entries(cursor: &Cursor<'_>, endian: Endian, ifd_offset: usize) -> Vec<MakerEntry> {
    let Ok(count) = cursor.read_u16(ifd_offset, endian) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for index in 0..count as usize {
        let offset = ifd_offset + 2 + index * 12;
        if offset + 12 > cursor.len() {
            break;
        }
        let Ok(tag_id) = cursor.read_u16(offset, endian) else {
            break;
        };
        let Ok(type_id) = cursor.read_u16(offset + 2, endian) else {
            break;
        };
        let Ok(count) = cursor.read_u32(offset + 4, endian) else {
            break;
        };
        let Ok(value_or_offset) = cursor.read_u32(offset + 8, endian) else {
            break;
        };
        entries.push(MakerEntry {
            tag_id,
            type_id,
            count,
            value_or_offset,
        });
    }
    entries
}

/// Return true when this tag is on the v1 confirmed-encrypted list.
///
/// `version_byte` is the first byte of the tag's payload, used to gate
/// `0x0097` (LensData): only v0200+ uses the encrypted layout. Pass `None`
/// for tags that don't need a version gate or when the byte cannot be read.
fn is_encrypted_tag(tag_id: u16, version_byte: Option<u8>) -> bool {
    match tag_id {
        0x0091 => true,
        0x0097 => matches!(version_byte, Some(byte) if byte >= 0x02),
        _ => false,
    }
}

/// Read the version preamble byte for a candidate encrypted tag. For
/// `0x0097` LensData this is the first byte of the payload (an ASCII digit
/// like `'0'` for v01.. or `'2'` for v02..); we read the raw byte rather
/// than the ASCII codepoint so the gate matches the way ExifTool encodes
/// the comparison (`>= 0x02`).
///
/// Returns `None` when the tag does not need a version gate or when the
/// payload cannot be read.
fn encrypted_version_byte(
    tag_id: u16,
    parsed: &ParsedMakerNote<'_>,
    entry: &MakerEntry,
) -> Option<u8> {
    if tag_id != 0x0097 {
        return None;
    }
    payload_bytes(&parsed.cursor, parsed.endian, entry).and_then(|bytes| bytes.first().copied())
}

/// Compute the absolute outer-file byte offset where this entry's payload
/// starts.
///
/// For inline values (count * type_size <= 4) the payload sits inside the
/// IFD entry itself; we return the absolute offset of the entry's
/// value_or_offset field. For out-of-line values the payload lives at the
/// inner-base + value_or_offset position; we add the cursor's
/// inner-absolute-base to land back in outer-file coordinates.
fn entry_payload_absolute_offset(parsed: &ParsedMakerNote<'_>, entry: &MakerEntry) -> Option<u64> {
    let byte_len = type_size(entry.type_id).checked_mul(entry.count as usize)?;
    if byte_len <= 4 {
        // Inline payload. We don't have the entry's local offset here, but
        // for the encrypted-region purpose the inner-base + value_or_offset
        // is the meaningful location nikon decoders look at — and inline
        // encrypted tags are not realistic in any case (ShotInfo / LensData
        // are always multi-byte blobs). Fall back to inner_absolute_base +
        // value_or_offset for symmetry with the out-of-line path.
        Some(parsed.inner_absolute_base + entry.value_or_offset as u64)
    } else {
        Some(parsed.inner_absolute_base + entry.value_or_offset as u64)
    }
}

fn payload_bytes<'a>(cursor: &Cursor<'a>, endian: Endian, entry: &MakerEntry) -> Option<Vec<u8>> {
    let byte_len = type_size(entry.type_id).checked_mul(entry.count as usize)?;
    if byte_len <= 4 {
        let packed = match endian {
            Endian::Little => entry.value_or_offset.to_le_bytes(),
            Endian::Big => entry.value_or_offset.to_be_bytes(),
        };
        Some(packed[..byte_len].to_vec())
    } else {
        cursor
            .slice(entry.value_or_offset as usize, byte_len)
            .ok()
            .map(|bytes| bytes.to_vec())
    }
}

fn type_size(type_id: u16) -> usize {
    match type_id {
        1 | 2 | 7 => 1,
        3 | 8 => 2,
        4 | 9 => 4,
        5 | 10 => 8,
        _ => 1,
    }
}

fn decode_plain_entry(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    endian: Endian,
    entry: &MakerEntry,
) {
    match entry.tag_id {
        0x0004 => push_ascii(out, container_name, "Quality", entry, cursor, endian),
        0x0005 => push_ascii(out, container_name, "WhiteBalance", entry, cursor, endian),
        0x0006 => push_ascii(out, container_name, "Sharpness", entry, cursor, endian),
        0x0007 => push_ascii(out, container_name, "FocusMode", entry, cursor, endian),
        0x0008 => push_ascii(out, container_name, "FlashSetting", entry, cursor, endian),
        0x0083 => push_lens_type(out, container_name, entry, cursor, endian),
        _ => {}
    }
}

fn push_ascii(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    entry: &MakerEntry,
    cursor: &Cursor<'_>,
    endian: Endian,
) {
    if entry.type_id != 2 {
        return;
    }
    let Some(bytes) = payload_bytes(cursor, endian, entry) else {
        return;
    };
    let value = trim_c_string_bytes(&bytes);
    if value.is_empty() {
        return;
    }
    out.push(MetadataEntry {
        namespace: "nikon".into(),
        tag_id: format!("0x{:04X}", entry.tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: provenance(container_name),
        notes: Vec::new(),
    });
}

/// LensType (0x0083) is a 1-byte BYTE bitfield. Surface it as an integer
/// so downstream consumers can decode the bit flags themselves; emitting a
/// human-readable name would require a model-specific lens database we do
/// not ship at v1.
fn push_lens_type(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    entry: &MakerEntry,
    cursor: &Cursor<'_>,
    endian: Endian,
) {
    if entry.type_id != 1 {
        return;
    }
    let Some(bytes) = payload_bytes(cursor, endian, entry) else {
        return;
    };
    let Some(byte) = bytes.first().copied() else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "nikon".into(),
        tag_id: format!("0x{:04X}", entry.tag_id),
        tag_name: "LensType".into(),
        value: TypedValue::Integer(byte as i64),
        provenance: provenance(container_name),
        notes: Vec::new(),
    });
}

fn trim_c_string_bytes(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn read_u16(bytes: &[u8], offset: usize, endian: Endian) -> Option<u16> {
    let arr: [u8; 2] = bytes.get(offset..offset + 2)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => u16::from_le_bytes(arr),
        Endian::Big => u16::from_be_bytes(arr),
    })
}

fn read_u32(bytes: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    let arr: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => u32::from_le_bytes(arr),
        Endian::Big => u32::from_be_bytes(arr),
    })
}

fn provenance(container_name: &str) -> Provenance {
    Provenance {
        container: container_name.into(),
        namespace: "nikon".into(),
        path: Some("ifd0_makernote".into()),
        offset_start: None,
        offset_end: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal little-endian TIFF whose IFD0 carries a Make=NIKON
    /// CORPORATION entry plus a MakerNote (0x927C) tag pointing at a
    /// caller-supplied MakerNote payload. Payload is appended to the buffer
    /// and the MakerNote tag's offset references its absolute file position.
    fn tiff_with_nikon_makernote(maker_payload: &[u8]) -> Vec<u8> {
        let p16 = |v: u16| v.to_le_bytes();
        let p32 = |v: u32| v.to_le_bytes();
        let make = b"NIKON CORPORATION\0";
        // Layout: header(8) + ifd0_count(2) + 2 entries(24) + next(4) = 38
        // then make blob (18 bytes) at offset 38, then MakerNote payload.
        let make_offset: u32 = 38;
        let maker_offset: u32 = make_offset + make.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&p32(8)); // IFD0 at offset 8
        out.extend_from_slice(&p16(2)); // 2 entries
        // 0x010F Make, ASCII, count = make.len(), out-of-line offset
        out.extend_from_slice(&p16(0x010F));
        out.extend_from_slice(&p16(2));
        out.extend_from_slice(&p32(make.len() as u32));
        out.extend_from_slice(&p32(make_offset));
        // 0x927C MakerNote, UNDEFINED, count = payload.len(), offset
        out.extend_from_slice(&p16(0x927C));
        out.extend_from_slice(&p16(7));
        out.extend_from_slice(&p32(maker_payload.len() as u32));
        out.extend_from_slice(&p32(maker_offset));
        out.extend_from_slice(&p32(0)); // next IFD = 0
        out.extend_from_slice(make);
        out.extend_from_slice(maker_payload);
        out
    }

    /// Build a Nikon TIFF-in-TIFF v2/v3 MakerNote payload containing
    /// `entries` (each `(tag_id, type_id, count, value_or_offset, blob)`,
    /// with `blob` appended if not empty). Inner endianness is little.
    fn nikon_tiff_in_tiff_payload(
        inner_entries: &[(u16, u16, u32, u32)],
        appended_blob: &[u8],
    ) -> Vec<u8> {
        let p16 = |v: u16| v.to_le_bytes();
        let p32 = |v: u32| v.to_le_bytes();
        let mut payload = Vec::new();
        // 6-byte signature + 1-byte version + 2 NUL bytes
        payload.extend_from_slice(b"Nikon\x00\x02");
        payload.push(0x10); // minor version
        payload.extend_from_slice(&[0x00, 0x00]);
        // Inner TIFF starts at payload offset 10.
        payload.extend_from_slice(b"II*\0");
        payload.extend_from_slice(&p32(8)); // first IFD at inner offset 8
        payload.extend_from_slice(&p16(inner_entries.len() as u16));
        for (tag_id, type_id, count, value_or_offset) in inner_entries {
            payload.extend_from_slice(&p16(*tag_id));
            payload.extend_from_slice(&p16(*type_id));
            payload.extend_from_slice(&p32(*count));
            payload.extend_from_slice(&p32(*value_or_offset));
        }
        payload.extend_from_slice(&p32(0)); // next IFD = 0
        payload.extend_from_slice(appended_blob);
        payload
    }

    #[test]
    fn truncated_payload_returns_empty() {
        // MakerNote payload too short to contain a Nikon header.
        let tiff_bytes = tiff_with_nikon_makernote(b"\x00\x00\x00");
        let tiff = xifty_container_tiff::parse_bytes(&tiff_bytes, 0, "tiff").unwrap();
        assert!(encrypted_regions(&tiff_bytes, 0, &tiff).is_empty());
        let exif = vec![MetadataEntry {
            namespace: "exif".into(),
            tag_id: "0x010F".into(),
            tag_name: "Make".into(),
            value: TypedValue::String("NIKON CORPORATION".into()),
            provenance: Provenance {
                container: "tiff".into(),
                namespace: "exif".into(),
                path: None,
                offset_start: None,
                offset_end: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        }];
        assert!(decode_from_tiff(&tiff_bytes, 0, "tiff", &tiff, &exif).is_empty());
    }

    #[test]
    fn unrecognised_header_returns_empty() {
        let payload = b"NotNikon\x00\x00\x00\x00\x00\x00\x00\x00";
        let tiff_bytes = tiff_with_nikon_makernote(payload);
        let tiff = xifty_container_tiff::parse_bytes(&tiff_bytes, 0, "tiff").unwrap();
        assert!(encrypted_regions(&tiff_bytes, 0, &tiff).is_empty());
    }

    #[test]
    fn plain_quality_tag_round_trips() {
        // Inner IFD with one ASCII Quality tag (value "FINE\0" inline, 5
        // bytes — must live out-of-line because count > 4).
        // Compute layout: inner TIFF starts at payload+10. Header(8) +
        // count(2) + 1 entry(12) + next(4) = inner offset 26 → inner-base
        // payload-relative blob lives at inner offset 26 (+10 outer).
        let blob = b"FINE\x00";
        let inner_blob_offset_inner = 26u32; // inner-relative
        let payload = nikon_tiff_in_tiff_payload(
            &[(0x0004, 2, blob.len() as u32, inner_blob_offset_inner)],
            blob,
        );
        let tiff_bytes = tiff_with_nikon_makernote(&payload);
        let tiff = xifty_container_tiff::parse_bytes(&tiff_bytes, 0, "tiff").unwrap();
        let exif = vec![MetadataEntry {
            namespace: "exif".into(),
            tag_id: "0x010F".into(),
            tag_name: "Make".into(),
            value: TypedValue::String("NIKON CORPORATION".into()),
            provenance: Provenance {
                container: "tiff".into(),
                namespace: "exif".into(),
                path: None,
                offset_start: None,
                offset_end: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        }];
        let entries = decode_from_tiff(&tiff_bytes, 0, "tiff", &tiff, &exif);
        assert_eq!(entries.len(), 1, "got entries: {entries:?}");
        assert_eq!(entries[0].namespace, "nikon");
        assert_eq!(entries[0].tag_name, "Quality");
        assert_eq!(entries[0].value, TypedValue::String("FINE".into()));
        assert!(encrypted_regions(&tiff_bytes, 0, &tiff).is_empty());
    }

    #[test]
    fn encrypted_shot_info_skipped_and_reported() {
        // 0x0091 ShotInfo: synthetic 16-byte encrypted blob with version
        // preamble 0x02 0x04 (v0204). decode_from_tiff must skip; encrypted_regions
        // must report exactly one record.
        let blob = b"\x02\x04abcdefghijklmn\x00\x00";
        let inner_blob_offset_inner = 26u32;
        let payload = nikon_tiff_in_tiff_payload(
            &[(0x0091, 7, blob.len() as u32, inner_blob_offset_inner)],
            blob,
        );
        let tiff_bytes = tiff_with_nikon_makernote(&payload);
        let tiff = xifty_container_tiff::parse_bytes(&tiff_bytes, 0, "tiff").unwrap();
        let exif = vec![MetadataEntry {
            namespace: "exif".into(),
            tag_id: "0x010F".into(),
            tag_name: "Make".into(),
            value: TypedValue::String("NIKON CORPORATION".into()),
            provenance: Provenance {
                container: "tiff".into(),
                namespace: "exif".into(),
                path: None,
                offset_start: None,
                offset_end: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        }];
        let entries = decode_from_tiff(&tiff_bytes, 0, "tiff", &tiff, &exif);
        assert!(
            entries.is_empty(),
            "encrypted ShotInfo must not be decoded as plain: {entries:?}"
        );
        let regions = encrypted_regions(&tiff_bytes, 0, &tiff);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].tag_id, 0x0091);
        // Inner blob lives at inner offset 26; inner_absolute_base sits at
        // outer file offset = (header 8 + ifd 2 entries(24) + next(4)) +
        // make blob(18) + payload offset of inner TIFF (10) = 38 + 18 + 10 = 66.
        // So absolute = 66 + 26 = 92.
        let outer_make_offset = 38 + 18; // outer file offset of MakerNote start
        let inner_tiff_outer_offset = outer_make_offset + 10;
        assert_eq!(
            regions[0].absolute_offset,
            inner_tiff_outer_offset as u64 + inner_blob_offset_inner as u64
        );
    }

    #[test]
    fn lens_data_v0100_is_not_flagged_as_encrypted() {
        // 0x0097 with version preamble 0x01 (v01..) is plain — must NOT be
        // reported as an encrypted region.
        let blob = b"\x01\x00abcdefghijklmn\x00\x00";
        let inner_blob_offset_inner = 26u32;
        let payload = nikon_tiff_in_tiff_payload(
            &[(0x0097, 7, blob.len() as u32, inner_blob_offset_inner)],
            blob,
        );
        let tiff_bytes = tiff_with_nikon_makernote(&payload);
        let tiff = xifty_container_tiff::parse_bytes(&tiff_bytes, 0, "tiff").unwrap();
        let regions = encrypted_regions(&tiff_bytes, 0, &tiff);
        assert!(
            regions.is_empty(),
            "v01 LensData must not be flagged: {regions:?}"
        );
    }

    #[test]
    fn lens_data_v0200_is_flagged_as_encrypted() {
        let blob = b"\x02\x00abcdefghijklmn\x00\x00";
        let inner_blob_offset_inner = 26u32;
        let payload = nikon_tiff_in_tiff_payload(
            &[(0x0097, 7, blob.len() as u32, inner_blob_offset_inner)],
            blob,
        );
        let tiff_bytes = tiff_with_nikon_makernote(&payload);
        let tiff = xifty_container_tiff::parse_bytes(&tiff_bytes, 0, "tiff").unwrap();
        let regions = encrypted_regions(&tiff_bytes, 0, &tiff);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].tag_id, 0x0097);
    }

    #[test]
    fn non_nikon_make_returns_empty() {
        let payload = nikon_tiff_in_tiff_payload(&[], &[]);
        let tiff_bytes = tiff_with_nikon_makernote(&payload);
        let tiff = xifty_container_tiff::parse_bytes(&tiff_bytes, 0, "tiff").unwrap();
        let exif = vec![MetadataEntry {
            namespace: "exif".into(),
            tag_id: "0x010F".into(),
            tag_name: "Make".into(),
            value: TypedValue::String("Canon".into()),
            provenance: Provenance {
                container: "tiff".into(),
                namespace: "exif".into(),
                path: None,
                offset_start: None,
                offset_end: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        }];
        let entries = decode_from_tiff(&tiff_bytes, 0, "tiff", &tiff, &exif);
        assert!(entries.is_empty());
    }
}
