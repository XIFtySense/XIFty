//! Bounded walk-up discovery against synthetic temp-dir layouts.

use std::fs;
use std::path::PathBuf;

use xifty_sidecar_mediapro::discover_walk_up;

fn unique_temp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "xifty-sidecar-mediapro-test-{name}-{pid}",
        pid = std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn finds_index_two_levels_up_typical_sony_layout() {
    let root = unique_temp("two-levels");
    let m4root = root.join("M4ROOT");
    let clip_dir = m4root.join("CLIP");
    fs::create_dir_all(&clip_dir).unwrap();
    let primary = clip_dir.join("C0001.MP4");
    fs::write(&primary, b"x").unwrap();
    let index = m4root.join("MEDIAPRO.XML");
    fs::write(&index, b"<MediaProfile/>").unwrap();

    let hits = discover_walk_up(&primary);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, index);
    assert_eq!(hits[0].label, "MEDIAPRO|C0001.MP4");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn finds_index_one_level_up_sibling_dir() {
    let root = unique_temp("one-level");
    fs::create_dir_all(&root).unwrap();
    let primary = root.join("C0001.MP4");
    fs::write(&primary, b"x").unwrap();
    let index = root.join("MEDIAPRO.XML");
    fs::write(&index, b"<MediaProfile/>").unwrap();

    let hits = discover_walk_up(&primary);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, index);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn finds_index_at_max_walk_up_depth_4() {
    let root = unique_temp("depth-4");
    // primary at root/a/b/c/d/CLIP/C0001.MP4 — primary.parent() = CLIP,
    // walking up: CLIP, d, c, b, a → MEDIAPRO at a (4 hops from CLIP).
    let deep = root.join("a").join("b").join("c").join("d").join("CLIP");
    fs::create_dir_all(&deep).unwrap();
    let primary = deep.join("C0001.MP4");
    fs::write(&primary, b"x").unwrap();
    let index = root.join("a").join("MEDIAPRO.XML");
    fs::write(&index, b"<MediaProfile/>").unwrap();

    let hits = discover_walk_up(&primary);
    assert_eq!(hits.len(), 1, "expected hit, got {hits:?}");
    assert_eq!(hits[0].path, index);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn does_not_find_index_5_levels_up() {
    let root = unique_temp("depth-5");
    // primary.parent() = CLIP, walking up over 5 levels (CLIP, e, d, c, b, a)
    // exceeds MAX_WALK_UP=4 → no hit when index is at root.
    let deep = root
        .join("a")
        .join("b")
        .join("c")
        .join("d")
        .join("e")
        .join("CLIP");
    fs::create_dir_all(&deep).unwrap();
    let primary = deep.join("C0001.MP4");
    fs::write(&primary, b"x").unwrap();
    let index = root.join("MEDIAPRO.XML");
    fs::write(&index, b"<MediaProfile/>").unwrap();

    let hits = discover_walk_up(&primary);
    assert!(hits.is_empty(), "expected no hits at depth 5, got {hits:?}");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn case_insensitive_lowercase_mediapro_xml() {
    let root = unique_temp("lowercase");
    let m4root = root.join("M4ROOT");
    let clip_dir = m4root.join("CLIP");
    fs::create_dir_all(&clip_dir).unwrap();
    let primary = clip_dir.join("C0001.MP4");
    fs::write(&primary, b"x").unwrap();
    let index = m4root.join("mediapro.xml");
    fs::write(&index, b"<MediaProfile/>").unwrap();

    let hits = discover_walk_up(&primary);
    assert_eq!(hits.len(), 1, "case-insensitive match should hit");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn no_match_returns_empty_no_panic() {
    let root = unique_temp("none");
    let primary = root.join("C0001.MP4");
    fs::write(&primary, b"x").unwrap();
    let hits = discover_walk_up(&primary);
    assert!(hits.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn walks_to_filesystem_root_without_panic() {
    // primary at the actual filesystem root parent — should terminate
    // gracefully when no MEDIAPRO.XML exists anywhere on the path.
    let primary = PathBuf::from("/nonexistent-xifty-test-path/CLIP/C0001.MP4");
    let _ = discover_walk_up(&primary); // must not panic
}
