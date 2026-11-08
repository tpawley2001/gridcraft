//! Several notes filled with pictures: notes showing the same picture share one image
//! relationship and media part, different pictures get distinct relationship ids, and the media
//! names don't collide with a sheet picture's.

use std::io::Read;
use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{Anchor, Comment, Image, NotePictureFill, Workbook};

use crate::write_xlsx;

fn note(data: &[u8]) -> Comment {
    Comment {
        author: "A".into(),
        text: "t".into(),
        replies: vec![],
        threaded: false,
        resolved: false,
        visible: false,
        dates: vec![],
        note_box: None,
        picture_fill: Some(NotePictureFill { data: Arc::new(data.to_vec()), mime: "image/png".into(), attrs: vec![("type".into(), "frame".into())] }),
        note_shape: None,
    }
}

fn part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).ok()?;
    let mut v = Vec::new();
    f.read_to_end(&mut v).unwrap();
    Some(v)
}

#[test]
fn notes_share_or_split_picture_relationships() {
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).unwrap();
    sheet.images.push(Image {
        locks: None,
        hidden: false,
        hyperlink: None,
        name: None,
        id: 1,
        anchor: Anchor::default(),
        data: Arc::new(b"\x89PNG sheet picture".to_vec()),
        mime: "image/png".into(),
        alt: String::new(),
        blip_ext: vec![],
        link: None,
    });
    sheet.comments.insert(CellRef::new(0, 0), note(b"\x89PNG one"));
    sheet.comments.insert(CellRef::new(1, 0), note(b"\x89PNG two"));
    sheet.comments.insert(CellRef::new(2, 0), note(b"\x89PNG one"));
    let out = write_xlsx(&wb).unwrap();
    let vml = String::from_utf8(part(&out, "xl/drawings/vmlDrawing1.vml").unwrap()).unwrap();
    let ids: Vec<&str> = vml.split("o:relid=\"").skip(1).map(|s| &s[..s.find('"').unwrap()]).collect();
    assert_eq!(ids.len(), 3, "{vml}");
    assert_eq!(ids[0], ids[2], "{vml}");
    assert_ne!(ids[0], ids[1], "{vml}");
    let rels = String::from_utf8(part(&out, "xl/drawings/_rels/vmlDrawing1.vml.rels").unwrap()).unwrap();
    assert_eq!(rels.matches("<Relationship ").count(), 2, "{rels}");
    let mut media: Vec<Vec<u8>> = (1..=3).filter_map(|i| part(&out, &format!("xl/media/image{i}.png"))).collect();
    media.sort();
    assert_eq!(media, [b"\x89PNG one".to_vec(), b"\x89PNG sheet picture".to_vec(), b"\x89PNG two".to_vec()]);
}
