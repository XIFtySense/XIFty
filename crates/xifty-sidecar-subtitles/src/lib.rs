//! Subtitle sidecar adapter — `.srt`, `.vtt`, `.ass` / `.ssa`.
//!
//! Subtitle sidecars are the most universal sidecar pattern in video — every
//! YouTube uploader, video editor, and podcast-with-video creator ships them
//! next to their `.mp4` / `.mov` files. This adapter discovers all subtitle
//! siblings of a primary video, parses headers/cues natively (no third-party
//! decoders, no shelling out), and surfaces a normalized field surface
//! (format, language, cue count, duration, first-cue preview, path) per
//! sidecar so consumers can answer "what languages does this video have
//! captions in?" from the envelope alone.
//!
//! Per-sidecar entries are emitted as flat `subtitles.<i>.<field>` tag names
//! (index-prefixed in discovery order) into the `subtitles` namespace,
//! mirroring the shape used by `xifty-sidecar-sony-nrt` since `TypedValue`
//! has no `Array` variant.
//!
//! Discovery convention:
//!
//! * `<basename>.srt` / `<basename>.vtt` / `<basename>.ass` / `<basename>.ssa`
//!   — bare match next to the primary video.
//! * `<basename>.<lang>.srt` etc — language-tagged, where `<lang>` is a
//!   conservative ISO 639 / BCP 47 token (2–3 ASCII letters, optionally
//!   followed by a `-REGION` of 2–3 alphanumerics). Anything else fails the
//!   filter and the file is treated as a bare match (no language inferred).
//!
//! Out of scope (per issue #131): graphical/binary subtitles (`.idx`/`.sub`,
//! PGS) and embedded subtitle tracks (MP4 `tx3g`, MKV) — those belong in a
//! separate adapter / meta crate.

use std::path::{Path, PathBuf};

use xifty_core::{Issue, MetadataEntry, Provenance, Severity, TypedValue};
use xifty_sidecar::{
    MergeContext, MergePolicy, SIDECAR_PARSE_ERROR, Sidecar, SidecarPayload, SidecarRef,
};

const NAMESPACE: &str = "subtitles";
const CONTAINER: &str = "sidecar";

/// Maximum length of the surfaced `first_cue_text` preview, in chars.
const FIRST_CUE_PREVIEW_CHARS: usize = 200;

/// Subtitle sidecar adapter — registered automatically by
/// [`xifty_sidecar::SidecarRegistry`] when the CLI/FFI sidecar feature is
/// enabled.
#[derive(Debug, Default)]
pub struct SubtitlesSidecar;

impl SubtitlesSidecar {
    pub const fn new() -> Self {
        Self
    }
}

impl Sidecar for SubtitlesSidecar {
    fn name(&self) -> &'static str {
        NAMESPACE
    }

    fn discover(&self, primary_path: Option<&Path>) -> Vec<SidecarRef> {
        let Some(primary) = primary_path else {
            return Vec::new();
        };
        discover_in_dir(primary)
    }

    fn parse(&self, sidecar: &SidecarRef, bytes: &[u8], _ctx: &MergeContext) -> SidecarPayload {
        parse_sidecar(sidecar, bytes)
    }

    fn priority(&self) -> u8 {
        100
    }

    fn merge_policy(&self) -> MergePolicy {
        // Subtitle sidecars carry strictly *additional* fields the video
        // container does not. There is no overlap with embedded metadata
        // today, but Complement is the safe default — if MP4 `tx3g` parsing
        // ever lands, the conflict-detector will surface mismatches.
        MergePolicy::Complement
    }
}

// ---------------------------------------------------------------------------
// Format
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Srt,
    Vtt,
    Ass,
}

impl Format {
    fn from_ext(ext: &str) -> Option<Self> {
        // Case-insensitive match — the caller has already lowercased.
        match ext {
            "srt" => Some(Format::Srt),
            "vtt" => Some(Format::Vtt),
            // .ass and .ssa share a parser — both are Aegisub / SubStation
            // Alpha format with the same `[Script Info] / [V4+ Styles] /
            // [Events]` section structure.
            "ass" | "ssa" => Some(Format::Ass),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Format::Srt => "srt",
            Format::Vtt => "vtt",
            Format::Ass => "ass",
        }
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// One discovered sibling: format + (optionally inferred) language + path.
#[derive(Debug, Clone)]
struct DiscoveredSidecar {
    format: Format,
    language: Option<String>,
    path: PathBuf,
}

/// Walk the parent directory of `primary` once, accept any file whose
/// extension is `srt`, `vtt`, `ass`, or `ssa` (case-insensitive) AND whose
/// stem either equals the primary stem (bare match) OR equals
/// `<primary_stem>.<lang_token>`. Returned sorted by `(format, language,
/// path)` for deterministic test output.
fn discover_in_dir(primary: &Path) -> Vec<SidecarRef> {
    let Some(dir) = primary.parent() else {
        return Vec::new();
    };
    let Some(primary_stem) = primary.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let read = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut hits: Vec<DiscoveredSidecar> = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Don't ever match the primary file itself.
        if path == primary {
            continue;
        }
        let Some((stem, ext)) = split_stem_ext(name) else {
            continue;
        };
        let ext_lower = ext.to_ascii_lowercase();
        let Some(format) = Format::from_ext(&ext_lower) else {
            continue;
        };

        // Bare match: stem == primary_stem (case-insensitive on ASCII).
        if stem.eq_ignore_ascii_case(primary_stem) {
            hits.push(DiscoveredSidecar {
                format,
                language: None,
                path,
            });
            continue;
        }

        // Language-tagged match: stem == "<primary_stem>.<lang>".
        // Compare prefix case-insensitively; require a `.` separator and a
        // valid BCP 47 token afterwards.
        if stem.len() <= primary_stem.len() + 1 {
            continue;
        }
        let (head, sep_and_lang) = stem.split_at(primary_stem.len());
        if !head.eq_ignore_ascii_case(primary_stem) {
            continue;
        }
        let bytes = sep_and_lang.as_bytes();
        if bytes.first() != Some(&b'.') {
            continue;
        }
        let lang_token = &sep_and_lang[1..];
        if let Some(language) = validate_language_token(lang_token) {
            hits.push(DiscoveredSidecar {
                format,
                language: Some(language),
                path,
            });
        }
    }

    // Stable order: format string asc, then language (None first), then path.
    hits.sort_by(|a, b| {
        a.format
            .as_str()
            .cmp(b.format.as_str())
            .then_with(|| a.language.cmp(&b.language))
            .then_with(|| a.path.cmp(&b.path))
    });

    hits.into_iter()
        .map(|hit| {
            let lang_label = hit.language.as_deref().unwrap_or("und");
            SidecarRef {
                path: hit.path,
                label: format!("{}/{}", hit.format.as_str(), lang_label),
            }
        })
        .collect()
}

/// Split a file name into `(stem, ext)` using the LAST `.` as the boundary.
/// `Path::file_stem` returns the stem with intermediate dots intact, which is
/// what we want — `clip.en` is a stem, `srt` is the extension. Returns
/// `None` for files with no extension or hidden dotfiles like `.DS_Store`.
fn split_stem_ext(name: &str) -> Option<(&str, &str)> {
    // Skip leading dot files (e.g. `.DS_Store`) — they have no stem.
    if name.starts_with('.') {
        return None;
    }
    let dot = name.rfind('.')?;
    if dot == 0 || dot + 1 == name.len() {
        return None;
    }
    Some((&name[..dot], &name[dot + 1..]))
}

/// Validate a candidate language token from a filename. Returns the
/// normalized lowercase form on success, `None` on rejection.
///
/// Policy (chosen consistently per plan-reviewer note 2): conservative
/// ISO 639 / BCP 47 — 2–3 ASCII letters, optionally followed by `-REGION`
/// where REGION is 2–3 alphanumeric characters. This rejects common false
/// positives like `final`, `fixed`, `forced`, `subbed` while accepting all
/// real-world language tags (`en`, `eng`, `en-US`, `pt-BR`, `zh-Hans`).
///
/// Examples accepted: `en`, `eng`, `en-us`, `EN-US`, `pt-BR`, `zh-Hant`.
/// Examples rejected: `final`, `fixed`, `forced`, `1`, `123`, `e`.
fn validate_language_token(token: &str) -> Option<String> {
    let (primary, region) = match token.split_once('-') {
        Some((p, r)) => (p, Some(r)),
        None => (token, None),
    };
    // Primary subtag: 2–3 ASCII letters.
    if !(2..=3).contains(&primary.len()) || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    if let Some(region) = region {
        // Region subtag: 2–3 ASCII alphanumerics. (BCP 47 allows 4-letter
        // script subtags too, but this stays narrow on purpose — false
        // positives matter more than missing exotic tags here.)
        if !(2..=4).contains(&region.len()) || !region.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
    }
    Some(token.to_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// Parser dispatch
// ---------------------------------------------------------------------------

/// Strip an optional UTF-8 BOM (`EF BB BF`). Applied uniformly across all
/// three formats per plan-reviewer note 1 — VTT files exported from browser
/// tools commonly carry a BOM, and SRT files saved by Notepad on Windows do
/// too.
fn strip_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(b"\xEF\xBB\xBF") {
        &bytes[3..]
    } else {
        bytes
    }
}

/// Re-derive `Format` from the sidecar's extension (lowercased). Defaults to
/// SRT on the (impossible) case where discovery somehow handed us a path with
/// no recognized extension — the parser will degrade gracefully.
fn format_from_path(path: &Path) -> Format {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
        .and_then(Format::from_ext)
        .unwrap_or(Format::Srt)
}

/// Re-extract the language label from `sidecar.label` ("<format>/<lang>").
/// Falls back to `und` so we always emit a value.
fn language_from_label(label: &str) -> &str {
    label.split_once('/').map(|(_, lang)| lang).unwrap_or("und")
}

fn parse_sidecar(sidecar: &SidecarRef, bytes: &[u8]) -> SidecarPayload {
    let bytes = strip_bom(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            // Lossy decode — still surface what we can; emit a warning so
            // callers know the file wasn't clean UTF-8.
            return SidecarPayload {
                entries: Vec::new(),
                issues: vec![Issue {
                    severity: Severity::Warning,
                    code: SIDECAR_PARSE_ERROR.into(),
                    message: format!(
                        "subtitle sidecar is not valid UTF-8: {}",
                        sidecar.path.display()
                    ),
                    offset: None,
                    context: Some(NAMESPACE.into()),
                }],
            };
        }
    };

    let format = format_from_path(&sidecar.path);
    let parsed = match format {
        Format::Srt => parse_srt(text),
        Format::Vtt => parse_vtt(text),
        Format::Ass => parse_ass(text),
    };

    let mut entries: Vec<MetadataEntry> = Vec::new();
    let mut issues: Vec<Issue> = Vec::new();

    if !parsed.parsed_ok {
        issues.push(Issue {
            severity: Severity::Warning,
            code: SIDECAR_PARSE_ERROR.into(),
            message: format!(
                "{} subtitle sidecar failed to parse cleanly: {}",
                format.as_str(),
                sidecar.path.display()
            ),
            offset: None,
            context: Some(NAMESPACE.into()),
        });
    }

    // Index 0 — adapter only ever emits one set of fields per sidecar; the
    // CLI registry runs `parse` once per discovered file so the multi-sidecar
    // case (clip.en.srt + clip.es.srt) becomes two separate `parse` calls
    // with their own provenance. The index is therefore always `0` from the
    // adapter's local perspective; downstream consumers distinguish the
    // groups by `provenance.path`.
    let i = 0usize;

    push_string(&mut entries, sidecar, i, "format", format.as_str().into());
    push_string(
        &mut entries,
        sidecar,
        i,
        "language",
        language_from_label(&sidecar.label).into(),
    );
    push_integer(&mut entries, sidecar, i, "cue_count", parsed.cue_count);
    if let Some(duration) = parsed.duration_seconds {
        push_float(&mut entries, sidecar, i, "duration_seconds", duration);
    }
    push_string(
        &mut entries,
        sidecar,
        i,
        "path",
        sidecar.path.to_string_lossy().into_owned(),
    );
    if let Some(text) = parsed.first_cue_text {
        push_string(&mut entries, sidecar, i, "first_cue_text", text);
    }
    if matches!(format, Format::Ass) {
        push_integer(&mut entries, sidecar, i, "style_count", parsed.style_count);
    }

    SidecarPayload { entries, issues }
}

#[derive(Debug, Default)]
struct ParsedSubtitle {
    cue_count: i64,
    style_count: i64,
    duration_seconds: Option<f64>,
    first_cue_text: Option<String>,
    parsed_ok: bool,
}

// ---------------------------------------------------------------------------
// SRT parser
// ---------------------------------------------------------------------------

/// SRT is line-based:
///
/// ```text
/// 1
/// 00:00:01,000 --> 00:00:04,000
/// First cue line one
/// First cue line two
///
/// 2
/// 00:00:05,000 --> 00:00:09,000
/// Second cue
/// ```
///
/// Robust to: missing cue numbers, `\r\n` line endings, blank trailing lines,
/// optional UTF-8 BOM (already stripped by the caller).
fn parse_srt(text: &str) -> ParsedSubtitle {
    let mut out = ParsedSubtitle::default();
    let mut last_end_seconds: Option<f64> = None;
    let mut found_any_timing = false;
    let mut collecting_first_cue = false;
    let mut first_cue_buf = String::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if let Some((_, end)) = parse_srt_timing(line) {
            out.cue_count += 1;
            last_end_seconds = Some(end);
            found_any_timing = true;
            collecting_first_cue = out.cue_count == 1;
            continue;
        }
        if collecting_first_cue {
            if line.is_empty() {
                collecting_first_cue = false;
                continue;
            }
            if !first_cue_buf.is_empty() {
                first_cue_buf.push('\n');
            }
            first_cue_buf.push_str(line);
            if first_cue_buf.chars().count() >= FIRST_CUE_PREVIEW_CHARS {
                collecting_first_cue = false;
            }
        }
    }

    out.duration_seconds = last_end_seconds;
    if !first_cue_buf.is_empty() {
        out.first_cue_text = Some(truncate_chars(&first_cue_buf, FIRST_CUE_PREVIEW_CHARS));
    }
    out.parsed_ok = found_any_timing;
    out
}

/// Parse `HH:MM:SS,mmm --> HH:MM:SS,mmm` (SRT comma) or
/// `HH:MM:SS.mmm --> HH:MM:SS.mmm` (VTT period). Returns
/// `(start_seconds, end_seconds)` on a clean match.
fn parse_srt_timing(line: &str) -> Option<(f64, f64)> {
    let arrow = line.find("-->")?;
    let lhs = line[..arrow].trim();
    let rhs = line[arrow + 3..].trim();
    // VTT cues may carry trailing settings after the end timestamp, e.g.
    // `00:00:05.000 --> 00:00:08.000 line:0% align:start`. Strip everything
    // after the first whitespace on the rhs.
    let rhs_ts = rhs.split_whitespace().next()?;
    let start = parse_subtitle_timestamp(lhs)?;
    let end = parse_subtitle_timestamp(rhs_ts)?;
    Some((start, end))
}

/// Parse a `HH:MM:SS,mmm` / `HH:MM:SS.mmm` / `MM:SS.mmm` / `H:MM:SS.cs`
/// timestamp into seconds. Accepts both `,` and `.` as the fractional
/// separator (SRT uses comma, VTT uses period). The fractional part is
/// digits-only — its length determines the divisor (3 digits => milliseconds,
/// 2 digits => centiseconds, etc).
fn parse_subtitle_timestamp(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let (whole, frac) = match raw.find(|c: char| c == ',' || c == '.') {
        Some(idx) => (&raw[..idx], Some(&raw[idx + 1..])),
        None => (raw, None),
    };
    let parts: Vec<&str> = whole.split(':').collect();
    let (h, m, s) = match parts.as_slice() {
        [h, m, s] => (
            h.parse::<u64>().ok()?,
            m.parse::<u64>().ok()?,
            s.parse::<u64>().ok()?,
        ),
        [m, s] => (0u64, m.parse::<u64>().ok()?, s.parse::<u64>().ok()?),
        _ => return None,
    };
    let mut seconds = (h * 3600 + m * 60 + s) as f64;
    if let Some(frac) = frac {
        if !frac.is_empty() && frac.bytes().all(|b| b.is_ascii_digit()) {
            let value: u64 = frac.parse().ok()?;
            let divisor = 10f64.powi(frac.len() as i32);
            seconds += value as f64 / divisor;
        }
    }
    Some(seconds)
}

// ---------------------------------------------------------------------------
// VTT parser
// ---------------------------------------------------------------------------

/// VTT shape:
///
/// ```text
/// WEBVTT
///
/// NOTE this is a comment
///
/// 00:00:01.000 --> 00:00:04.000
/// First cue
///
/// optional-cue-id
/// 00:00:05.000 --> 00:00:09.000 line:0%
/// Second cue
/// ```
///
/// Skips `NOTE` and `STYLE` blocks; ignores cue identifiers (any non-empty
/// non-timing line that immediately precedes a timing line).
fn parse_vtt(text: &str) -> ParsedSubtitle {
    let mut out = ParsedSubtitle::default();

    // The very first non-empty line MUST be `WEBVTT` (optionally followed by
    // whitespace + header text). Files without this header are malformed.
    let first_line = text.lines().next().map(|l| l.trim_end_matches('\r'));
    let header_ok = match first_line {
        Some(line) => {
            line == "WEBVTT" || line.starts_with("WEBVTT ") || line.starts_with("WEBVTT\t")
        }
        None => false,
    };
    if !header_ok {
        out.parsed_ok = false;
        return out;
    }

    let mut last_end_seconds: Option<f64> = None;
    let mut found_any_timing = false;
    let mut collecting_first_cue = false;
    let mut first_cue_buf = String::new();
    let mut in_skip_block = false; // inside NOTE / STYLE block

    let mut lines = text.lines().peekable();
    // Skip the WEBVTT header line.
    let _ = lines.next();

    while let Some(raw_line) = lines.next() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            in_skip_block = false;
            if collecting_first_cue {
                collecting_first_cue = false;
            }
            continue;
        }
        if in_skip_block {
            continue;
        }
        // NOTE/STYLE/REGION blocks run until the next blank line.
        if line == "NOTE"
            || line.starts_with("NOTE ")
            || line.starts_with("NOTE\t")
            || line == "STYLE"
            || line == "REGION"
        {
            in_skip_block = true;
            continue;
        }
        if let Some((_, end)) = parse_srt_timing(line) {
            out.cue_count += 1;
            last_end_seconds = Some(end);
            found_any_timing = true;
            collecting_first_cue = out.cue_count == 1;
            continue;
        }
        if collecting_first_cue {
            if !first_cue_buf.is_empty() {
                first_cue_buf.push('\n');
            }
            first_cue_buf.push_str(line);
            if first_cue_buf.chars().count() >= FIRST_CUE_PREVIEW_CHARS {
                collecting_first_cue = false;
            }
        }
        // Otherwise: treat as a cue identifier or stray line — ignore.
    }

    out.duration_seconds = last_end_seconds;
    if !first_cue_buf.is_empty() {
        out.first_cue_text = Some(truncate_chars(&first_cue_buf, FIRST_CUE_PREVIEW_CHARS));
    }
    out.parsed_ok = found_any_timing;
    out
}

// ---------------------------------------------------------------------------
// ASS / SSA parser
// ---------------------------------------------------------------------------

/// ASS shape:
///
/// ```text
/// [Script Info]
/// Title: Example
///
/// [V4+ Styles]
/// Format: Name, Fontname, ...
/// Style: Default,Arial,...
/// Style: Title,Arial,...
///
/// [Events]
/// Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
/// Dialogue: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,Hello world
/// Dialogue: 0,0:00:05.00,0:00:09.00,Default,,0,0,0,,Goodbye, world
/// ```
///
/// The `[Events]` `Format:` line is dynamic — author can reorder columns —
/// so we look up `Start`, `End`, and `Text` by name. The `Text` column is
/// always the LAST column per ASS spec (it's the only one that can contain
/// commas), which the spec mandates.
fn parse_ass(text: &str) -> ParsedSubtitle {
    let mut out = ParsedSubtitle::default();

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Section {
        Unknown,
        Styles,
        Events,
    }
    let mut section = Section::Unknown;
    let mut events_format: Option<Vec<String>> = None;
    let mut last_end_seconds: Option<f64> = None;
    let mut first_dialogue_text: Option<String> = None;
    let mut found_any_section = false;

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r').trim_start();
        if line.is_empty() {
            continue;
        }
        // Comments — ASS allows `;` and `!:` prefixes.
        if line.starts_with(';') || line.starts_with("!:") {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            found_any_section = true;
            let name = line[1..line.len() - 1].trim();
            section = if name.eq_ignore_ascii_case("V4+ Styles")
                || name.eq_ignore_ascii_case("V4 Styles")
            {
                Section::Styles
            } else if name.eq_ignore_ascii_case("Events") {
                Section::Events
            } else {
                Section::Unknown
            };
            continue;
        }
        match section {
            Section::Styles => {
                if line.starts_with("Style:") {
                    out.style_count += 1;
                }
            }
            Section::Events => {
                if let Some(rest) = line.strip_prefix("Format:") {
                    events_format = Some(
                        rest.split(',')
                            .map(|s| s.trim().to_string())
                            .collect::<Vec<_>>(),
                    );
                } else if let Some(rest) = line.strip_prefix("Dialogue:") {
                    out.cue_count += 1;
                    let format_columns = match events_format.as_ref() {
                        Some(c) => c,
                        None => continue,
                    };
                    // The Text column may contain commas; per ASS spec it is
                    // ALWAYS the last column. So split on `,` only up to
                    // (n_cols - 1) commas, treating the remainder as Text.
                    let parts = split_ass_event(rest.trim_start(), format_columns.len());
                    let mut start_value: Option<f64> = None;
                    let mut end_value: Option<f64> = None;
                    let mut text_value: Option<String> = None;
                    for (idx, name) in format_columns.iter().enumerate() {
                        let Some(field) = parts.get(idx) else {
                            continue;
                        };
                        match name.as_str() {
                            "Start" => start_value = parse_subtitle_timestamp(field),
                            "End" => end_value = parse_subtitle_timestamp(field),
                            "Text" => text_value = Some(field.clone()),
                            _ => {}
                        }
                    }
                    let _ = start_value;
                    if let Some(end) = end_value {
                        last_end_seconds = Some(end);
                    }
                    if first_dialogue_text.is_none() {
                        if let Some(text) = text_value {
                            first_dialogue_text = Some(text);
                        }
                    }
                }
            }
            Section::Unknown => {}
        }
    }

    out.duration_seconds = last_end_seconds;
    if let Some(text) = first_dialogue_text {
        out.first_cue_text = Some(truncate_chars(&text, FIRST_CUE_PREVIEW_CHARS));
    }
    out.parsed_ok = found_any_section && (out.cue_count > 0 || out.style_count > 0);
    out
}

/// Split an ASS `Dialogue:` payload into its columns. Per ASS spec the Text
/// column is the LAST column and may contain commas — so we split on the
/// first `(n_cols - 1)` commas, and treat everything after as a single
/// `Text` column.
fn split_ass_event(payload: &str, n_cols: usize) -> Vec<String> {
    if n_cols == 0 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::with_capacity(n_cols);
    let mut remaining = payload;
    for _ in 0..(n_cols - 1) {
        match remaining.find(',') {
            Some(idx) => {
                out.push(remaining[..idx].trim().to_string());
                remaining = &remaining[idx + 1..];
            }
            None => {
                out.push(remaining.trim().to_string());
                return out;
            }
        }
    }
    out.push(remaining.to_string());
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect()
}

fn provenance(sidecar: &SidecarRef) -> Provenance {
    Provenance {
        container: CONTAINER.into(),
        namespace: NAMESPACE.into(),
        path: Some(sidecar.path.to_string_lossy().into_owned()),
        offset_start: None,
        offset_end: None,
        notes: vec![format!("subtitles sidecar ({})", sidecar.label)],
    }
}

fn tag_name(i: usize, field: &str) -> String {
    format!("subtitles.{i}.{field}")
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    i: usize,
    field: &str,
    value: String,
) {
    let name = tag_name(i, field);
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: name.clone(),
        tag_name: name,
        value: TypedValue::String(value),
        provenance: provenance(sidecar),
        notes: Vec::new(),
    });
}

fn push_integer(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    i: usize,
    field: &str,
    value: i64,
) {
    let name = tag_name(i, field);
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: name.clone(),
        tag_name: name,
        value: TypedValue::Integer(value),
        provenance: provenance(sidecar),
        notes: Vec::new(),
    });
}

fn push_float(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    i: usize,
    field: &str,
    value: f64,
) {
    let name = tag_name(i, field);
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: name.clone(),
        tag_name: name,
        value: TypedValue::Float(value),
        provenance: provenance(sidecar),
        notes: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidecar_for(path: &str, label: &str) -> SidecarRef {
        SidecarRef {
            path: PathBuf::from(path),
            label: label.into(),
        }
    }

    // ----- timestamp parser -----

    #[test]
    fn parses_srt_timestamp_with_milliseconds() {
        let s = parse_subtitle_timestamp("01:02:03,500").unwrap();
        assert!((s - (3600.0 + 120.0 + 3.5)).abs() < 1e-6);
    }

    #[test]
    fn parses_vtt_timestamp_with_period_separator() {
        let s = parse_subtitle_timestamp("00:00:09.250").unwrap();
        assert!((s - 9.25).abs() < 1e-6);
    }

    #[test]
    fn parses_ass_centisecond_timestamp() {
        let s = parse_subtitle_timestamp("0:00:05.42").unwrap();
        assert!((s - 5.42).abs() < 1e-6);
    }

    // ----- language token validator -----

    #[test]
    fn validates_iso_639_two_letter() {
        assert_eq!(validate_language_token("en").as_deref(), Some("en"));
        assert_eq!(validate_language_token("EN").as_deref(), Some("en"));
        assert_eq!(validate_language_token("fr").as_deref(), Some("fr"));
    }

    #[test]
    fn validates_iso_639_three_letter() {
        assert_eq!(validate_language_token("eng").as_deref(), Some("eng"));
    }

    #[test]
    fn validates_with_region_subtag() {
        assert_eq!(validate_language_token("en-US").as_deref(), Some("en-us"));
        assert_eq!(validate_language_token("pt-BR").as_deref(), Some("pt-br"));
        assert_eq!(
            validate_language_token("zh-Hans").as_deref(),
            Some("zh-hans")
        );
    }

    #[test]
    fn rejects_common_false_positives() {
        assert!(validate_language_token("final").is_none());
        assert!(validate_language_token("fixed").is_none());
        assert!(validate_language_token("forced").is_none());
        assert!(validate_language_token("e").is_none());
        assert!(validate_language_token("123").is_none());
        assert!(validate_language_token("1").is_none());
    }

    // ----- SRT parsing -----

    const MIN_SRT: &str = "1\n00:00:01,000 --> 00:00:04,000\nHello world\n\n2\n00:00:05,000 --> 00:00:09,500\nGoodbye world\n";

    #[test]
    fn parses_srt_minimal_fixture() {
        let parsed = parse_srt(MIN_SRT);
        assert!(parsed.parsed_ok);
        assert_eq!(parsed.cue_count, 2);
        assert!((parsed.duration_seconds.unwrap() - 9.5).abs() < 1e-6);
        assert_eq!(parsed.first_cue_text.as_deref(), Some("Hello world"));
    }

    #[test]
    fn parses_srt_without_cue_numbers() {
        // Some authoring tools omit the leading numeric line.
        let s = "00:00:01,000 --> 00:00:02,000\nFoo\n\n00:00:03,000 --> 00:00:04,000\nBar\n";
        let parsed = parse_srt(s);
        assert!(parsed.parsed_ok);
        assert_eq!(parsed.cue_count, 2);
    }

    #[test]
    fn parses_srt_with_crlf_line_endings() {
        let s = "1\r\n00:00:01,000 --> 00:00:02,000\r\nFoo\r\n\r\n";
        let parsed = parse_srt(s);
        assert!(parsed.parsed_ok);
        assert_eq!(parsed.cue_count, 1);
        assert_eq!(parsed.first_cue_text.as_deref(), Some("Foo"));
    }

    #[test]
    fn malformed_srt_with_no_timing_lines_emits_parse_error() {
        let s = "this is not a subtitle\nfile at all\n";
        let parsed = parse_srt(s);
        assert!(!parsed.parsed_ok);
        assert_eq!(parsed.cue_count, 0);

        let payload = parse_sidecar(&sidecar_for("/tmp/bad.srt", "srt/und"), s.as_bytes());
        assert_eq!(payload.issues.len(), 1);
        assert_eq!(payload.issues[0].code, SIDECAR_PARSE_ERROR);
        assert_eq!(payload.issues[0].severity, Severity::Warning);
        // Even on a malformed file, format/language/path/cue_count still emit.
        assert!(
            payload
                .entries
                .iter()
                .any(|e| e.tag_name == "subtitles.0.format")
        );
    }

    #[test]
    fn srt_first_cue_truncates_at_200_chars() {
        let long = "x".repeat(500);
        let s = format!("1\n00:00:01,000 --> 00:00:02,000\n{long}\n");
        let parsed = parse_srt(&s);
        let preview = parsed.first_cue_text.unwrap();
        assert_eq!(preview.chars().count(), FIRST_CUE_PREVIEW_CHARS);
    }

    // ----- VTT parsing -----

    const MIN_VTT: &str = "WEBVTT\n\nNOTE this is a comment\n\n00:00:01.000 --> 00:00:04.000\nHello WebVTT\n\n00:00:05.000 --> 00:00:08.250 line:0%\nSecond line\n";

    #[test]
    fn parses_vtt_minimal_fixture() {
        let parsed = parse_vtt(MIN_VTT);
        assert!(parsed.parsed_ok);
        assert_eq!(parsed.cue_count, 2);
        assert!((parsed.duration_seconds.unwrap() - 8.25).abs() < 1e-6);
        assert_eq!(parsed.first_cue_text.as_deref(), Some("Hello WebVTT"));
    }

    #[test]
    fn vtt_without_webvtt_header_fails() {
        let s = "00:00:01.000 --> 00:00:02.000\nNo header\n";
        let parsed = parse_vtt(s);
        assert!(!parsed.parsed_ok);

        let payload = parse_sidecar(&sidecar_for("/tmp/bad.vtt", "vtt/und"), s.as_bytes());
        assert_eq!(payload.issues.len(), 1);
        assert_eq!(payload.issues[0].code, SIDECAR_PARSE_ERROR);
    }

    #[test]
    fn vtt_strips_utf8_bom() {
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice(MIN_VTT.as_bytes());
        let payload = parse_sidecar(&sidecar_for("/tmp/clip.vtt", "vtt/und"), &bytes);
        assert!(payload.issues.is_empty(), "{:?}", payload.issues);
        let cue_count = payload
            .entries
            .iter()
            .find(|e| e.tag_name == "subtitles.0.cue_count")
            .unwrap();
        assert_eq!(cue_count.value, TypedValue::Integer(2));
    }

    #[test]
    fn srt_strips_utf8_bom() {
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice(MIN_SRT.as_bytes());
        let payload = parse_sidecar(&sidecar_for("/tmp/clip.srt", "srt/und"), &bytes);
        assert!(payload.issues.is_empty(), "{:?}", payload.issues);
    }

    // ----- ASS parsing -----

    const MIN_ASS: &str = "[Script Info]\nTitle: Example\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize\nStyle: Default,Arial,20\nStyle: Title,Arial,40\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,Hello, world\nDialogue: 0,0:00:05.00,0:00:09.50,Default,,0,0,0,,Goodbye now\n";

    #[test]
    fn parses_ass_minimal_fixture() {
        let parsed = parse_ass(MIN_ASS);
        assert!(parsed.parsed_ok);
        assert_eq!(parsed.cue_count, 2);
        assert_eq!(parsed.style_count, 2);
        assert!((parsed.duration_seconds.unwrap() - 9.5).abs() < 1e-6);
        // First Dialogue's Text column carries a comma — must not be split.
        assert_eq!(parsed.first_cue_text.as_deref(), Some("Hello, world"));
    }

    #[test]
    fn ass_handles_reordered_format_columns() {
        // Author has placed Text earlier-than-spec — but per ASS spec it's
        // still the last column. Exercise a non-standard but seen-in-the-wild
        // ordering of the *other* columns to verify the column-name lookup.
        let s = "[Events]\nFormat: Start, End, Layer, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0:00:02.00,0:00:06.00,0,Default,,0,0,0,,Reordered cue\n";
        let parsed = parse_ass(s);
        assert_eq!(parsed.cue_count, 1);
        assert!((parsed.duration_seconds.unwrap() - 6.0).abs() < 1e-6);
        assert_eq!(parsed.first_cue_text.as_deref(), Some("Reordered cue"));
    }

    #[test]
    fn parse_sidecar_for_ass_emits_style_count() {
        let payload = parse_sidecar(&sidecar_for("/tmp/clip.ass", "ass/und"), MIN_ASS.as_bytes());
        assert!(payload.issues.is_empty(), "{:?}", payload.issues);
        let style = payload
            .entries
            .iter()
            .find(|e| e.tag_name == "subtitles.0.style_count")
            .unwrap();
        assert_eq!(style.value, TypedValue::Integer(2));
    }

    #[test]
    fn parse_sidecar_for_srt_does_not_emit_style_count() {
        let payload = parse_sidecar(&sidecar_for("/tmp/clip.srt", "srt/und"), MIN_SRT.as_bytes());
        assert!(
            !payload
                .entries
                .iter()
                .any(|e| e.tag_name == "subtitles.0.style_count"),
            "style_count is ASS-only"
        );
    }

    // ----- end-to-end entry shape -----

    #[test]
    fn srt_entries_carry_correct_provenance_and_tags() {
        let payload = parse_sidecar(
            &sidecar_for("/tmp/clip.en.srt", "srt/en"),
            MIN_SRT.as_bytes(),
        );
        assert!(payload.issues.is_empty(), "{:?}", payload.issues);
        let expected = [
            "subtitles.0.format",
            "subtitles.0.language",
            "subtitles.0.cue_count",
            "subtitles.0.duration_seconds",
            "subtitles.0.path",
            "subtitles.0.first_cue_text",
        ];
        for tag_name in expected {
            assert!(
                payload.entries.iter().any(|e| e.tag_name == tag_name),
                "expected entry {tag_name}"
            );
        }
        let format = payload
            .entries
            .iter()
            .find(|e| e.tag_name == "subtitles.0.format")
            .unwrap();
        assert_eq!(format.namespace, "subtitles");
        assert_eq!(format.provenance.namespace, "subtitles");
        assert_eq!(format.provenance.container, "sidecar");
        assert_eq!(format.value, TypedValue::String("srt".into()));
        let language = payload
            .entries
            .iter()
            .find(|e| e.tag_name == "subtitles.0.language")
            .unwrap();
        assert_eq!(language.value, TypedValue::String("en".into()));
    }

    // ----- discovery -----

    #[test]
    fn discover_buffer_only_returns_empty() {
        let s = SubtitlesSidecar::new();
        assert!(s.discover(None).is_empty());
    }

    #[test]
    fn discover_finds_bare_and_language_tagged_siblings() {
        let dir = std::env::temp_dir().join("xifty-sidecar-subtitles-discover-multi");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("clip.mp4");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("clip.srt"), b"x").unwrap();
        std::fs::write(dir.join("clip.en.srt"), b"x").unwrap();
        std::fs::write(dir.join("clip.es.vtt"), b"x").unwrap();
        std::fs::write(dir.join("clip.ass"), b"x").unwrap();
        // Decoys.
        std::fs::write(dir.join("clip.final.srt"), b"x").unwrap(); // not a valid lang tag
        std::fs::write(dir.join("other.srt"), b"x").unwrap(); // unrelated stem
        std::fs::write(dir.join("clip.txt"), b"x").unwrap(); // unrelated ext

        let s = SubtitlesSidecar::new();
        let hits = s.discover(Some(&primary));
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        // Sorted by (format, language). Bare matches list as `und`.
        assert_eq!(
            labels,
            vec!["ass/und", "srt/und", "srt/en", "vtt/es"],
            "got {labels:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_case_insensitive_extension() {
        let dir = std::env::temp_dir().join("xifty-sidecar-subtitles-discover-case");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("CLIP.MOV");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("CLIP.SRT"), b"x").unwrap();
        std::fs::write(dir.join("CLIP.EN.VTT"), b"x").unwrap();

        let s = SubtitlesSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_rejects_bogus_language_tokens() {
        let dir = std::env::temp_dir().join("xifty-sidecar-subtitles-discover-bogus");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("clip.mp4");
        std::fs::write(&primary, b"x").unwrap();
        // `final` is alphabetic but 5 chars — fails the 2-3 letter rule.
        std::fs::write(dir.join("clip.final.srt"), b"x").unwrap();
        // `forced` similar.
        std::fs::write(dir.join("clip.forced.vtt"), b"x").unwrap();
        // `en` is valid.
        std::fs::write(dir.join("clip.en.srt"), b"x").unwrap();

        let s = SubtitlesSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "srt/en");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
