use std::process::Command;

#[path = "../../../../crates/xlsx/src/tests/xlsb_protection_fixture.rs"]
mod fixture;

#[test]
fn xlsb_protection_survives_cli_conversion() {
    let dir = std::env::temp_dir().join(format!("gridcraft-xlsb-protection-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("protected.xlsb");
    let output = dir.join("protected.xlsx");
    let bytes = fixture::file(&[], &fixture::record(535, &fixture::sheet_payload(0xCC1A, [1; 16])));
    std::fs::write(&input, bytes).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_gridcraft-cli")).arg("convert").arg(&input).arg(&output).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(std::fs::read(&output).unwrap())).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut xml).unwrap();
    assert!(xml.contains("<sheetProtection"), "{xml}");
    assert!(xml.contains("CC1A"), "{xml}");
    std::fs::remove_dir_all(dir).unwrap();
}
