use xifty_container_tiff::TiffContainer;
use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

const SONY_MAKER_NOTE_HEADER: &[u8] = b"SONY DSC \0\0\0";
const SONY_DECIPHER_TABLE: [u8; 256] = [
    0, 1, 50, 177, 10, 14, 135, 40, 2, 204, 202, 173, 27, 220, 8, 237, 100, 134, 240, 79, 140, 108,
    184, 203, 105, 196, 44, 3, 151, 182, 147, 124, 20, 243, 226, 62, 48, 142, 215, 96, 28, 161,
    171, 55, 236, 117, 190, 35, 21, 106, 89, 63, 208, 185, 150, 181, 80, 39, 136, 227, 129, 148,
    224, 192, 4, 92, 198, 232, 95, 75, 112, 56, 159, 130, 128, 81, 43, 197, 69, 73, 155, 33, 82,
    83, 84, 133, 11, 93, 97, 218, 123, 85, 38, 36, 7, 110, 54, 91, 71, 183, 217, 74, 162, 223, 191,
    18, 37, 188, 30, 127, 86, 234, 16, 230, 207, 103, 77, 60, 145, 131, 225, 49, 179, 111, 244, 5,
    138, 70, 200, 24, 118, 104, 189, 172, 146, 42, 19, 233, 15, 163, 122, 219, 61, 212, 231, 58,
    26, 87, 175, 32, 66, 178, 158, 195, 139, 242, 213, 211, 164, 126, 31, 152, 156, 238, 116, 165,
    166, 167, 216, 94, 176, 180, 52, 206, 168, 121, 119, 90, 193, 137, 174, 154, 17, 51, 157, 245,
    57, 25, 101, 120, 22, 113, 210, 169, 68, 99, 64, 41, 186, 160, 143, 228, 214, 59, 132, 13, 194,
    78, 88, 221, 153, 34, 107, 201, 187, 23, 6, 229, 125, 102, 67, 98, 246, 205, 53, 144, 46, 65,
    141, 109, 170, 9, 115, 149, 12, 241, 29, 222, 76, 47, 45, 247, 209, 114, 235, 239, 72, 199,
    248, 249, 250, 251, 252, 253, 254, 255,
];

#[derive(Debug, Clone, Copy)]
struct MakerEntry {
    tag_id: u16,
    type_id: u16,
    count: u32,
    value_or_offset: u32,
}

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
    if !matches!(make, Some(value) if value.eq_ignore_ascii_case("SONY")) {
        return Vec::new();
    }

    let Some(maker_note) = tiff.entries.iter().find(|entry| entry.tag_id == 0x927C) else {
        return Vec::new();
    };
    let start = maker_note.value_or_offset as usize;
    let Some(end) = start.checked_add(maker_note.count as usize) else {
        return Vec::new();
    };
    let Some(maker_bytes) = bytes.get(start..end) else {
        return Vec::new();
    };
    if !maker_bytes.starts_with(SONY_MAKER_NOTE_HEADER) {
        return Vec::new();
    }

    let cursor = Cursor::new(bytes, base_offset);
    let top_level = parse_ifd_entries(&cursor, tiff.endian, start + SONY_MAKER_NOTE_HEADER.len());
    let mut entries = Vec::new();

    decode_plain_entries(
        &mut entries,
        container_name,
        &cursor,
        tiff.endian,
        &top_level,
        "sony_makernote",
    );
    decode_shot_info(
        &mut entries,
        container_name,
        &cursor,
        tiff.endian,
        &top_level,
    );
    decode_af_points(
        &mut entries,
        container_name,
        &cursor,
        tiff.endian,
        &top_level,
    );
    decode_tag_9401(&mut entries, container_name, &cursor, &top_level);
    decode_tag_9400(&mut entries, container_name, &cursor, &top_level);
    decode_tag_9402(&mut entries, container_name, &cursor, &top_level);
    decode_tag_9405(&mut entries, container_name, &cursor, &top_level);
    decode_tag_9406(&mut entries, container_name, &cursor, &top_level);
    decode_tag_940c(&mut entries, container_name, &cursor, &top_level);
    decode_tag_9050(&mut entries, container_name, &cursor, &top_level);
    decode_tag_2010(&mut entries, container_name, &cursor, &top_level);
    decode_tag_9416(&mut entries, container_name, &cursor, &top_level);

    entries
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

fn decode_plain_entries(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    endian: Endian,
    entries: &[MakerEntry],
    path: &str,
) {
    for entry in entries {
        match entry.tag_id {
            0x2002 => push_integer(
                out,
                container_name,
                "Rating",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(|value| value as i64),
            ),
            0x2004 => push_integer(
                out,
                container_name,
                "Contrast",
                entry.tag_id,
                path,
                read_i32(cursor, endian, entry).map(|value| value as i64),
            ),
            0x2005 => push_integer(
                out,
                container_name,
                "Saturation",
                entry.tag_id,
                path,
                read_i32(cursor, endian, entry).map(|value| value as i64),
            ),
            0x2006 => push_integer(
                out,
                container_name,
                "Sharpness",
                entry.tag_id,
                path,
                read_i32(cursor, endian, entry).map(|value| value as i64),
            ),
            0x2007 => push_integer(
                out,
                container_name,
                "Brightness",
                entry.tag_id,
                path,
                read_i32(cursor, endian, entry).map(|value| value as i64),
            ),
            0x2008 => push_string(
                out,
                container_name,
                "LongExposureNoiseReduction",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(long_exposure_nr),
            ),
            0x2009 => push_string(
                out,
                container_name,
                "HighISONoiseReduction",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(high_iso_nr),
            ),
            0x200A => push_string(
                out,
                container_name,
                "HDR",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(format_hdr),
            ),
            0x2014 => push_string(
                out,
                container_name,
                "WBShiftAB_GM",
                entry.tag_id,
                path,
                read_i32_pair(cursor, endian, entry).map(|(a, b)| format!("{a} {b}")),
            ),
            0x2026 => push_string(
                out,
                container_name,
                "WBShiftAB_GM_Precise",
                entry.tag_id,
                path,
                read_i32_pair(cursor, endian, entry)
                    .map(|(a, b)| format!("{:.2} {:.2}", a as f64, b as f64)),
            ),
            0xB020 => push_string(
                out,
                container_name,
                "CreativeStyle",
                entry.tag_id,
                path,
                read_ascii(cursor, endian, entry),
            ),
            0xB021 => push_string(
                out,
                container_name,
                "ColorTemperature",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(|value| {
                    if value == 0 {
                        "Auto".into()
                    } else {
                        value.to_string()
                    }
                }),
            ),
            0xB022 => push_integer(
                out,
                container_name,
                "ColorCompensationFilter",
                entry.tag_id,
                path,
                read_i32(cursor, endian, entry).map(|value| value as i64),
            ),
            0xB023 => push_string(
                out,
                container_name,
                "SceneMode",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(scene_mode),
            ),
            0xB024 => push_string(
                out,
                container_name,
                "ZoneMatching",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(zone_matching),
            ),
            0xB025 => push_string(
                out,
                container_name,
                "DynamicRangeOptimizer",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(dynamic_range_optimizer),
            ),
            0xB026 => push_string(
                out,
                container_name,
                "ImageStabilization",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(boolean_off_on),
            ),
            0xB029 => push_string(
                out,
                container_name,
                "ColorMode",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(color_mode),
            ),
            0xB02B => push_string(
                out,
                container_name,
                "FullImageSize",
                entry.tag_id,
                path,
                read_u32_pair(cursor, endian, entry).map(|(a, b)| format!("{b}x{a}")),
            ),
            0xB02C => push_string(
                out,
                container_name,
                "PreviewImageSize",
                entry.tag_id,
                path,
                read_u32_pair(cursor, endian, entry).map(|(a, b)| format!("{b}x{a}")),
            ),
            0xB000 => push_string(
                out,
                container_name,
                "FileFormat",
                entry.tag_id,
                path,
                read_u8_vec(cursor, entry).map(format_file_format),
            ),
            0x0102 => push_string(
                out,
                container_name,
                "Quality",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(quality),
            ),
            0x0104 => push_string(
                out,
                container_name,
                "FlashExposureComp",
                entry.tag_id,
                path,
                read_rational_string(cursor, endian, entry, 10.0),
            ),
            0x0112 => push_integer(
                out,
                container_name,
                "WhiteBalanceFineTune",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(|value| value as i64),
            ),
            0x0115 => push_string(
                out,
                container_name,
                "WhiteBalance",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(white_balance),
            ),
            0xB001 => push_integer(
                out,
                container_name,
                "SonyModelID",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(|value| value as i64),
            ),
            0x200B => push_string(
                out,
                container_name,
                "MultiFrameNoiseReduction",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(multi_frame_nr),
            ),
            0x200E => push_string(
                out,
                container_name,
                "PictureEffect",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(picture_effect),
            ),
            0x200F => push_string(
                out,
                container_name,
                "SoftSkinEffect",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(soft_skin_effect),
            ),
            0x2011 => push_string(
                out,
                container_name,
                "VignettingCorrection",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(correction_setting),
            ),
            0x2012 => push_string(
                out,
                container_name,
                "LateralChromaticAberration",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(correction_setting),
            ),
            0x2013 => push_string(
                out,
                container_name,
                "DistortionCorrectionSetting",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(correction_setting),
            ),
            0x2016 => push_string(
                out,
                container_name,
                "AutoPortraitFramed",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(yes_no),
            ),
            0x2017 => push_string(
                out,
                container_name,
                "FlashAction",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(flash_action),
            ),
            0x201A => push_string(
                out,
                container_name,
                "ElectronicFrontCurtainShutter",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(boolean_off_on),
            ),
            0x201B => push_string(
                out,
                container_name,
                "FocusMode",
                entry.tag_id,
                path,
                read_u8(cursor, entry).map(focus_mode),
            ),
            0x201C => push_string(
                out,
                container_name,
                "AFAreaModeSetting",
                entry.tag_id,
                path,
                read_u8(cursor, entry).map(af_area_mode_setting),
            ),
            0x201D => push_string(
                out,
                container_name,
                "FlexibleSpotPosition",
                entry.tag_id,
                path,
                read_u16_pair(cursor, endian, entry).map(|(x, y)| format!("{x} {y}")),
            ),
            0x201E => push_string(
                out,
                container_name,
                "AFPointSelected",
                entry.tag_id,
                path,
                read_u8(cursor, entry).map(af_point_selected),
            ),
            0x2021 => push_string(
                out,
                container_name,
                "AFTracking",
                entry.tag_id,
                path,
                read_u8(cursor, entry).map(af_tracking),
            ),
            0x2023 => push_string(
                out,
                container_name,
                "MultiFrameNREffect",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(multi_frame_nr_effect),
            ),
            0x2027 => push_string(
                out,
                container_name,
                "FocusLocation",
                entry.tag_id,
                path,
                read_u16_quad(cursor, endian, entry).map(|(a, b, c, d)| format!("{a} {b} {c} {d}")),
            ),
            0x2028 => push_string(
                out,
                container_name,
                "VariableLowPassFilter",
                entry.tag_id,
                path,
                read_u16_pair(cursor, endian, entry).map(variable_low_pass_filter),
            ),
            0x202B => push_string(
                out,
                container_name,
                "PrioritySetInAWB",
                entry.tag_id,
                path,
                read_u8(cursor, entry).map(priority_set_in_awb),
            ),
            0x202C => push_string(
                out,
                container_name,
                "MeteringMode2",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(metering_mode2),
            ),
            0x202D => push_string(
                out,
                container_name,
                "ExposureStandardAdjustment",
                entry.tag_id,
                path,
                read_rational_string(cursor, endian, entry, 6.0),
            ),
            0x2029 => push_string(
                out,
                container_name,
                "RAWFileType",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(raw_file_type),
            ),
            0x202F => push_string(
                out,
                container_name,
                "PixelShiftInfo",
                entry.tag_id,
                path,
                raw_value_bytes(cursor, endian, entry).map(pixel_shift_info),
            ),
            0xB041 => push_string(
                out,
                container_name,
                "ExposureMode",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(exposure_mode),
            ),
            0xB048 => push_string(
                out,
                container_name,
                "FlashLevel",
                entry.tag_id,
                path,
                read_i16(cursor, endian, entry).map(flash_level),
            ),
            0xB049 => push_string(
                out,
                container_name,
                "ReleaseMode",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(release_mode),
            ),
            0xB04A => push_integer(
                out,
                container_name,
                "SequenceNumber",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(|value| value as i64),
            ),
            0xB04B => push_string(
                out,
                container_name,
                "Anti-Blur",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(anti_blur),
            ),
            0xB04F => push_string(
                out,
                container_name,
                "DynamicRangeOptimizer",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(dynamic_range_optimizer_short),
            ),
            0xB052 => push_string(
                out,
                container_name,
                "IntelligentAuto",
                entry.tag_id,
                path,
                read_u16(cursor, endian, entry).map(boolean_off_on_short),
            ),
            0xB027 => push_string(
                out,
                container_name,
                "LensType",
                entry.tag_id,
                path,
                read_u32(cursor, endian, entry).map(|value| lens_type_name(value as u16)),
            ),
            0xB02A => push_string(
                out,
                container_name,
                "LensSpec",
                entry.tag_id,
                path,
                raw_value_bytes(cursor, endian, entry).map(|bytes| lens_spec(&bytes)),
            ),
            _ => {}
        }
    }
}

fn decode_shot_info(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    endian: Endian,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x3000) else {
        return;
    };
    let Some(bytes) = raw_value_bytes(cursor, endian, entry) else {
        return;
    };
    push_integer(
        out,
        container_name,
        "FaceInfoOffset",
        entry.tag_id,
        "sony_shot_info",
        le_u16_at(&bytes, 0x02).map(|value| value as i64),
    );
    push_timestamp(
        out,
        container_name,
        "SonyDateTime",
        entry.tag_id,
        "sony_shot_info",
        bytes.get(6..26).map(trim_c_string_bytes),
    );
    push_integer(
        out,
        container_name,
        "SonyImageHeight",
        entry.tag_id,
        "sony_shot_info",
        le_u16_at(&bytes, 0x1a).map(|value| value as i64),
    );
    push_integer(
        out,
        container_name,
        "SonyImageWidth",
        entry.tag_id,
        "sony_shot_info",
        le_u16_at(&bytes, 0x1c).map(|value| value as i64),
    );
    push_integer(
        out,
        container_name,
        "FacesDetected",
        entry.tag_id,
        "sony_shot_info",
        le_u16_at(&bytes, 0x30).map(|value| value as i64),
    );
    push_integer(
        out,
        container_name,
        "FaceInfoLength",
        entry.tag_id,
        "sony_shot_info",
        le_u16_at(&bytes, 0x32).map(|value| value as i64),
    );
    push_string(
        out,
        container_name,
        "MetaVersion",
        entry.tag_id,
        "sony_shot_info",
        bytes.get(0x34..0x44).map(trim_c_string_bytes),
    );
}

fn decode_af_points(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    endian: Endian,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x202A) else {
        return;
    };
    let Some(bytes) = raw_value_bytes(cursor, endian, entry) else {
        return;
    };
    let count = bytes.get(1).copied().unwrap_or_default() as usize;
    push_integer(
        out,
        container_name,
        "FocalPlaneAFPointsUsed",
        entry.tag_id,
        "sony_af_points",
        Some(count as i64),
    );
    if let (Some(width), Some(height)) = (le_u16_at(&bytes, 0x02), le_u16_at(&bytes, 0x04)) {
        push_string(
            out,
            container_name,
            "FocalPlaneAFPointArea",
            entry.tag_id,
            "sony_af_points",
            Some(format!("{width} {height}")),
        );
    }
    for index in 0..count.min(15) {
        let offset = 0x06 + index * 4;
        if let (Some(x), Some(y)) = (le_u16_at(&bytes, offset), le_u16_at(&bytes, offset + 2)) {
            push_string(
                out,
                container_name,
                &format!("FocalPlaneAFPointLocation{}", index + 1),
                entry.tag_id,
                "sony_af_points",
                Some(format!("{x} {y}")),
            );
        }
    }
}

fn decode_tag_9401(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x9401) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    let version = bytes.first().copied().unwrap_or_default();
    let iso_offset = match version {
        68 => 0x0634,
        73 | 74 => 0x0636,
        78 => 0x064c,
        90 => 0x0653,
        93 | 94 => 0x0678,
        100 | 103 => 0x06b8,
        124 | 125 => 0x06de,
        127 | 128 | 130 => 0x06e7,
        144 | 146 => 0x059d,
        148 => 0x0498,
        152 | 154 | 155 => 0x04a2,
        160 | 164 => 0x04a1,
        167 => 0x049d,
        178 => 0x044e,
        181 => 0x03e2,
        185 | 186 | 187 => 0x03f4,
        198 => 0x0453,
        _ => return,
    };
    push_string(
        out,
        container_name,
        "ISOSetting",
        entry.tag_id,
        "sony_tag9401",
        bytes.get(iso_offset).copied().map(iso_setting),
    );
    push_string(
        out,
        container_name,
        "ISOAutoMin",
        entry.tag_id,
        "sony_tag9401",
        bytes.get(iso_offset + 2).copied().map(iso_setting),
    );
    push_string(
        out,
        container_name,
        "ISOAutoMax",
        entry.tag_id,
        "sony_tag9401",
        bytes.get(iso_offset + 4).copied().map(iso_setting),
    );
}

fn decode_tag_9400(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x9400) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    push_integer(
        out,
        container_name,
        "SequenceImageNumber",
        entry.tag_id,
        "sony_tag9400",
        le_u32_at(&bytes, 0x0012).map(|value| value as i64 + 1),
    );
    push_string(
        out,
        container_name,
        "SequenceLength",
        entry.tag_id,
        "sony_tag9400",
        bytes.get(0x0016).copied().map(sequence_length),
    );
    push_integer(
        out,
        container_name,
        "SequenceFileNumber",
        entry.tag_id,
        "sony_tag9400",
        le_u32_at(&bytes, 0x001a).map(|value| value as i64 + 1),
    );
    push_string(
        out,
        container_name,
        "CameraOrientation",
        entry.tag_id,
        "sony_tag9400",
        bytes.get(0x0029).copied().map(camera_orientation),
    );
    push_string(
        out,
        container_name,
        "Quality2",
        entry.tag_id,
        "sony_tag9400",
        bytes.get(0x002a).copied().map(quality2_byte),
    );
    push_integer(
        out,
        container_name,
        "ModelReleaseYear",
        entry.tag_id,
        "sony_tag9400",
        bytes.get(0x0053).copied().map(model_release_year),
    );
}

fn decode_tag_9402(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x9402) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    if bytes.get(2).copied() == Some(255) {
        push_string(
            out,
            container_name,
            "AmbientTemperature",
            entry.tag_id,
            "sony_tag9402",
            bytes
                .get(4)
                .copied()
                .map(|value| format!("{} C", value as i8)),
        );
    }
    push_string(
        out,
        container_name,
        "FocusMode",
        entry.tag_id,
        "sony_tag9402",
        bytes
            .get(0x16)
            .copied()
            .map(|value| focus_mode(value & 0x7f)),
    );
    push_string(
        out,
        container_name,
        "AFAreaMode",
        entry.tag_id,
        "sony_tag9402",
        bytes.get(0x17).copied().map(af_area_mode),
    );
    push_integer(
        out,
        container_name,
        "FocusPosition2",
        entry.tag_id,
        "sony_tag9402",
        bytes.get(0x2d).copied().map(|value| value as i64),
    );
}

fn decode_tag_9405(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x9405) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    push_integer(
        out,
        container_name,
        "SonyISO",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x0004).map(|value| sony_iso(value) as i64),
    );
    push_integer(
        out,
        container_name,
        "BaseISO",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x0006).map(|value| sony_iso(value) as i64),
    );
    push_float(
        out,
        container_name,
        "StopsAboveBaseISO",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x000a).map(stops_above_base_iso),
    );
    push_string(
        out,
        container_name,
        "SonyExposureTime2",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x000e).map(|value| sony_exposure_time(value as f64)),
    );
    push_string(
        out,
        container_name,
        "ExposureTime",
        entry.tag_id,
        "sony_tag9405",
        rational32u_at(&bytes, 0x0010).map(format_exposure_time),
    );
    push_string(
        out,
        container_name,
        "SonyMaxApertureValue",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x0016).map(sony_f_number),
    );
    push_integer(
        out,
        container_name,
        "SequenceImageNumber",
        entry.tag_id,
        "sony_tag9405",
        le_u32_at(&bytes, 0x0024).map(|value| value as i64 + 1),
    );
    push_integer(
        out,
        container_name,
        "SonyImageWidthMax",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x003e).map(|value| value as i64),
    );
    push_integer(
        out,
        container_name,
        "SonyImageHeightMax",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x0040).map(|value| value as i64),
    );
    push_string(
        out,
        container_name,
        "PictureEffect2",
        entry.tag_id,
        "sony_tag9405",
        bytes.get(0x0046).copied().map(picture_effect2),
    );
    push_string(
        out,
        container_name,
        "ExposureProgram",
        entry.tag_id,
        "sony_tag9405",
        bytes.get(0x0048).copied().map(exposure_program3),
    );
    push_integer(
        out,
        container_name,
        "Sharpness",
        entry.tag_id,
        "sony_tag9405",
        bytes.get(0x0052).copied().map(|value| (value as i8) as i64),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrParamsPresent",
        entry.tag_id,
        "sony_tag9405",
        bytes.get(0x5a).copied().map(yes_no_byte),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrection",
        entry.tag_id,
        "sony_tag9405",
        bytes.get(0x5b).copied().map(distortion_correction),
    );
    push_string(
        out,
        container_name,
        "LensFormat",
        entry.tag_id,
        "sony_tag9405",
        bytes.get(0x5d).copied().map(lens_format),
    );
    push_string(
        out,
        container_name,
        "LensMount",
        entry.tag_id,
        "sony_tag9405",
        bytes.get(0x5e).copied().map(lens_mount),
    );
    push_string(
        out,
        container_name,
        "LensType2",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x60).map(lens_type2_name),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrParams",
        entry.tag_id,
        "sony_tag9405",
        int16_slice_string(&bytes, 0x64, 16),
    );
    push_string(
        out,
        container_name,
        "LensZoomPosition",
        entry.tag_id,
        "sony_tag9405",
        le_u16_at(&bytes, 0x034e).map(|value| format!("{:.0}%", value as f64 / 10.24)),
    );
    push_string(
        out,
        container_name,
        "VignettingCorrParams",
        entry.tag_id,
        "sony_tag9405",
        int16_slice_string(&bytes, 0x035c, 16),
    );
    push_string(
        out,
        container_name,
        "ChromaticAberrationCorrParams",
        entry.tag_id,
        "sony_tag9405",
        int16_slice_string(&bytes, 0x03b8, 32),
    );
}

fn decode_tag_9406(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x9406) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    if let Some(temp) = bytes.get(0x05) {
        let celsius = (*temp as f64 - 32.0) / 1.8;
        push_string(
            out,
            container_name,
            "BatteryTemperature",
            entry.tag_id,
            "sony_tag9406",
            Some(format!("{celsius:.1} C")),
        );
    }
    push_string(
        out,
        container_name,
        "BatteryLevel",
        entry.tag_id,
        "sony_tag9406",
        bytes.get(0x07).copied().map(|value| format!("{value}%")),
    );
}

fn decode_tag_940c(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x940C) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    push_string(
        out,
        container_name,
        "LensMount2",
        entry.tag_id,
        "sony_tag940c",
        bytes.get(0x08).copied().map(lens_mount2),
    );
    push_string(
        out,
        container_name,
        "LensType3",
        entry.tag_id,
        "sony_tag940c",
        le_u16_at(&bytes, 0x09).map(lens_type2_name),
    );
    push_string(
        out,
        container_name,
        "CameraE-mountVersion",
        entry.tag_id,
        "sony_tag940c",
        le_u16_at(&bytes, 0x0b).map(version_u16),
    );
    push_string(
        out,
        container_name,
        "LensE-mountVersion",
        entry.tag_id,
        "sony_tag940c",
        le_u16_at(&bytes, 0x0d).map(version_u16),
    );
    push_string(
        out,
        container_name,
        "LensFirmwareVersion",
        entry.tag_id,
        "sony_tag940c",
        le_u16_at(&bytes, 0x14).map(lens_firmware_version),
    );
}

fn decode_tag_9050(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x9050) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    push_string(
        out,
        container_name,
        "Shutter",
        entry.tag_id,
        "sony_tag9050",
        int16_triplet_at(&bytes, 0x26).map(|(a, b, c)| format!("Mechanical ({a} {b} {c})")),
    );
    push_string(
        out,
        container_name,
        "FlashStatus",
        entry.tag_id,
        "sony_tag9050",
        bytes.get(0x39).copied().map(flash_status),
    );
    push_integer(
        out,
        container_name,
        "ShutterCount",
        entry.tag_id,
        "sony_tag9050",
        le_u32_at(&bytes, 0x3a).map(|value| (value & 0x00ff_ffff) as i64),
    );
    push_string(
        out,
        container_name,
        "SonyExposureTime",
        entry.tag_id,
        "sony_tag9050",
        le_u16_at(&bytes, 0x46).map(|value| sony_exposure_time(value as f64)),
    );
    push_string(
        out,
        container_name,
        "SonyFNumber",
        entry.tag_id,
        "sony_tag9050",
        le_u16_at(&bytes, 0x48).map(sony_f_number),
    );
    push_string(
        out,
        container_name,
        "ReleaseMode2",
        entry.tag_id,
        "sony_tag9050",
        bytes.get(0x4b).copied().map(release_mode2),
    );
    push_integer(
        out,
        container_name,
        "ShutterCount2",
        entry.tag_id,
        "sony_tag9050",
        le_u32_at(&bytes, 0x50).map(|value| (value & 0x00ff_ffff) as i64),
    );
    push_string(
        out,
        container_name,
        "InternalSerialNumber",
        entry.tag_id,
        "sony_tag9050",
        bytes.get(0x88..0x8e).map(hex_bytes),
    );
    push_string(
        out,
        container_name,
        "LensMount",
        entry.tag_id,
        "sony_tag9050",
        bytes.get(0x0105).copied().map(lens_mount),
    );
    push_string(
        out,
        container_name,
        "LensFormat",
        entry.tag_id,
        "sony_tag9050",
        bytes.get(0x0106).copied().map(lens_format),
    );
    push_string(
        out,
        container_name,
        "LensType2",
        entry.tag_id,
        "sony_tag9050",
        le_u16_at(&bytes, 0x0107).map(lens_type2_name),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrParamsPresent",
        entry.tag_id,
        "sony_tag9050",
        bytes.get(0x010b).copied().map(yes_no_byte),
    );
    push_string(
        out,
        container_name,
        "LensSpecFeatures",
        entry.tag_id,
        "sony_tag9050",
        bytes.get(0x0116..0x0118).map(lens_spec_features),
    );
    push_integer(
        out,
        container_name,
        "ShutterCount3",
        entry.tag_id,
        "sony_tag9050",
        le_u32_at(&bytes, 0x019f).map(|value| (value & 0x00ff_ffff) as i64),
    );
}

fn decode_tag_2010(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x2010) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    push_string(
        out,
        container_name,
        "ReleaseMode3",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x0204).copied().map(release_mode2),
    );
    push_string(
        out,
        container_name,
        "SelfTimer",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x0210).copied().map(self_timer),
    );
    push_string(
        out,
        container_name,
        "FlashMode",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x0211).copied().map(flash_mode),
    );
    push_float(
        out,
        container_name,
        "StopsAboveBaseISO",
        entry.tag_id,
        "sony_tag2010",
        le_u16_at(&bytes, 0x0217).map(stops_above_base_iso),
    );
    push_float(
        out,
        container_name,
        "BrightnessValue",
        entry.tag_id,
        "sony_tag2010",
        le_u16_at(&bytes, 0x0219).map(brightness_value_2010),
    );
    push_string(
        out,
        container_name,
        "HDRSetting",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x021f).copied().map(hdr_setting),
    );
    push_float(
        out,
        container_name,
        "ExposureCompensation",
        entry.tag_id,
        "sony_tag2010",
        le_i16_at(&bytes, 0x0223).map(exposure_compensation_2010),
    );
    push_string(
        out,
        container_name,
        "PictureProfile",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x0237).copied().map(picture_profile),
    );
    push_string(
        out,
        container_name,
        "PictureEffect2",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x023c).copied().map(picture_effect2),
    );
    push_string(
        out,
        container_name,
        "Quality2",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x0247).copied().map(quality2_byte),
    );
    push_string(
        out,
        container_name,
        "MeteringMode",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x024b).copied().map(metering_mode_2010),
    );
    push_string(
        out,
        container_name,
        "ExposureProgram",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x024c).copied().map(exposure_program3),
    );
    push_string(
        out,
        container_name,
        "WB_RGBLevels",
        entry.tag_id,
        "sony_tag2010",
        u16_slice_string(&bytes, 0x0252, 3),
    );
    push_string(
        out,
        container_name,
        "FocalLength",
        entry.tag_id,
        "sony_tag2010",
        le_u16_at(&bytes, 0x030a).map(|value| format!("{:.1} mm", value as f64 / 10.0)),
    );
    push_string(
        out,
        container_name,
        "MinFocalLength",
        entry.tag_id,
        "sony_tag2010",
        le_u16_at(&bytes, 0x030c).map(|value| format!("{:.1} mm", value as f64 / 10.0)),
    );
    push_string(
        out,
        container_name,
        "MaxFocalLength",
        entry.tag_id,
        "sony_tag2010",
        le_u16_at(&bytes, 0x030e)
            .filter(|value| *value > 0)
            .map(|value| format!("{:.1} mm", value as f64 / 10.0)),
    );
    push_integer(
        out,
        container_name,
        "SonyISO",
        entry.tag_id,
        "sony_tag2010",
        le_u16_at(&bytes, 0x0320).map(|value| sony_iso(value) as i64),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrParams",
        entry.tag_id,
        "sony_tag2010",
        int16_slice_string(&bytes, 0x17d0, 16),
    );
    push_string(
        out,
        container_name,
        "LensFormat",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x17f1).copied().map(lens_format),
    );
    push_string(
        out,
        container_name,
        "LensMount",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x17f2).copied().map(lens_mount),
    );
    push_string(
        out,
        container_name,
        "LensType2",
        entry.tag_id,
        "sony_tag2010",
        le_u16_at(&bytes, 0x17f3).map(lens_type2_name),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrParamsPresent",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x17f8).copied().map(yes_no_byte),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrParamsNumber",
        entry.tag_id,
        "sony_tag2010",
        bytes
            .get(0x17f9)
            .copied()
            .map(distortion_corr_params_number),
    );
    push_string(
        out,
        container_name,
        "AspectRatio",
        entry.tag_id,
        "sony_tag2010",
        bytes.get(0x188c).copied().map(aspect_ratio),
    );
}

fn decode_tag_9416(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    cursor: &Cursor<'_>,
    entries: &[MakerEntry],
) {
    let Some(entry) = find_entry(entries, 0x9416) else {
        return;
    };
    let Some(bytes) = deciphered_value_bytes(cursor, entry) else {
        return;
    };
    push_integer(
        out,
        container_name,
        "SonyISO",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x0004).map(|value| sony_iso(value) as i64),
    );
    push_float(
        out,
        container_name,
        "StopsAboveBaseISO",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x0006).map(stops_above_base_iso),
    );
    push_string(
        out,
        container_name,
        "SonyExposureTime2",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x000a).map(|value| sony_exposure_time(value as f64)),
    );
    push_string(
        out,
        container_name,
        "SonyMaxApertureValue",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x0012).map(sony_f_number),
    );
    push_integer(
        out,
        container_name,
        "SequenceImageNumber",
        entry.tag_id,
        "sony_tag9416",
        le_u32_at(&bytes, 0x001d).map(|value| value as i64 + 1),
    );
    push_string(
        out,
        container_name,
        "ReleaseMode2",
        entry.tag_id,
        "sony_tag9416",
        bytes.get(0x002b).copied().map(release_mode2),
    );
    push_string(
        out,
        container_name,
        "ExposureProgram",
        entry.tag_id,
        "sony_tag9416",
        bytes.get(0x0035).copied().map(exposure_program3),
    );
    push_string(
        out,
        container_name,
        "CreativeStyle",
        entry.tag_id,
        "sony_tag9416",
        bytes.get(0x0037).copied().map(creative_style_code),
    );
    push_string(
        out,
        container_name,
        "LensMount",
        entry.tag_id,
        "sony_tag9416",
        bytes.get(0x0048).copied().map(lens_mount),
    );
    push_string(
        out,
        container_name,
        "LensFormat",
        entry.tag_id,
        "sony_tag9416",
        bytes.get(0x0049).copied().map(lens_format),
    );
    push_string(
        out,
        container_name,
        "LensType2",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x004b).map(lens_type2_name),
    );
    push_string(
        out,
        container_name,
        "DistortionCorrParams",
        entry.tag_id,
        "sony_tag9416",
        int16_slice_string(&bytes, 0x004f, 16),
    );
    push_string(
        out,
        container_name,
        "PictureProfile",
        entry.tag_id,
        "sony_tag9416",
        bytes.get(0x0070).copied().map(picture_profile),
    );
    push_string(
        out,
        container_name,
        "FocalLength",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x0071).map(|value| format!("{:.1} mm", value as f64 / 10.0)),
    );
    push_string(
        out,
        container_name,
        "MinFocalLength",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x0073).map(|value| format!("{:.1} mm", value as f64 / 10.0)),
    );
    push_string(
        out,
        container_name,
        "MaxFocalLength",
        entry.tag_id,
        "sony_tag9416",
        le_u16_at(&bytes, 0x0075).map(|value| format!("{:.1} mm", value as f64 / 10.0)),
    );
    push_string(
        out,
        container_name,
        "VignettingCorrParams",
        entry.tag_id,
        "sony_tag9416",
        int16_slice_string(&bytes, 0x0891, 16).or_else(|| int16_slice_string(&bytes, 0x088f, 16)),
    );
    push_string(
        out,
        container_name,
        "ChromaticAberrationCorrParams",
        entry.tag_id,
        "sony_tag9416",
        int16_slice_string(&bytes, 0x083b, 32),
    );
}

fn find_entry(entries: &[MakerEntry], tag_id: u16) -> Option<&MakerEntry> {
    entries.iter().find(|entry| entry.tag_id == tag_id)
}

fn raw_value_bytes(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<Vec<u8>> {
    let byte_len = maker_type_size(entry.type_id).checked_mul(entry.count as usize)?;
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

fn deciphered_value_bytes(cursor: &Cursor<'_>, entry: &MakerEntry) -> Option<Vec<u8>> {
    let raw = cursor
        .slice(entry.value_or_offset as usize, entry.count as usize)
        .ok()?;
    Some(
        raw.iter()
            .map(|byte| SONY_DECIPHER_TABLE[*byte as usize])
            .collect(),
    )
}

fn maker_type_size(type_id: u16) -> usize {
    match type_id {
        1 | 2 | 7 => 1,
        3 | 8 => 2,
        4 | 9 => 4,
        5 | 10 => 8,
        _ => 1,
    }
}

fn read_u8(cursor: &Cursor<'_>, entry: &MakerEntry) -> Option<u8> {
    raw_value_bytes(cursor, Endian::Little, entry)?
        .first()
        .copied()
}

fn read_u8_vec(cursor: &Cursor<'_>, entry: &MakerEntry) -> Option<Vec<u8>> {
    raw_value_bytes(cursor, Endian::Little, entry)
}

fn read_u16(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<u16> {
    match entry.type_id {
        3 => {
            let bytes = raw_value_bytes(cursor, endian, entry)?;
            Some(match endian {
                Endian::Little => u16::from_le_bytes(bytes[0..2].try_into().ok()?),
                Endian::Big => u16::from_be_bytes(bytes[0..2].try_into().ok()?),
            })
        }
        _ => None,
    }
}

fn read_i16(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<i16> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    Some(match endian {
        Endian::Little => i16::from_le_bytes(bytes[0..2].try_into().ok()?),
        Endian::Big => i16::from_be_bytes(bytes[0..2].try_into().ok()?),
    })
}

fn read_u32(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<u32> {
    match entry.type_id {
        4 => {
            let bytes = raw_value_bytes(cursor, endian, entry)?;
            Some(match endian {
                Endian::Little => u32::from_le_bytes(bytes[0..4].try_into().ok()?),
                Endian::Big => u32::from_be_bytes(bytes[0..4].try_into().ok()?),
            })
        }
        _ => None,
    }
}

fn read_i32(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<i32> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    Some(match endian {
        Endian::Little => i32::from_le_bytes(bytes[0..4].try_into().ok()?),
        Endian::Big => i32::from_be_bytes(bytes[0..4].try_into().ok()?),
    })
}

fn read_u32_pair(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<(u32, u32)> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    Some((
        u32::from_le_bytes(bytes[0..4].try_into().ok()?),
        u32::from_le_bytes(bytes[4..8].try_into().ok()?),
    ))
}

fn read_i32_pair(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<(i32, i32)> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    Some((
        i32::from_le_bytes(bytes[0..4].try_into().ok()?),
        i32::from_le_bytes(bytes[4..8].try_into().ok()?),
    ))
}

fn read_u16_pair(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<(u16, u16)> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    Some((
        u16::from_le_bytes(bytes[0..2].try_into().ok()?),
        u16::from_le_bytes(bytes[2..4].try_into().ok()?),
    ))
}

fn read_u16_quad(
    cursor: &Cursor<'_>,
    endian: Endian,
    entry: &MakerEntry,
) -> Option<(u16, u16, u16, u16)> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    Some((
        u16::from_le_bytes(bytes[0..2].try_into().ok()?),
        u16::from_le_bytes(bytes[2..4].try_into().ok()?),
        u16::from_le_bytes(bytes[4..6].try_into().ok()?),
        u16::from_le_bytes(bytes[6..8].try_into().ok()?),
    ))
}

fn read_ascii(cursor: &Cursor<'_>, endian: Endian, entry: &MakerEntry) -> Option<String> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    Some(trim_c_string_bytes(&bytes))
}

fn read_rational_string(
    cursor: &Cursor<'_>,
    endian: Endian,
    entry: &MakerEntry,
    divisor: f64,
) -> Option<String> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    let numerator = i32::from_le_bytes(bytes[0..4].try_into().ok()?) as f64;
    let denominator = i32::from_le_bytes(bytes[4..8].try_into().ok()?) as f64;
    if denominator == 0.0 {
        return None;
    }
    let value = numerator / denominator;
    Some(format_number(value / (1.0 / divisor)))
}

fn le_u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn le_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn le_i16_at(bytes: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn rational32u_at(bytes: &[u8], offset: usize) -> Option<f64> {
    let numerator = le_u16_at(bytes, offset)? as f64;
    let denominator = le_u16_at(bytes, offset + 2)? as f64;
    if denominator == 0.0 {
        return None;
    }
    Some(numerator / denominator)
}

fn int16_triplet_at(bytes: &[u8], offset: usize) -> Option<(u16, u16, u16)> {
    Some((
        le_u16_at(bytes, offset)?,
        le_u16_at(bytes, offset + 2)?,
        le_u16_at(bytes, offset + 4)?,
    ))
}

fn int16_slice_string(bytes: &[u8], offset: usize, count: usize) -> Option<String> {
    let mut values = Vec::new();
    for index in 0..count {
        let start = offset + index * 2;
        let value = i16::from_le_bytes(bytes.get(start..start + 2)?.try_into().ok()?);
        values.push(value.to_string());
    }
    Some(values.join(" "))
}

fn u16_slice_string(bytes: &[u8], offset: usize, count: usize) -> Option<String> {
    let mut values = Vec::new();
    for index in 0..count {
        let start = offset + index * 2;
        values.push(le_u16_at(bytes, start)?.to_string());
    }
    Some(values.join(" "))
}

fn trim_c_string_bytes(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn provenance(container_name: &str, path: &str) -> Provenance {
    Provenance {
        container: container_name.into(),
        namespace: "sony".into(),
        path: Some(path.into()),
        offset_start: None,
        offset_end: None,
        notes: Vec::new(),
    }
}

fn push_string(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    tag_id: u16,
    path: &str,
    value: Option<String>,
) {
    let Some(value) = value else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "sony".into(),
        tag_id: format!("0x{:04X}", tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: provenance(container_name, path),
        notes: Vec::new(),
    });
}

fn push_timestamp(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    tag_id: u16,
    path: &str,
    value: Option<String>,
) {
    let Some(value) = value else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "sony".into(),
        tag_id: format!("0x{:04X}", tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::Timestamp(value),
        provenance: provenance(container_name, path),
        notes: Vec::new(),
    });
}

fn push_integer(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    tag_id: u16,
    path: &str,
    value: Option<i64>,
) {
    let Some(value) = value else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "sony".into(),
        tag_id: format!("0x{:04X}", tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(value),
        provenance: provenance(container_name, path),
        notes: Vec::new(),
    });
}

fn push_float(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    tag_id: u16,
    path: &str,
    value: Option<f64>,
) {
    let Some(value) = value else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "sony".into(),
        tag_id: format!("0x{:04X}", tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::Float(value),
        provenance: provenance(container_name, path),
        notes: Vec::new(),
    });
}

fn long_exposure_nr(value: u32) -> String {
    match value {
        0 => "Off".into(),
        1 => "On (unused)".into(),
        0x10001 => "On (dark subtracted)".into(),
        _ => value.to_string(),
    }
}

fn high_iso_nr(value: u16) -> String {
    match value {
        0 => "Off".into(),
        1 => "Low".into(),
        2 => "Normal".into(),
        3 => "High".into(),
        256 => "Auto".into(),
        _ => value.to_string(),
    }
}

fn format_hdr(value: u32) -> String {
    let mode = match (value & 0xffff) as u16 {
        0 => "Off",
        1 => "Auto",
        0x10 => "1.0 EV",
        0x11 => "1.5 EV",
        0x12 => "2.0 EV",
        0x13 => "2.5 EV",
        0x14 => "3.0 EV",
        0x15 => "3.5 EV",
        0x16 => "4.0 EV",
        0x17 => "4.5 EV",
        0x18 => "5.0 EV",
        0x19 => "5.5 EV",
        0x1a => "6.0 EV",
        _ => "Unknown",
    };
    let image = match (value >> 16) as u16 {
        0 => "Uncorrected image",
        1 => "HDR image (good)",
        2 => "HDR image (fail 1)",
        3 => "HDR image (fail 2)",
        _ => "Unknown",
    };
    format!("{mode}; {image}")
}

fn scene_mode(value: u32) -> String {
    match value {
        16 => "Auto".into(),
        _ => value.to_string(),
    }
}

fn zone_matching(value: u32) -> String {
    match value {
        0 => "ISO Setting Used".into(),
        _ => value.to_string(),
    }
}

fn dynamic_range_optimizer(value: u32) -> String {
    match value {
        0 => "Off".into(),
        1 => "Standard".into(),
        2 => "Advanced Auto".into(),
        3 => "Auto".into(),
        _ => value.to_string(),
    }
}

fn dynamic_range_optimizer_short(value: u16) -> String {
    dynamic_range_optimizer(value as u32)
}

fn boolean_off_on(value: u32) -> String {
    match value {
        0 => "Off".into(),
        1 => "On".into(),
        _ => value.to_string(),
    }
}

fn boolean_off_on_short(value: u16) -> String {
    boolean_off_on(value as u32)
}

fn color_mode(value: u32) -> String {
    match value {
        0 => "Standard".into(),
        _ => value.to_string(),
    }
}

fn format_file_format(bytes: Vec<u8>) -> String {
    if bytes.len() == 4 {
        format!("ARW {}.{}.{}", bytes[3] + 2, bytes[0], bytes[2])
    } else {
        hex_bytes(&bytes)
    }
}

fn quality(value: u32) -> String {
    match value {
        0 => "RAW".into(),
        1 => "Super Fine".into(),
        2 => "Fine".into(),
        3 => "Standard".into(),
        4 => "Economy".into(),
        5 => "Extra Fine".into(),
        6 => "RAW + JPEG/HEIF".into(),
        7 => "Compressed RAW".into(),
        8 => "Compressed RAW + JPEG".into(),
        9 => "Light".into(),
        _ => value.to_string(),
    }
}

fn white_balance(value: u32) -> String {
    match value {
        0 => "Auto".into(),
        1 => "Color Temperature/Color Filter".into(),
        0x10 => "Daylight".into(),
        0x20 => "Cloudy".into(),
        0x30 => "Shade".into(),
        0x40 => "Tungsten".into(),
        0x50 => "Flash".into(),
        0x60 => "Fluorescent".into(),
        0x70 => "Custom".into(),
        0x80 => "Underwater".into(),
        _ => value.to_string(),
    }
}

fn multi_frame_nr(value: u32) -> String {
    match value {
        0 => "Off".into(),
        1 => "On".into(),
        _ => value.to_string(),
    }
}

fn picture_effect(value: u16) -> String {
    match value {
        0 => "Off".into(),
        _ => value.to_string(),
    }
}

fn soft_skin_effect(value: u32) -> String {
    match value {
        0 => "Off".into(),
        1 => "Low".into(),
        2 => "Mid".into(),
        3 => "High".into(),
        _ => value.to_string(),
    }
}

fn correction_setting(value: u32) -> String {
    match value {
        0 => "Off".into(),
        2 => "Auto".into(),
        _ => value.to_string(),
    }
}

fn yes_no(value: u16) -> String {
    if value == 0 {
        "No".into()
    } else {
        "Yes".into()
    }
}

fn yes_no_byte(value: u8) -> String {
    if value == 0 {
        "No".into()
    } else {
        "Yes".into()
    }
}

fn flash_action(value: u32) -> String {
    match value {
        0 => "Did not fire".into(),
        1 => "Flash Fired".into(),
        2 => "External Flash Fired".into(),
        3 => "Wireless Controlled Flash Fired".into(),
        _ => value.to_string(),
    }
}

fn focus_mode(value: u8) -> String {
    match value {
        0 => "Manual".into(),
        2 => "AF-S".into(),
        3 => "AF-C".into(),
        4 => "AF-A".into(),
        6 => "DMF".into(),
        7 => "AF-D".into(),
        _ => value.to_string(),
    }
}

fn af_area_mode_setting(value: u8) -> String {
    match value {
        0 => "Wide".into(),
        1 => "Center".into(),
        3 => "Flexible Spot".into(),
        4 => "Flexible Spot (LA-EA4)".into(),
        8 => "Zone".into(),
        9 => "Center (LA-EA4)".into(),
        11 => "Zone".into(),
        12 => "Expanded Flexible Spot".into(),
        13 => "Custom AF Area".into(),
        _ => value.to_string(),
    }
}

fn af_tracking(value: u8) -> String {
    match value {
        0 => "Off".into(),
        1 => "On".into(),
        _ => value.to_string(),
    }
}

fn af_point_selected(value: u8) -> String {
    match value {
        0 => "n/a".into(),
        1 => "Center".into(),
        2 => "Top".into(),
        3 => "Upper-right".into(),
        4 => "Right".into(),
        5 => "Lower-right".into(),
        6 => "Bottom".into(),
        7 => "Lower-left".into(),
        8 => "Left".into(),
        9 => "Upper-left".into(),
        _ => value.to_string(),
    }
}

fn multi_frame_nr_effect(value: u32) -> String {
    match value {
        0 => "Normal".into(),
        _ => value.to_string(),
    }
}

fn priority_set_in_awb(value: u8) -> String {
    match value {
        0 => "Standard".into(),
        _ => value.to_string(),
    }
}

fn metering_mode2(value: u16) -> String {
    match value {
        256 => "Multi-segment".into(),
        _ => value.to_string(),
    }
}

fn raw_file_type(value: u16) -> String {
    match value {
        0 => "Compressed RAW".into(),
        _ => value.to_string(),
    }
}

fn exposure_mode(value: u16) -> String {
    match value {
        6 => "Auto".into(),
        _ => value.to_string(),
    }
}

fn flash_level(value: i16) -> String {
    match value {
        0 => "Normal".into(),
        _ => value.to_string(),
    }
}

fn release_mode(value: u16) -> String {
    match value {
        2 => "Continuous".into(),
        _ => value.to_string(),
    }
}

fn quality2_byte(value: u8) -> String {
    match value {
        0 => "JPEG".into(),
        1 => "RAW".into(),
        2 => "RAW + JPEG".into(),
        3 => "JPEG + MPO".into(),
        4 => "HEIF".into(),
        6 => "RAW + HEIF".into(),
        _ => value.to_string(),
    }
}

fn iso_setting(value: u8) -> String {
    match value {
        0 => "Auto".into(),
        _ => value.to_string(),
    }
}

fn af_area_mode(value: u8) -> String {
    match value {
        0 => "Multi".into(),
        1 => "Center".into(),
        2 => "Spot".into(),
        3 => "Flexible Spot".into(),
        10 => "Selective (for Miniature effect)".into(),
        11 => "Zone".into(),
        12 => "Expanded Flexible Spot".into(),
        13 => "Custom AF Area".into(),
        14 => "Tracking".into(),
        15 => "Face Tracking".into(),
        20 => "Animal Eye Tracking".into(),
        21 => "Human Eye Tracking".into(),
        255 => "Manual".into(),
        _ => value.to_string(),
    }
}

fn distortion_correction(value: u8) -> String {
    match value {
        0 => "None".into(),
        1 => "Applied".into(),
        _ => value.to_string(),
    }
}

fn distortion_corr_params_number(value: u8) -> String {
    match value {
        11 => "11 (APS-C)".into(),
        16 => "16 (Full-frame)".into(),
        _ => value.to_string(),
    }
}

fn lens_mount2(value: u8) -> String {
    match value {
        0 => "Unknown".into(),
        1 => "A-mount (1)".into(),
        4 => "E-mount".into(),
        5 => "A-mount (5)".into(),
        _ => value.to_string(),
    }
}

fn lens_mount(value: u8) -> String {
    match value {
        0 => "Unknown".into(),
        1 => "A-mount".into(),
        2 => "E-mount".into(),
        3 => "A-mount (3)".into(),
        _ => value.to_string(),
    }
}

fn lens_format(value: u8) -> String {
    match value {
        0 => "Unknown".into(),
        1 => "APS-C".into(),
        2 => "Full-frame".into(),
        _ => value.to_string(),
    }
}

fn lens_type2_name(value: u16) -> String {
    match value {
        32793 => "Sony E PZ 16-50mm F3.5-5.6 OSS".into(),
        _ => value.to_string(),
    }
}

fn lens_type_name(value: u16) -> String {
    match value {
        0xffff => "E-Mount, T-Mount, Other Lens or no lens".into(),
        _ => value.to_string(),
    }
}

fn version_u16(value: u16) -> String {
    format!("{:x}.{:02x}", value >> 8, value & 0xff)
}

fn lens_firmware_version(value: u16) -> String {
    format!("Ver.{:02x}.{:03}", value >> 8, value & 0xff)
}

fn flash_status(value: u8) -> String {
    match value {
        0 => "No Flash present".into(),
        2 => "Flash Inhibited".into(),
        64 => "Built-in Flash present".into(),
        65 => "Built-in Flash Fired".into(),
        66 => "Built-in Flash Inhibited".into(),
        128 => "External Flash present".into(),
        129 => "External Flash Fired".into(),
        _ => value.to_string(),
    }
}

fn flash_mode(value: u8) -> String {
    match value {
        0 => "Autoflash".into(),
        1 => "Fill-flash".into(),
        2 => "Flash Off".into(),
        3 => "Slow Sync".into(),
        4 => "Rear Sync".into(),
        6 => "Wireless".into(),
        _ => value.to_string(),
    }
}

fn self_timer(value: u8) -> String {
    match value {
        0 => "Off".into(),
        1 => "Self-timer 5 or 10 s".into(),
        2 => "Self-timer 2 s".into(),
        _ => value.to_string(),
    }
}

fn camera_orientation(value: u8) -> String {
    match value {
        1 => "Horizontal (normal)".into(),
        3 => "Rotate 180".into(),
        6 => "Rotate 90 CW".into(),
        8 => "Rotate 270 CW".into(),
        _ => value.to_string(),
    }
}

fn model_release_year(value: u8) -> i64 {
    2000 + value as i64
}

fn sequence_length(value: u8) -> String {
    match value {
        0 => "Continuous".into(),
        1 => "1 shot".into(),
        2 => "2 shots".into(),
        3 => "3 shots".into(),
        4 => "4 shots".into(),
        5 => "5 shots".into(),
        6 => "6 shots".into(),
        7 => "7 shots".into(),
        9 => "9 shots".into(),
        10 => "10 shots".into(),
        12 => "12 shots".into(),
        16 => "16 shots".into(),
        100 => "Continuous - iSweep Panorama".into(),
        200 => "Continuous - Sweep Panorama".into(),
        _ => value.to_string(),
    }
}

fn brightness_value_2010(value: u16) -> f64 {
    value as f64 / 256.0 - 56.6
}

fn hdr_setting(value: u8) -> String {
    match value {
        0 => "Off".into(),
        1 => "HDR Auto".into(),
        3 => "HDR 1 EV".into(),
        5 => "HDR 2 EV".into(),
        7 => "HDR 3 EV".into(),
        9 => "HDR 4 EV".into(),
        11 => "HDR 5 EV".into(),
        13 => "HDR 6 EV".into(),
        _ => value.to_string(),
    }
}

fn exposure_compensation_2010(value: i16) -> f64 {
    -(value as f64) / 256.0
}

fn picture_effect2(value: u8) -> String {
    match value {
        0 => "Off".into(),
        1 => "Toy Camera".into(),
        2 => "Pop Color".into(),
        3 => "Posterization".into(),
        4 => "Retro Photo".into(),
        5 => "Soft High Key".into(),
        6 => "Partial Color".into(),
        7 => "High Contrast Monochrome".into(),
        8 => "Soft Focus".into(),
        9 => "HDR Painting".into(),
        10 => "Rich-tone Monochrome".into(),
        11 => "Miniature".into(),
        12 => "Water Color".into(),
        13 => "Illustration".into(),
        _ => value.to_string(),
    }
}

fn metering_mode_2010(value: u8) -> String {
    match value {
        0 => "Multi-segment".into(),
        2 => "Center-weighted average".into(),
        3 => "Spot".into(),
        4 => "Average".into(),
        5 => "Highlight".into(),
        _ => value.to_string(),
    }
}

fn aspect_ratio(value: u8) -> String {
    match value {
        0 => "16:9".into(),
        1 => "4:3".into(),
        2 => "3:2".into(),
        3 => "1:1".into(),
        5 => "Panorama".into(),
        _ => value.to_string(),
    }
}

fn anti_blur(value: u16) -> String {
    match value {
        0 => "Off".into(),
        1 => "On (Continuous)".into(),
        2 => "On (Shooting)".into(),
        _ => value.to_string(),
    }
}

fn variable_low_pass_filter((first, second): (u16, u16)) -> String {
    if first == 0 && second == 0 {
        "n/a".into()
    } else {
        format!("{first} {second}")
    }
}

fn pixel_shift_info(bytes: Vec<u8>) -> String {
    if bytes.iter().all(|byte| *byte == 0) {
        "n/a".into()
    } else {
        hex_bytes(&bytes)
    }
}

fn sony_iso(value: u16) -> u32 {
    (100.0 * 2f64.powf(16.0 - value as f64 / 256.0)).round() as u32
}

fn stops_above_base_iso(value: u16) -> f64 {
    16.0 - value as f64 / 256.0
}

fn sony_exposure_time(value: f64) -> String {
    let seconds = if value == 0.0 {
        0.0
    } else {
        2f64.powf(16.0 - value / 256.0)
    };
    format_exposure_time(seconds)
}

fn format_exposure_time(seconds: f64) -> String {
    if seconds == 0.0 {
        return "Bulb".into();
    }
    if seconds < 1.0 {
        let denominator = (1.0 / seconds).round();
        return format!("1/{:.0}", denominator);
    }
    format_number(seconds)
}

fn sony_f_number(value: u16) -> String {
    format!("{:.1}", 2f64.powf((value as f64 / 256.0 - 16.0) / 2.0))
}

fn release_mode2(value: u8) -> String {
    match value {
        0 => "Normal".into(),
        1 => "Continuous".into(),
        2 => "Continuous - Exposure Bracketing".into(),
        3 => "DRO or White Balance Bracketing".into(),
        5 => "Continuous - Burst".into(),
        6 => "Single Frame - Capture During Movie".into(),
        7 => "Continuous - Sweep Panorama".into(),
        8 => "Continuous - Anti-Motion Blur, Hand-held Twilight".into(),
        9 => "Continuous - HDR".into(),
        10 => "Continuous - Background defocus".into(),
        13 => "Continuous - 3D Sweep Panorama".into(),
        15 => "Continuous - High Resolution Sweep Panorama".into(),
        16 => "Continuous - 3D Image".into(),
        17 => "Continuous - Burst 2".into(),
        18 => "Normal - iAuto+".into(),
        19 => "Continuous - Speed/Advance Priority".into(),
        20 => "Continuous - Multi Frame NR".into(),
        23 => "Single-frame - Exposure Bracketing".into(),
        26 => "Continuous Low".into(),
        27 => "Continuous - High Sensitivity".into(),
        28 => "Smile Shutter".into(),
        146 => "Single Frame - Movie Capture".into(),
        _ => value.to_string(),
    }
}

fn exposure_program3(value: u8) -> String {
    match value {
        0 => "Program AE".into(),
        1 => "Portrait".into(),
        2 => "Beach".into(),
        3 => "Sports".into(),
        4 => "Snow".into(),
        5 => "Landscape".into(),
        6 => "Auto".into(),
        7 => "Aperture-priority AE".into(),
        8 => "Shutter speed priority AE".into(),
        9 => "Night Scene / Twilight".into(),
        10 => "Hi-Speed Shutter".into(),
        11 => "Twilight Portrait".into(),
        12 => "Soft Snap/Portrait".into(),
        13 => "Fireworks".into(),
        14 => "Smile Shutter".into(),
        15 => "Manual".into(),
        18 => "iAuto".into(),
        _ => value.to_string(),
    }
}

fn creative_style_code(value: u8) -> String {
    match value {
        0 => "Standard".into(),
        1 => "Vivid".into(),
        2 => "Neutral".into(),
        3 => "Portrait".into(),
        4 => "Landscape".into(),
        5 => "B&W".into(),
        6 => "Clear".into(),
        7 => "Deep".into(),
        8 => "Light".into(),
        9 => "Sunset".into(),
        10 => "Night View/Portrait".into(),
        11 => "Autumn Leaves".into(),
        13 => "Sepia".into(),
        15 => "FL".into(),
        16 => "VV2".into(),
        17 => "IN".into(),
        18 => "SH".into(),
        19 => "FL2".into(),
        20 => "FL3".into(),
        255 => "Off".into(),
        _ => value.to_string(),
    }
}

fn picture_profile(value: u8) -> String {
    match value {
        0 => "Gamma Still - Standard/Neutral (PP2)".into(),
        1 => "Gamma Still - Portrait".into(),
        3 => "Gamma Still - Night View/Portrait".into(),
        4 => "Gamma Still - B&W/Sepia".into(),
        5 => "Gamma Still - Clear".into(),
        6 => "Gamma Still - Deep".into(),
        7 => "Gamma Still - Light".into(),
        8 => "Gamma Still - Vivid".into(),
        9 => "Gamma Still - Real".into(),
        10 => "Gamma Movie (PP1)".into(),
        22 => "Gamma ITU709 (PP3 or PP4)".into(),
        24 => "Gamma Cine1 (PP5)".into(),
        25 => "Gamma Cine2 (PP6)".into(),
        26 => "Gamma Cine3".into(),
        27 => "Gamma Cine4".into(),
        28 => "Gamma S-Log2 (PP7)".into(),
        29 => "Gamma ITU709 (800%)".into(),
        31 => "Gamma S-Log3 (PP8 or PP9)".into(),
        33 => "Gamma HLG2 (PP10)".into(),
        34 => "Gamma HLG3".into(),
        36 => "Off".into(),
        37 => "FL".into(),
        38 => "VV2".into(),
        39 => "IN".into(),
        40 => "SH".into(),
        48 => "FL2".into(),
        49 => "FL3".into(),
        _ => value.to_string(),
    }
}

fn lens_spec(bytes: &[u8]) -> String {
    if bytes.len() != 8 {
        return hex_bytes(bytes);
    }
    let short_focal = format!("{:02x}{:02x}", bytes[1], bytes[2])
        .parse::<u16>()
        .unwrap_or(0);
    let long_focal = format!("{:02x}{:02x}", bytes[3], bytes[4])
        .parse::<u16>()
        .unwrap_or(0);
    let short_aperture = format!("{:02x}", bytes[5])
        .parse::<u16>()
        .map(|value| value as f64 / 10.0)
        .unwrap_or(0.0);
    let long_aperture = format!("{:02x}", bytes[6])
        .parse::<u16>()
        .map(|value| value as f64 / 10.0)
        .unwrap_or(0.0);
    let flags = u16::from_be_bytes([bytes[0], bytes[7]]);

    let mut parts: Vec<String> = lens_spec_feature_parts(flags)
        .iter()
        .filter(|part| part.prefix)
        .map(|part| part.name.clone())
        .collect();

    if short_focal > 0 && short_aperture > 0.0 {
        let focal = if long_focal > 0 && long_focal != short_focal {
            format!("{short_focal}-{long_focal}mm")
        } else {
            format!("{short_focal}mm")
        };
        let aperture =
            if long_aperture > 0.0 && (long_aperture - short_aperture).abs() > f64::EPSILON {
                format!("F{short_aperture:.1}-{long_aperture:.1}")
            } else {
                format!("F{short_aperture:.1}")
            };
        parts.push(format!("{focal} {aperture}"));
    }

    parts.extend(
        lens_spec_feature_parts(flags)
            .into_iter()
            .filter(|part| !part.prefix)
            .map(|part| part.name),
    );

    if parts.is_empty() {
        hex_bytes(bytes)
    } else {
        parts.join(" ")
    }
}

fn lens_spec_features(bytes: &[u8]) -> String {
    if bytes.len() < 2 {
        return hex_bytes(bytes);
    }
    let flags = u16::from_be_bytes([bytes[0], bytes[1]]);
    let parts: Vec<String> = lens_spec_feature_parts(flags)
        .into_iter()
        .map(|part| part.name)
        .collect();
    if parts.is_empty() {
        hex_bytes(bytes)
    } else {
        parts.join(" ")
    }
}

#[derive(Clone)]
struct LensFeaturePart {
    name: String,
    prefix: bool,
}

fn lens_spec_feature_parts(flags: u16) -> Vec<LensFeaturePart> {
    let mut parts = Vec::new();
    if flags & 0x0300 == 0x0100 {
        parts.push(LensFeaturePart {
            name: "DT".into(),
            prefix: true,
        });
    } else if flags & 0x0300 == 0x0200 {
        parts.push(LensFeaturePart {
            name: "FE".into(),
            prefix: true,
        });
    } else if flags & 0x0300 == 0x0300 {
        parts.push(LensFeaturePart {
            name: "E".into(),
            prefix: true,
        });
    }
    if flags & 0x4000 != 0 {
        parts.push(LensFeaturePart {
            name: "PZ".into(),
            prefix: true,
        });
    }
    if flags & 0x0020 != 0 {
        parts.push(LensFeaturePart {
            name: "STF".into(),
            prefix: true,
        });
    } else if flags & 0x0040 != 0 {
        parts.push(LensFeaturePart {
            name: "Reflex".into(),
            prefix: true,
        });
    } else if flags & 0x0060 != 0 {
        parts.push(LensFeaturePart {
            name: "Macro".into(),
            prefix: true,
        });
    } else if flags & 0x0080 != 0 {
        parts.push(LensFeaturePart {
            name: "Fisheye".into(),
            prefix: true,
        });
    }
    if flags & 0x0004 != 0 {
        parts.push(LensFeaturePart {
            name: "ZA".into(),
            prefix: false,
        });
    } else if flags & 0x0008 != 0 {
        parts.push(LensFeaturePart {
            name: "G".into(),
            prefix: false,
        });
    }
    if flags & 0x0001 != 0 {
        parts.push(LensFeaturePart {
            name: "SSM".into(),
            prefix: false,
        });
    } else if flags & 0x0002 != 0 {
        parts.push(LensFeaturePart {
            name: "SAM".into(),
            prefix: false,
        });
    }
    if flags & 0x8000 != 0 {
        parts.push(LensFeaturePart {
            name: "OSS".into(),
            prefix: false,
        });
    }
    if flags & 0x2000 != 0 {
        parts.push(LensFeaturePart {
            name: "LE".into(),
            prefix: false,
        });
    }
    if flags & 0x0800 != 0 {
        parts.push(LensFeaturePart {
            name: "II".into(),
            prefix: false,
        });
    }
    parts
}

fn format_number(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decipher_table_matches_known_sony_tag9402_prefix() {
        let raw = [0x51, 0x01, 0xff, 0x00];
        let deciphered: Vec<u8> = raw
            .into_iter()
            .map(|byte| SONY_DECIPHER_TABLE[byte as usize])
            .collect();
        assert_eq!(deciphered, vec![0x21, 0x01, 0xff, 0x00]);
    }
}
