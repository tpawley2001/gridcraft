//! Picture Format > Size: "Lock aspect ratio" (`a:picLocks noChangeAspect`, ECMA-376 Part 1
//! §20.1.2.2.31) and Height/Width edits that keep proportions while it is on.

use gridcraft_model::{AnchorMode, Image, PictureLocks};
use serde_json::json;

use super::s;
use crate::Session;

/// An 80 x 40 pt picture on the first sheet with the given locks.
fn picture(s: &mut Session, locks: Option<PictureLocks>) -> u32 {
    let id = super::add_image(s, "B2", AnchorMode::MoveAndSize);
    let img = image_mut(s, id);
    img.locks = locks;
    img.anchor.width = 80.0;
    img.anchor.height = 40.0;
    id
}

fn image_mut(s: &mut Session, id: u32) -> &mut Image {
    let d = s.doc_mut().unwrap();
    let wb = std::sync::Arc::make_mut(&mut d.wb);
    wb.sheet_mut(0).unwrap().images.iter_mut().find(|i| i.id == id).unwrap()
}

fn image(s: &Session, id: u32) -> &Image {
    s.doc().unwrap().wb.sheet(0).unwrap().images.iter().find(|i| i.id == id).unwrap()
}

fn size(s: &Session, id: u32) -> (f32, f32) {
    let a = &image(s, id).anchor;
    (a.width, a.height)
}

fn locked(on: bool) -> Option<PictureLocks> {
    Some(PictureLocks { no_change_aspect: on, ..Default::default() })
}

#[test]
fn default_picture_is_locked_and_height_scales_width() {
    let mut s = s();
    let id = picture(&mut s, None);
    assert!(image(&s, id).aspect_locked(), "an inserted picture (no picLocks yet) saves noChangeAspect=1");
    s.execute("image.setSize", json!({"id": id, "height": 60})).unwrap();
    assert_eq!(size(&s, id), (120.0, 60.0));
}

#[test]
fn locked_width_scales_height() {
    let mut s = s();
    let id = picture(&mut s, locked(true));
    s.execute("image.setSize", json!({"id": id, "width": 50})).unwrap();
    assert_eq!(size(&s, id), (50.0, 25.0));
}

#[test]
fn unlocked_changes_one_dimension_only() {
    let mut s = s();
    let id = picture(&mut s, locked(false));
    assert!(!image(&s, id).aspect_locked());
    s.execute("image.setSize", json!({"id": id, "height": 60})).unwrap();
    assert_eq!(size(&s, id), (80.0, 60.0));
}

#[test]
fn both_dimensions_are_taken_as_given() {
    let mut s = s();
    let id = picture(&mut s, locked(true));
    s.execute("image.setSize", json!({"id": id, "width": 100, "height": 10})).unwrap();
    assert_eq!(size(&s, id), (100.0, 10.0));
}

#[test]
fn locked_resize_past_the_cap_keeps_the_ratio() {
    let mut s = s();
    let id = picture(&mut s, locked(true));
    s.execute("image.setSize", json!({"id": id, "height": 9000})).unwrap();
    assert_eq!(size(&s, id), (10_000.0, 5_000.0));
}

#[test]
fn errors_without_a_size_or_for_an_unknown_picture() {
    let mut s = s();
    let id = picture(&mut s, None);
    assert!(s.execute("image.setSize", json!({"id": id})).is_err());
    assert!(s.execute("image.setSize", json!({"id": id + 999, "width": 10})).is_err());
    assert!(s.execute("image.setLockAspectRatio", json!({"id": id + 999, "locked": true})).is_err());
    assert_eq!(size(&s, id), (80.0, 40.0));
}

#[test]
fn toggling_the_lock_keeps_other_flags() {
    let mut s = s();
    let id = picture(&mut s, None);
    let r = s.execute("image.setLockAspectRatio", json!({"id": id, "locked": false})).unwrap();
    assert_eq!(r["locked"], json!(false));
    assert_eq!(image(&s, id).locks, locked(false));

    let id2 = picture(&mut s, Some(PictureLocks { no_crop: true, no_rot: true, ..Default::default() }));
    s.execute("image.setLockAspectRatio", json!({"id": id2, "locked": true})).unwrap();
    assert_eq!(image(&s, id2).locks, Some(PictureLocks { no_crop: true, no_rot: true, no_change_aspect: true, ..Default::default() }));
}

#[test]
fn unlocked_survives_xlsx_round_trip() {
    let mut s = s();
    let id = picture(&mut s, None);
    s.execute("image.setLockAspectRatio", json!({"id": id, "locked": false})).unwrap();
    let saved = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap();
    s.execute("file.open", json!({"name": "lock.xlsx", "base64": saved["base64"]})).unwrap();
    let img = &s.doc().unwrap().wb.sheet(0).unwrap().images[0];
    assert!(!img.aspect_locked());
}

#[test]
fn set_size_is_one_undo_step() {
    let mut s = s();
    let id = picture(&mut s, None);
    s.execute("image.setSize", json!({"id": id, "height": 60})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(size(&s, id), (80.0, 40.0));
}
