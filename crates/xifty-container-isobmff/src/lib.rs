use std::collections::HashMap;

use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

#[derive(Debug, Clone)]
pub struct IsobmffPayload {
    pub kind: &'static str,
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct IsobmffDimensions {
    pub width: u32,
    pub height: u32,
    pub offset_start: u64,
    pub offset_end: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct IsobmffContainer {
    pub major_brand: [u8; 4],
    pub compatible_brands: Vec<[u8; 4]>,
    pub nodes: Vec<ContainerNode>,
    pub payloads: Vec<IsobmffPayload>,
    pub primary_item_dimensions: Option<IsobmffDimensions>,
    pub issues: Vec<Issue>,
}

impl IsobmffContainer {
    pub fn exif_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "exif")
    }

    pub fn xmp_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads.iter().filter(|payload| payload.kind == "xmp")
    }

    pub fn is_heif_still_image(&self) -> bool {
        heif_brand(self.major_brand) || self.compatible_brands.iter().copied().any(heif_brand)
    }
}

#[derive(Debug, Clone)]
struct ParsedBox {
    start: usize,
    box_type: [u8; 4],
    data_offset: usize,
    end: usize,
    path: String,
}

#[derive(Debug, Clone)]
struct ItemInfo {
    item_id: u32,
    item_type: [u8; 4],
    content_type: Option<String>,
}

#[derive(Debug, Clone)]
struct ItemExtent {
    offset: u64,
    length: u64,
}

#[derive(Debug, Clone)]
struct ItemLocation {
    item_id: u32,
    construction_method: u16,
    base_offset: u64,
    extents: Vec<ItemExtent>,
}

#[derive(Debug, Clone)]
struct PropertyAssociation {
    item_id: u32,
    property_indexes: Vec<u16>,
}

#[derive(Debug, Clone)]
struct ParseState {
    item_infos: Vec<ItemInfo>,
    item_locations: Vec<ItemLocation>,
    property_associations: Vec<PropertyAssociation>,
    property_dimensions: HashMap<u16, IsobmffDimensions>,
    primary_item_id: Option<u32>,
    idat_payloads: Vec<(u64, u64, String)>,
}

pub fn parse(source: &SourceBytes) -> Result<IsobmffContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<IsobmffContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 16 || cursor.slice(4, 4)? != b"ftyp" {
        return Err(XiftyError::Parse {
            message: "not an isobmff container".into(),
        });
    }

    let mut state = ParseState {
        item_infos: Vec::new(),
        item_locations: Vec::new(),
        property_associations: Vec::new(),
        property_dimensions: HashMap::new(),
        primary_item_id: None,
        idat_payloads: Vec::new(),
    };
    let mut nodes = Vec::new();
    let mut payloads = Vec::new();
    let mut issues = Vec::new();
    let mut major_brand = *b"    ";
    let mut compatible_brands = Vec::new();

    parse_children(
        &cursor,
        0,
        cursor.len(),
        None,
        &mut state,
        &mut nodes,
        &mut payloads,
        &mut issues,
        &mut major_brand,
        &mut compatible_brands,
    )?;

    payloads.extend(payloads_from_items(&cursor, &state, &mut issues));
    let primary_item_dimensions = primary_item_dimensions(&state);

    if !heif_brand(major_brand) && !compatible_brands.iter().copied().any(heif_brand) {
        issues.push(issue(
            Severity::Info,
            "isobmff_non_heif_brand",
            format!(
                "isobmff major brand {} is not a recognized still-image HEIF brand",
                fourcc(major_brand)
            ),
        ));
    }

    Ok(IsobmffContainer {
        major_brand,
        compatible_brands,
        nodes,
        payloads,
        primary_item_dimensions,
        issues,
    })
}

fn parse_children(
    cursor: &Cursor<'_>,
    start: usize,
    end: usize,
    parent: Option<&ParsedBox>,
    state: &mut ParseState,
    nodes: &mut Vec<ContainerNode>,
    payloads: &mut Vec<IsobmffPayload>,
    issues: &mut Vec<Issue>,
    major_brand: &mut [u8; 4],
    compatible_brands: &mut Vec<[u8; 4]>,
) -> Result<(), XiftyError> {
    let mut offset = start;
    while offset < end {
        let Some(parsed) = parse_box_header(cursor, offset, end, parent, issues)? else {
            break;
        };

        nodes.push(ContainerNode {
            kind: "box".into(),
            label: parsed.path.clone(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(parsed.end),
            parent_label: parent.map(|item| item.path.clone()),
        });

        match &parsed.box_type {
            b"ftyp" => parse_ftyp(cursor, &parsed, major_brand, compatible_brands)?,
            b"meta" => {
                if parsed.data_offset + 4 > parsed.end {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "isobmff_meta_header_truncated".into(),
                        message: "meta box is missing full-box header bytes".into(),
                        offset: Some(cursor.absolute_offset(parsed.data_offset)),
                        context: Some(parsed.path.clone()),
                    });
                } else {
                    parse_children(
                        cursor,
                        parsed.data_offset + 4,
                        parsed.end,
                        Some(&parsed),
                        state,
                        nodes,
                        payloads,
                        issues,
                        major_brand,
                        compatible_brands,
                    )?;
                }
            }
            b"dinf" | b"dref" | b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts"
            | b"udta" | b"iprp" | b"ipco" => {
                parse_children(
                    cursor,
                    parsed.data_offset,
                    parsed.end,
                    Some(&parsed),
                    state,
                    nodes,
                    payloads,
                    issues,
                    major_brand,
                    compatible_brands,
                )?;
            }
            b"iinf" => {
                let child_start = parse_iinf(cursor, &parsed, state, issues)?;
                if child_start <= parsed.end {
                    parse_children(
                        cursor,
                        child_start,
                        parsed.end,
                        Some(&parsed),
                        state,
                        nodes,
                        payloads,
                        issues,
                        major_brand,
                        compatible_brands,
                    )?;
                }
            }
            b"infe" => parse_infe(cursor, &parsed, state, issues)?,
            b"pitm" => parse_pitm(cursor, &parsed, state, issues)?,
            b"iloc" => parse_iloc(cursor, &parsed, state, issues)?,
            b"ipma" => parse_ipma(cursor, &parsed, state, issues)?,
            b"ispe" => parse_ispe(cursor, &parsed, state, issues)?,
            b"idat" => {
                state.idat_payloads.push((
                    cursor.absolute_offset(parsed.data_offset),
                    (parsed.end - parsed.data_offset) as u64,
                    parsed.path.clone(),
                ));
                issues.push(Issue {
                    severity: Severity::Info,
                    code: "isobmff_structure_recognized_uninterpreted".into(),
                    message: "recognized idat structure and retained it for item routing".into(),
                    offset: Some(cursor.absolute_offset(offset)),
                    context: Some(parsed.path.clone()),
                });
            }
            b"Exif" => {
                payloads.push(IsobmffPayload {
                    kind: "exif",
                    offset_start: cursor.absolute_offset(offset),
                    offset_end: cursor.absolute_offset(parsed.end),
                    data_offset: cursor.absolute_offset(parsed.data_offset),
                    data_length: (parsed.end - parsed.data_offset) as u64,
                    path: parsed.path.clone(),
                });
            }
            b"mime" => {
                if let Some(payload) = parse_mime_payload(cursor, &parsed) {
                    payloads.push(payload);
                } else {
                    issues.push(Issue {
                        severity: Severity::Info,
                        code: "isobmff_mime_payload_unsupported".into(),
                        message:
                            "mime box was recognized but did not contain a supported XMP payload"
                                .into(),
                        offset: Some(cursor.absolute_offset(offset)),
                        context: Some(parsed.path.clone()),
                    });
                }
            }
            b"xml " => {
                payloads.push(IsobmffPayload {
                    kind: "xmp",
                    offset_start: cursor.absolute_offset(offset),
                    offset_end: cursor.absolute_offset(parsed.end),
                    data_offset: cursor.absolute_offset(parsed.data_offset),
                    data_length: (parsed.end - parsed.data_offset) as u64,
                    path: parsed.path.clone(),
                });
            }
            b"iref" => {
                issues.push(Issue {
                    severity: Severity::Info,
                    code: "isobmff_structure_recognized_uninterpreted".into(),
                    message: format!(
                        "recognized {} structure but do not yet interpret its full semantics",
                        fourcc(parsed.box_type)
                    ),
                    offset: Some(cursor.absolute_offset(offset)),
                    context: Some(parsed.path.clone()),
                });
            }
            _ => {}
        }

        offset = parsed.end;
    }

    Ok(())
}

fn parse_box_header(
    cursor: &Cursor<'_>,
    offset: usize,
    parent_end: usize,
    parent: Option<&ParsedBox>,
    issues: &mut Vec<Issue>,
) -> Result<Option<ParsedBox>, XiftyError> {
    if offset + 8 > parent_end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_box_header_out_of_bounds".into(),
            message: "truncated isobmff box header".into(),
            offset: Some(cursor.absolute_offset(offset)),
            context: None,
        });
        return Ok(None);
    }

    let size32 = cursor.read_u32(offset, Endian::Big)? as u64;
    let type_slice = cursor.slice(offset + 4, 4)?;
    let box_type = [type_slice[0], type_slice[1], type_slice[2], type_slice[3]];
    let mut header_size = 8usize;
    let box_size = if size32 == 1 {
        if offset + 16 > parent_end {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "isobmff_large_size_header_truncated".into(),
                message: "large-size box header truncated".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some(fourcc(box_type)),
            });
            return Ok(None);
        }
        header_size = 16;
        let high = cursor.read_u32(offset + 8, Endian::Big)? as u64;
        let low = cursor.read_u32(offset + 12, Endian::Big)? as u64;
        (high << 32) | low
    } else if size32 == 0 {
        (parent_end - offset) as u64
    } else {
        size32
    };

    if box_size < header_size as u64 {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_box_size_invalid".into(),
            message: format!(
                "box {} declared invalid size {}",
                fourcc(box_type),
                box_size
            ),
            offset: Some(cursor.absolute_offset(offset)),
            context: Some(fourcc(box_type)),
        });
        return Ok(None);
    }

    let end = offset.saturating_add(box_size as usize);
    if end > parent_end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_box_size_out_of_bounds".into(),
            message: format!("box {} exceeds containing bounds", fourcc(box_type)),
            offset: Some(cursor.absolute_offset(offset)),
            context: Some(fourcc(box_type)),
        });
        return Ok(None);
    }

    let path = if let Some(parent) = parent {
        format!("{}/{}", parent.path, fourcc(box_type))
    } else {
        fourcc(box_type)
    };

    Ok(Some(ParsedBox {
        start: offset,
        box_type,
        data_offset: offset + header_size,
        end,
        path,
    }))
}

fn parse_ftyp(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    major_brand: &mut [u8; 4],
    compatible_brands: &mut Vec<[u8; 4]>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 8 > parsed.end {
        return Err(XiftyError::Parse {
            message: "ftyp box too small".into(),
        });
    }
    let major = cursor.slice(parsed.data_offset, 4)?;
    *major_brand = [major[0], major[1], major[2], major[3]];
    compatible_brands.clear();
    let mut offset = parsed.data_offset + 8;
    while offset + 4 <= parsed.end {
        let brand = cursor.slice(offset, 4)?;
        compatible_brands.push([brand[0], brand[1], brand[2], brand[3]]);
        offset += 4;
    }
    Ok(())
}

fn parse_iinf(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    _state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<usize, XiftyError> {
    if parsed.data_offset + 6 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_iinf_truncated".into(),
            message: "iinf box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(parsed.end);
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    Ok(parsed.data_offset + if version == 0 { 6 } else { 8 })
}

fn parse_infe(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 4 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_infe_truncated".into(),
            message: "infe box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    if version < 2 {
        return Ok(());
    }

    let mut offset = parsed.data_offset + 4;
    let item_id = cursor.read_u16(offset, Endian::Big)? as u32;
    offset += 2;
    offset += 2; // item_protection_index
    let item_type_bytes = cursor.slice(offset, 4)?;
    let item_type = [
        item_type_bytes[0],
        item_type_bytes[1],
        item_type_bytes[2],
        item_type_bytes[3],
    ];
    offset += 4;

    let payload = cursor.bytes().get(offset..parsed.end).unwrap_or_default();
    let mut parts = payload.split(|byte| *byte == 0);
    let _name = parts.next().unwrap_or_default();
    let content_type = if &item_type == b"mime" {
        parts
            .next()
            .and_then(|value| std::str::from_utf8(value).ok())
            .map(str::to_string)
    } else {
        None
    };

    state.item_infos.push(ItemInfo {
        item_id,
        item_type,
        content_type,
    });
    Ok(())
}

fn parse_pitm(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 6 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_pitm_truncated".into(),
            message: "pitm box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    let item_id = if version == 0 {
        cursor.read_u16(parsed.data_offset + 4, Endian::Big)? as u32
    } else {
        cursor.read_u32(parsed.data_offset + 4, Endian::Big)?
    };
    state.primary_item_id = Some(item_id);
    Ok(())
}

fn parse_iloc(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 8 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_iloc_truncated".into(),
            message: "iloc box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    let sizes = cursor.read_u8(parsed.data_offset + 4)?;
    let offset_size = (sizes >> 4) as usize;
    let length_size = (sizes & 0x0F) as usize;
    let base_and_index = cursor.read_u8(parsed.data_offset + 5)?;
    let base_offset_size = (base_and_index >> 4) as usize;
    let index_size = if version >= 1 {
        (base_and_index & 0x0F) as usize
    } else {
        0
    };
    let mut offset = parsed.data_offset + 6;
    let item_count = if version < 2 {
        cursor.read_u16(offset, Endian::Big)? as u32
    } else {
        cursor.read_u32(offset, Endian::Big)?
    };
    offset += if version < 2 { 2 } else { 4 };

    for _ in 0..item_count {
        let item_id = if version < 2 {
            let value = cursor.read_u16(offset, Endian::Big)? as u32;
            offset += 2;
            value
        } else {
            let value = cursor.read_u32(offset, Endian::Big)?;
            offset += 4;
            value
        };
        let construction_method = if version == 1 || version == 2 {
            let value = cursor.read_u16(offset, Endian::Big)? & 0x000F;
            offset += 2;
            value
        } else {
            0
        };
        offset += 2; // data_reference_index
        let base_offset = read_sized_uint(cursor, &mut offset, base_offset_size)?;
        let extent_count = cursor.read_u16(offset, Endian::Big)? as usize;
        offset += 2;

        let mut extents = Vec::new();
        for _ in 0..extent_count {
            if (version == 1 || version == 2) && index_size > 0 {
                let _ = read_sized_uint(cursor, &mut offset, index_size)?;
            }
            let extent_offset = read_sized_uint(cursor, &mut offset, offset_size)?;
            let extent_length = read_sized_uint(cursor, &mut offset, length_size)?;
            extents.push(ItemExtent {
                offset: extent_offset,
                length: extent_length,
            });
        }

        state.item_locations.push(ItemLocation {
            item_id,
            construction_method,
            base_offset,
            extents,
        });
    }

    Ok(())
}

fn parse_ipma(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 8 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_ipma_truncated".into(),
            message: "ipma box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    let flags = ((cursor.read_u8(parsed.data_offset + 1)? as u32) << 16)
        | ((cursor.read_u8(parsed.data_offset + 2)? as u32) << 8)
        | (cursor.read_u8(parsed.data_offset + 3)? as u32);
    let association_16 = (flags & 1) != 0;
    let mut offset = parsed.data_offset + 4;
    let entry_count = cursor.read_u32(offset, Endian::Big)? as usize;
    offset += 4;

    for _ in 0..entry_count {
        let item_id = if version < 1 {
            let value = cursor.read_u16(offset, Endian::Big)? as u32;
            offset += 2;
            value
        } else {
            let value = cursor.read_u32(offset, Endian::Big)?;
            offset += 4;
            value
        };
        let association_count = cursor.read_u8(offset)? as usize;
        offset += 1;
        let mut property_indexes = Vec::new();
        for _ in 0..association_count {
            let value = if association_16 {
                let raw = cursor.read_u16(offset, Endian::Big)?;
                offset += 2;
                raw & 0x7FFF
            } else {
                let raw = cursor.read_u8(offset)? as u16;
                offset += 1;
                raw & 0x7F
            };
            if value != 0 {
                property_indexes.push(value);
            }
        }
        state.property_associations.push(PropertyAssociation {
            item_id,
            property_indexes,
        });
    }

    Ok(())
}

fn parse_ispe(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 12 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_ispe_truncated".into(),
            message: "ispe box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let width = cursor.read_u32(parsed.data_offset + 4, Endian::Big)?;
    let height = cursor.read_u32(parsed.data_offset + 8, Endian::Big)?;
    let property_index = (state.property_dimensions.len() + 1) as u16;
    state.property_dimensions.insert(
        property_index,
        IsobmffDimensions {
            width,
            height,
            offset_start: cursor.absolute_offset(parsed.start),
            offset_end: cursor.absolute_offset(parsed.end),
            path: parsed.path.clone(),
        },
    );
    Ok(())
}

fn payloads_from_items(
    cursor: &Cursor<'_>,
    state: &ParseState,
    issues: &mut Vec<Issue>,
) -> Vec<IsobmffPayload> {
    let mut payloads = Vec::new();

    for info in &state.item_infos {
        let Some(kind) = metadata_item_kind(info) else {
            continue;
        };
        let Some(location) = state
            .item_locations
            .iter()
            .find(|location| location.item_id == info.item_id)
        else {
            continue;
        };
        if location.extents.len() != 1 {
            issues.push(Issue {
                severity: Severity::Info,
                code: "isobmff_item_multi_extent_unsupported".into(),
                message: format!(
                    "metadata item {} uses {} extents; only single-extent routing is supported",
                    info.item_id,
                    location.extents.len()
                ),
                offset: None,
                context: Some(format!("item {}", info.item_id)),
            });
            continue;
        }

        let extent = &location.extents[0];
        let absolute_data_offset = match location.construction_method {
            0 => location.base_offset + extent.offset,
            1 => {
                let Some((idat_offset, _, path)) = state.idat_payloads.first() else {
                    issues.push(Issue {
                        severity: Severity::Info,
                        code: "isobmff_item_idat_missing".into(),
                        message: format!(
                            "metadata item {} requires idat construction, but no idat payload was found",
                            info.item_id
                        ),
                        offset: None,
                        context: Some(format!("item {}", info.item_id)),
                    });
                    continue;
                };
                let _ = path;
                idat_offset + location.base_offset + extent.offset
            }
            method => {
                issues.push(Issue {
                    severity: Severity::Info,
                    code: "isobmff_item_construction_unsupported".into(),
                    message: format!(
                        "metadata item {} uses unsupported construction method {}",
                        info.item_id, method
                    ),
                    offset: None,
                    context: Some(format!("item {}", info.item_id)),
                });
                continue;
            }
        };

        let Ok(start) = usize::try_from(absolute_data_offset) else {
            continue;
        };
        let Ok(length) = usize::try_from(extent.length) else {
            continue;
        };
        if cursor.bytes().get(start..start + length).is_none() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "isobmff_item_payload_out_of_bounds".into(),
                message: format!("metadata item {} points outside file bounds", info.item_id),
                offset: Some(absolute_data_offset),
                context: Some(format!("item {}", info.item_id)),
            });
            continue;
        }

        payloads.push(IsobmffPayload {
            kind,
            offset_start: absolute_data_offset,
            offset_end: absolute_data_offset + extent.length,
            data_offset: absolute_data_offset,
            data_length: extent.length,
            path: format!("meta/item[{}/{}]", info.item_id, fourcc(info.item_type)),
        });
    }

    payloads
}

fn metadata_item_kind(info: &ItemInfo) -> Option<&'static str> {
    if &info.item_type == b"Exif" {
        return Some("exif");
    }
    if &info.item_type == b"mime" {
        return match info.content_type.as_deref() {
            Some("application/rdf+xml" | "application/xml" | "text/xml") => Some("xmp"),
            _ => None,
        };
    }
    None
}

fn primary_item_dimensions(state: &ParseState) -> Option<IsobmffDimensions> {
    let item_id = state.primary_item_id?;
    let associations = state
        .property_associations
        .iter()
        .find(|association| association.item_id == item_id)?;
    for property_index in &associations.property_indexes {
        if let Some(dimensions) = state.property_dimensions.get(property_index) {
            return Some(dimensions.clone());
        }
    }
    None
}

fn read_sized_uint(
    cursor: &Cursor<'_>,
    offset: &mut usize,
    size: usize,
) -> Result<u64, XiftyError> {
    if size == 0 {
        return Ok(0);
    }
    if size > 8 {
        return Err(XiftyError::Parse {
            message: format!("unsupported integer size {size} in isobmff parser"),
        });
    }
    let bytes = cursor.slice(*offset, size)?;
    *offset += size;
    let mut value = 0u64;
    for byte in bytes {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

fn parse_mime_payload(cursor: &Cursor<'_>, parsed: &ParsedBox) -> Option<IsobmffPayload> {
    let payload = cursor.bytes().get(parsed.data_offset..parsed.end)?;
    let nul = payload.iter().position(|byte| *byte == 0)?;
    let mime = std::str::from_utf8(&payload[..nul]).ok()?;
    if mime != "application/rdf+xml" && mime != "application/xml" && mime != "text/xml" {
        return None;
    }
    let data_offset = parsed.data_offset + nul + 1;
    if data_offset >= parsed.end {
        return None;
    }
    Some(IsobmffPayload {
        kind: "xmp",
        offset_start: cursor.absolute_offset(parsed.start),
        offset_end: cursor.absolute_offset(parsed.end),
        data_offset: cursor.absolute_offset(data_offset),
        data_length: (parsed.end - data_offset) as u64,
        path: parsed.path.clone(),
    })
}

fn fourcc(value: [u8; 4]) -> String {
    String::from_utf8_lossy(&value).into_owned()
}

fn heif_brand(brand: [u8; 4]) -> bool {
    matches!(
        &brand,
        b"mif1" | b"msf1" | b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"avif" | b"avis"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let size = (8 + data.len()) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out
    }

    fn full_box(kind: &[u8; 4], version_flags: [u8; 4], children: &[u8]) -> Vec<u8> {
        let mut data = version_flags.to_vec();
        data.extend_from_slice(children);
        boxed(kind, &data)
    }

    #[test]
    fn parses_minimal_heif() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"heic\0\0\0\0mif1"));
        bytes.extend_from_slice(&full_box(
            b"meta",
            [0, 0, 0, 0],
            &boxed(b"Exif", b"II*\0\x08\0\0\0\x00\x00\0\0\0\0"),
        ));

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.major_brand, b"heic");
        assert!(parsed.is_heif_still_image());
        assert_eq!(parsed.exif_payloads().count(), 1);
    }

    #[test]
    fn parses_item_based_metadata_and_primary_dimensions() {
        let bytes = include_bytes!("../../../fixtures/minimal/real_exif.heic");
        let parsed = parse_bytes(bytes, 0).unwrap();
        assert_eq!(parsed.exif_payloads().count(), 1);
        let dimensions = parsed.primary_item_dimensions.unwrap();
        assert_eq!((dimensions.width, dimensions.height), (700, 476));
    }

    #[test]
    fn parses_nested_meta_and_xmp_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"mif1\0\0\0\0heic"));
        bytes.extend_from_slice(&full_box(
            b"meta",
            [0, 0, 0, 0],
            &boxed(
                b"mime",
                b"application/rdf+xml\0<x:xmpmeta><rdf:Description tiff:Make=\"XIFtyCam\" /></x:xmpmeta>",
            ),
        ));

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.xmp_payloads().count(), 1);
        assert!(parsed.nodes.iter().any(|node| node.label == "meta/mime"));
    }

    #[test]
    fn reports_box_size_out_of_bounds() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"heic\0\0\0\0mif1"));
        bytes.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x40, b'm', b'e', b't', b'a', 0x00, 0x00, 0x00, 0x00,
        ]);

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|issue| issue.code == "isobmff_box_size_out_of_bounds")
        );
    }
}
