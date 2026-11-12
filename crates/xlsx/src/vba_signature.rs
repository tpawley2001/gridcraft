//! The digital signature of a VBA project ([MS-OFFMACRO2]): parts related to `xl/vbaProject.bin`
//! by its own relationships part. A signature covers only the project's bytes, so it is kept
//! opaque and written back only while the workbook's VBA project is byte-for-byte what was read
//! with it. Signatures are never parsed, verified or created.

use std::sync::Arc;

use gridcraft_model::{PreservedPart, PreservedRel, PreservedVbaSignature, Workbook};

use crate::IoError;
use crate::package::{Package, normalize};
use crate::write::{Out, Rels};

/// (relationship type, content type) of the legacy, agile and V3 signatures.
const KINDS: [(&str, &str); 3] = [
    ("http://schemas.microsoft.com/office/2006/relationships/vbaProjectSignature", "application/vnd.ms-office.vbaProjectSignature"),
    ("http://schemas.microsoft.com/office/2014/relationships/vbaProjectSignatureAgile", "application/vnd.ms-office.vbaProjectSignatureAgile"),
    ("http://schemas.microsoft.com/office/2020/07/relationships/vbaProjectSignatureV3", "application/vnd.ms-office.vbaProjectSignatureV3"),
];

/// The kind of a relationship type's last segment, if it is a signature.
fn kind_of(last: &str) -> Option<(&'static str, &'static str)> {
    KINDS.iter().copied().find(|(ty, _)| ty.rsplit('/').next() == Some(last))
}

/// Reads the signature parts of the VBA project part `vba_part`, read as `project`. A
/// relationship whose part is missing or unreadable is skipped with a warning; only size limits fail.
pub(crate) fn read(
    pkg: &mut Package<'_>,
    vba_part: &str,
    project: &Arc<Vec<u8>>,
    warn: &mut dyn FnMut(String),
) -> Result<Option<PreservedVbaSignature>, IoError> {
    let rels = match pkg.rels(vba_part) {
        Ok(r) => r,
        Err(IoError::TooLarge(m)) => return Err(IoError::TooLarge(m)),
        Err(e) => {
            warn(format!("the VBA project's relationships could not be read; its signature was dropped: {e}"));
            return Ok(None);
        }
    };
    let mut sig = PreservedVbaSignature { project: Arc::clone(project), ..Default::default() };
    for r in rels.iter().filter(|r| !r.external) {
        let Some((ty, content_type)) = kind_of(&r.kind) else { continue };
        let name = normalize(&r.target);
        // The VBA project itself, its relationships part, or a part already kept: not a signature.
        if name == normalize(vba_part) || name.ends_with(".rels") || name == "[content_types].xml" {
            continue;
        }
        if sig.parts.iter().any(|p| normalize(&p.name) == name) {
            continue;
        }
        match pkg.read(&r.target) {
            Ok(Some(data)) => {
                sig.rels.push(PreservedRel { id: r.id.clone(), rel_type: ty.to_string(), target: r.target.clone(), external: false });
                sig.parts.push(PreservedPart { name: r.target.clone(), content_type: content_type.to_string(), data: data.into(), rels: vec![] });
            }
            Ok(None) => warn(format!("the VBA project's signature names a missing part {} and was dropped", r.target)),
            Err(IoError::TooLarge(m)) => return Err(IoError::TooLarge(m)),
            Err(e) => warn(format!("the VBA project's signature part {} could not be read and was dropped: {e}", r.target)),
        }
    }
    Ok(if sig.parts.is_empty() { None } else { Some(sig) })
}

/// Writes the kept signature of the VBA project written as `xl/vbaProject.bin` with `project`:
/// only when `project` is the exact bytes the signature was read with.
pub(crate) fn write(wb: &Workbook, project: &Arc<Vec<u8>>, out: &mut Out) {
    let Some(sig) = wb.preserved.as_deref().and_then(|p| p.vba_signature.as_ref()) else { return };
    if !(Arc::ptr_eq(&sig.project, project) || sig.project.as_slice() == project.as_slice()) {
        return;
    }
    let mut rels = Rels::default();
    for rel in &sig.rels {
        let Some(part) = sig.parts.iter().find(|p| p.name == rel.target) else { continue };
        // A name GridCraft already wrote stays that part; the signature goes rather than replace it.
        if out.has_part(&part.name) {
            continue;
        }
        let target = match part.name.strip_prefix("xl/") {
            Some(t) if !t.contains('/') => t.to_string(),
            _ => format!("/{}", part.name),
        };
        rels.add_kept(&rel.id, &rel.rel_type, &target, false);
        out.part(&part.name, Some(&part.content_type), part.data.to_vec());
    }
    if !rels.is_empty() {
        out.part("xl/_rels/vbaProject.bin.rels", None, rels.xml().into_bytes());
    }
}
