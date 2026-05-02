//! Sony video metadata interpreter (`namespace = "sony_video"`).
//!
//! Decodes Sony's `PROF` and `USMT` UUID atoms found in MP4/MOV files
//! produced by Sony pro/prosumer video cameras (FX-series, A7-series video,
//! A1). The Sony "User Data Atom UUID family" uses a 16-byte usertype where
//! bytes 0..4 are the atom name in ASCII (PROF, USMT, PRPF, ...) and bytes
//! 4..16 are the constant tail
//! `21 d2 4f ce bb 88 69 5c fa c9 c7 40`.
//!
//! Reverse-engineered from ExifTool's `lib/Image/ExifTool/QuickTime.pm`
//! (`UUID-PROF`/`UUID-USMT` dispatch, `%Profile`, `%FileProf`, `%AudioProf`,
//! `%VideoProf`, `%UserMedia`, `%MetaData`, and `ProcessMetaData`). ExifTool
//! is reference material only — read to understand, port to Rust, ship Rust.
//! Nothing from ExifTool ships in the artifact.
//!
//! ## Phase 1 bounded ship list
//!
//! **PROF** — sub-boxes dispatched: `FPRF`, `APRF`, `VPRF`. See `prof.rs`
//! module-level docs for per-tag citations.
//!
//! **USMT** — only `MTDT` sub-boxes dispatched (per `%QuickTime::UserMedia`
//! at `QuickTime.pm:2589-2597`). MTDT records emit the well-attested subset
//! `Title / ProductionDate / Software / Product / TrackProperty / TimeZone /
//! ModifyDate` per `%QuickTime::MetaData` (`QuickTime.pm:2599-2658`).
//!
//! `PRPF` and `AOLY` (and other Sony-family atoms) are recognised by the
//! container parser as `kind = "sony-video-atom"` but are deferred for
//! Phase 2 — the dispatcher silently no-ops on unknown tags so future
//! decoders can be added without touching the container layer.

mod prof;
mod usmt;

pub use prof::decode_prof;
pub use usmt::decode_usmt;

/// FourCC names of `uuid` atom families this crate currently dispatches to a
/// real decoder. Reflects the Phase 1 bounded ship list. Container-level
/// recognition (in `xifty-container-isobmff::parse_uuid`) covers a wider set
/// (PROF/USMT/PRPF/AOLY/MTDT-as-uuid/etc.); this list narrows that to the
/// subset that has a working Rust decoder behind it.
pub const SUPPORTED_USERTYPES: &[&str] = &["PROF", "USMT"];
