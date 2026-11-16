//! The SpreadsheetML forms of data connections (ECMA-376 Part 1 §18.13 `connection`) and query
//! tables (§18.12 `queryTable`) that the binary-workbook readers translate their records to, and
//! the pass-through parts that carry them. Each reader decodes its own records ([MS-XLSB] or
//! [MS-XLS]) into these structs; the XML written from them is the same whatever the source.

use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_model::{Preserved, PreservedPart, PreservedRel};

use crate::write::XML_DECL;
use crate::xml::esc_attr;

pub(crate) const CT_CONNECTIONS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml";
pub(crate) const CT_QUERY_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml";
pub(crate) const REL_CONNECTIONS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections";
pub(crate) const REL_QUERY_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

/// The `type` values of `textField` (§18.13.13), indexed as both binary formats number them.
const TEXT_FIELD_TYPES: [&str; 10] = ["general", "text", "MDY", "DMY", "YMD", "MYD", "DYM", "YDM", "skip", "EMD"];

/// The `textField` type numbered `n` in the binary formats.
pub(crate) fn text_field_type(n: u32) -> Option<&'static str> {
    usize::try_from(n).ok().and_then(|i| TEXT_FIELD_TYPES.get(i)).copied()
}

/// Whether XML 1.0 can carry the character.
pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

pub(crate) fn attr(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, " {name}=\"{}\"", esc_attr(value));
}
pub(crate) fn flag(out: &mut String, name: &str, on: bool) {
    let _ = write!(out, " {name}=\"{}\"", u8::from(on));
}

/// A `<connection>` element. Optional attributes left at their defaults are not written.
#[derive(Default)]
pub(crate) struct Connection {
    pub id: u32,
    pub name: String,
    /// The `type` attribute (1 ODBC, 2 DAO, 4 web, 5 OLE DB, 6 text, 7 ADO).
    pub ty: u32,
    pub description: Option<String>,
    pub source_file: Option<String>,
    pub odc_file: Option<String>,
    pub keep_alive: bool,
    pub interval: u16,
    pub reconnection_method: Option<u32>,
    pub refreshed_version: u8,
    pub min_refreshable_version: u8,
    pub save_password: bool,
    pub new: bool,
    pub deleted: bool,
    pub only_use_connection_file: bool,
    pub background: bool,
    pub refresh_on_load: bool,
    pub save_data: bool,
    pub credentials: Option<&'static str>,
    pub single_sign_on_id: Option<String>,
    /// The child elements in schema order (`dbPr`, `webPr`, `textPr`, `parameters`), as XML.
    pub children: Vec<String>,
}

impl Connection {
    pub(crate) fn xml(&self) -> String {
        let mut x = format!("<connection id=\"{}\"", self.id);
        if let Some(s) = &self.source_file {
            attr(&mut x, "sourceFile", s);
        }
        if let Some(s) = &self.odc_file {
            attr(&mut x, "odcFile", s);
        }
        if self.keep_alive {
            flag(&mut x, "keepAlive", true);
        }
        if self.interval != 0 {
            let _ = write!(x, " interval=\"{}\"", self.interval);
        }
        attr(&mut x, "name", &self.name);
        if let Some(s) = &self.description {
            attr(&mut x, "description", s);
        }
        let _ = write!(x, " type=\"{}\"", self.ty);
        if let Some(m) = self.reconnection_method {
            let _ = write!(x, " reconnectionMethod=\"{m}\"");
        }
        let _ = write!(x, " refreshedVersion=\"{}\"", self.refreshed_version);
        if self.min_refreshable_version != 0 {
            let _ = write!(x, " minRefreshableVersion=\"{}\"", self.min_refreshable_version);
        }
        for (on, name) in [
            (self.save_password, "savePassword"),
            (self.new, "new"),
            (self.deleted, "deleted"),
            (self.only_use_connection_file, "onlyUseConnectionFile"),
            (self.background, "background"),
            (self.refresh_on_load, "refreshOnLoad"),
            (self.save_data, "saveData"),
        ] {
            if on {
                flag(&mut x, name, true);
            }
        }
        if let Some(s) = self.credentials {
            attr(&mut x, "credentials", s);
        }
        if let Some(s) = &self.single_sign_on_id {
            attr(&mut x, "singleSignOnId", s);
        }
        x.push('>');
        for child in &self.children {
            x.push_str(child);
        }
        x.push_str("</connection>");
        x
    }
}

/// A `<dbPr>` element.
pub(crate) struct DbPr {
    pub connection: String,
    pub command: Option<String>,
    pub server_command: Option<String>,
    /// The `commandType` attribute; `None` for the default (SQL).
    pub command_type: Option<u32>,
}

impl DbPr {
    pub(crate) fn xml(&self) -> String {
        let mut x = String::from("<dbPr");
        attr(&mut x, "connection", &self.connection);
        if let Some(s) = &self.command {
            attr(&mut x, "command", s);
        }
        if let Some(s) = &self.server_command {
            attr(&mut x, "serverCommand", s);
        }
        if let Some(t) = self.command_type {
            let _ = write!(x, " commandType=\"{t}\"");
        }
        x.push_str("/>");
        x
    }
}

/// A `<webPr>` element.
#[derive(Default)]
pub(crate) struct WebPr {
    pub xml: bool,
    pub source_data: bool,
    pub parse_pre: bool,
    pub consecutive: bool,
    pub first_row: bool,
    pub xl97: bool,
    pub text_dates: bool,
    pub xl2000: bool,
    pub url: Option<String>,
    pub post: Option<String>,
    pub html_tables: bool,
    /// `rtf` or `all`; `None` for the default (`none`).
    pub html_format: Option<&'static str>,
    pub edit_page: Option<String>,
    /// The `tables` child: its count and its `m`/`s`/`x` items as XML.
    pub tables: Option<(u32, String)>,
}

impl WebPr {
    pub(crate) fn xml(&self) -> String {
        let mut x = String::from("<webPr");
        for (on, name) in [
            (self.xml, "xml"),
            (self.source_data, "sourceData"),
            (self.parse_pre, "parsePre"),
            (self.consecutive, "consecutive"),
            (self.first_row, "firstRow"),
            (self.xl97, "xl97"),
            (self.text_dates, "textDates"),
            (self.xl2000, "xl2000"),
        ] {
            if on {
                flag(&mut x, name, true);
            }
        }
        if let Some(s) = &self.url {
            attr(&mut x, "url", s);
        }
        if let Some(s) = &self.post {
            attr(&mut x, "post", s);
        }
        if self.html_tables {
            flag(&mut x, "htmlTables", true);
        }
        if let Some(s) = self.html_format {
            attr(&mut x, "htmlFormat", s);
        }
        if let Some(s) = &self.edit_page {
            attr(&mut x, "editPage", s);
        }
        match &self.tables {
            None => x.push_str("/>"),
            Some((count, items)) => {
                let _ = write!(x, "><tables count=\"{count}\">{items}</tables></webPr>");
            }
        }
        x
    }
}

/// A `<textPr>` element with its `textFields`.
pub(crate) struct TextPr {
    pub prompt: bool,
    /// `mac` or `dos`; `None` for the default (`win`).
    pub file_type: Option<&'static str>,
    pub first_row: u32,
    pub source_file: String,
    pub delimited: bool,
    /// The decimal and thousands separators when they differ from `.` and `,`.
    pub decimal: Option<char>,
    pub thousands: Option<char>,
    pub tab: bool,
    pub space: bool,
    pub comma: bool,
    pub semicolon: bool,
    pub consecutive: bool,
    /// `singleQuote` or `none`; `None` for the default (`doubleQuote`).
    pub qualifier: Option<&'static str>,
    pub delimiter: Option<char>,
    /// Each field's type ([`text_field_type`]) and start position.
    pub fields: Vec<(&'static str, u32)>,
}

impl TextPr {
    pub(crate) fn xml(&self) -> String {
        let mut x = String::from("<textPr");
        if !self.prompt {
            flag(&mut x, "prompt", false);
        }
        if let Some(s) = self.file_type {
            attr(&mut x, "fileType", s);
        }
        if self.first_row != 1 {
            let _ = write!(x, " firstRow=\"{}\"", self.first_row);
        }
        if !self.source_file.is_empty() {
            attr(&mut x, "sourceFile", &self.source_file);
        }
        if !self.delimited {
            flag(&mut x, "delimited", false);
        }
        for (c, name) in [(self.decimal, "decimal"), (self.thousands, "thousands")] {
            if let Some(c) = c {
                attr(&mut x, name, &c.to_string());
            }
        }
        if !self.tab {
            flag(&mut x, "tab", false);
        }
        for (on, name) in [(self.space, "space"), (self.comma, "comma"), (self.semicolon, "semicolon"), (self.consecutive, "consecutive")] {
            if on {
                flag(&mut x, name, true);
            }
        }
        if let Some(s) = self.qualifier {
            attr(&mut x, "qualifier", s);
        }
        if let Some(c) = self.delimiter {
            attr(&mut x, "delimiter", &c.to_string());
        }
        let _ = write!(x, "><textFields count=\"{}\">", self.fields.len());
        for (ty, position) in &self.fields {
            x.push_str("<textField");
            if *ty != "general" {
                attr(&mut x, "type", ty);
            }
            if *position != 0 {
                let _ = write!(x, " position=\"{position}\"");
            }
            x.push_str("/>");
        }
        x.push_str("</textFields></textPr>");
        x
    }
}

/// A `<queryTable>` part.
pub(crate) struct QueryTable {
    pub name: String,
    pub headers: bool,
    pub row_numbers: bool,
    pub disable_refresh: bool,
    pub background_refresh: bool,
    pub first_background_refresh: bool,
    pub refresh_on_load: bool,
    /// `overwriteClear` or `insertClear`; `None` for the default (`insertDelete`).
    pub grow_shrink: Option<&'static str>,
    pub fill_formulas: bool,
    pub remove_data_on_save: bool,
    pub disable_edit: bool,
    pub preserve_formatting: bool,
    pub adjust_column_width: bool,
    pub intermediate: bool,
    pub connection_id: u32,
    pub auto_format_id: u16,
    pub apply_number: bool,
    pub apply_border: bool,
    pub apply_font: bool,
    pub apply_pattern: bool,
    pub apply_alignment: bool,
    /// Not written when the source format has no such setting.
    pub apply_width_height: Option<bool>,
    pub refresh: Option<QueryTableRefresh>,
}

impl QueryTable {
    pub(crate) fn xml(&self) -> String {
        let mut x = format!("{XML_DECL}<queryTable xmlns=\"{NS_MAIN}\"");
        attr(&mut x, "name", &self.name);
        for (on, name, default) in [
            (self.headers, "headers", true),
            (self.row_numbers, "rowNumbers", false),
            (self.disable_refresh, "disableRefresh", false),
            (self.background_refresh, "backgroundRefresh", true),
            (self.first_background_refresh, "firstBackgroundRefresh", false),
            (self.refresh_on_load, "refreshOnLoad", false),
        ] {
            if on != default {
                flag(&mut x, name, on);
            }
        }
        if let Some(g) = self.grow_shrink {
            attr(&mut x, "growShrinkType", g);
        }
        for (on, name, default) in [
            (self.fill_formulas, "fillFormulas", false),
            (self.remove_data_on_save, "removeDataOnSave", false),
            (self.disable_edit, "disableEdit", false),
            (self.preserve_formatting, "preserveFormatting", true),
            (self.adjust_column_width, "adjustColumnWidth", true),
            (self.intermediate, "intermediate", false),
        ] {
            if on != default {
                flag(&mut x, name, on);
            }
        }
        let _ = write!(x, " connectionId=\"{}\" autoFormatId=\"{}\"", self.connection_id, self.auto_format_id);
        for (on, name) in [
            (self.apply_number, "applyNumberFormats"),
            (self.apply_border, "applyBorderFormats"),
            (self.apply_font, "applyFontFormats"),
            (self.apply_pattern, "applyPatternFormats"),
            (self.apply_alignment, "applyAlignmentFormats"),
        ] {
            flag(&mut x, name, on);
        }
        if let Some(on) = self.apply_width_height {
            flag(&mut x, "applyWidthHeightFormats", on);
        }
        match &self.refresh {
            Some(r) => {
                x.push('>');
                r.write(&mut x);
                x.push_str("</queryTable>");
            }
            None => x.push_str("/>"),
        }
        x
    }
}

/// A `<queryTableRefresh>` element with its fields and deleted fields.
pub(crate) struct QueryTableRefresh {
    pub next_id: u32,
    pub preserve_sort_filter_layout: bool,
    pub field_id_wrapped: bool,
    pub headers_in_last_refresh: bool,
    pub minimum_version: u16,
    pub unbound_columns_left: u16,
    pub unbound_columns_right: u16,
    pub fields: Vec<QueryTableField>,
    pub deleted: Vec<String>,
}

impl QueryTableRefresh {
    fn write(&self, x: &mut String) {
        let _ = write!(x, "<queryTableRefresh nextId=\"{}\"", self.next_id);
        if !self.preserve_sort_filter_layout {
            flag(x, "preserveSortFilterLayout", false);
        }
        if self.field_id_wrapped {
            flag(x, "fieldIdWrapped", true);
        }
        if !self.headers_in_last_refresh {
            flag(x, "headersInLastRefresh", false);
        }
        if self.minimum_version != 0 {
            let _ = write!(x, " minimumVersion=\"{}\"", self.minimum_version);
        }
        if self.unbound_columns_left != 0 {
            let _ = write!(x, " unboundColumnsLeft=\"{}\"", self.unbound_columns_left);
        }
        if self.unbound_columns_right != 0 {
            let _ = write!(x, " unboundColumnsRight=\"{}\"", self.unbound_columns_right);
        }
        let _ = write!(x, "><queryTableFields count=\"{}\">", self.fields.len());
        for f in &self.fields {
            f.write(x);
        }
        x.push_str("</queryTableFields>");
        if !self.deleted.is_empty() {
            let _ = write!(x, "<queryTableDeletedFields count=\"{}\">", self.deleted.len());
            for name in &self.deleted {
                x.push_str("<deletedField");
                attr(x, "name", name);
                x.push_str("/>");
            }
            x.push_str("</queryTableDeletedFields>");
        }
        x.push_str("</queryTableRefresh>");
    }
}

/// A `<queryTableField>` element.
pub(crate) struct QueryTableField {
    pub id: u32,
    pub name: Option<String>,
    pub data_bound: bool,
    pub row_numbers: bool,
    pub fill_formulas: bool,
    pub clipped: bool,
    pub table_column_id: u32,
}

impl QueryTableField {
    fn write(&self, x: &mut String) {
        let _ = write!(x, "<queryTableField id=\"{}\"", self.id);
        if let Some(s) = &self.name {
            attr(x, "name", s);
        }
        if !self.data_bound {
            flag(x, "dataBound", false);
        }
        for (on, name) in [(self.row_numbers, "rowNumbers"), (self.fill_formulas, "fillFormulas"), (self.clipped, "clipped")] {
            if on {
                flag(x, name, true);
            }
        }
        if self.table_column_id != 0 {
            let _ = write!(x, " tableColumnId=\"{}\"", self.table_column_id);
        }
        x.push_str("/>");
    }
}

/// The workbook's `xl/connections.xml` holding `connections` (`<connection>` elements), as
/// pass-through content; `None` when there are none.
pub(crate) fn connections_part<S: AsRef<str>>(connections: &[S]) -> Option<Arc<Preserved>> {
    if connections.is_empty() {
        return None;
    }
    let mut xml = format!("{XML_DECL}<connections xmlns=\"{NS_MAIN}\">");
    for c in connections {
        xml.push_str(c.as_ref());
    }
    xml.push_str("</connections>");
    let name = "xl/connections.xml".to_string();
    Some(Arc::new(Preserved {
        rels: vec![PreservedRel { id: "rIdConnections".into(), rel_type: REL_CONNECTIONS.into(), target: name.clone(), external: false }],
        parts: vec![PreservedPart { name, content_type: CT_CONNECTIONS.into(), data: xml.into_bytes().into(), rels: vec![], ..Default::default() }],
        ..Preserved::default()
    }))
}

/// Adds query table part number `n` (`xl/queryTables/queryTableN.xml`) to a sheet's pass-through
/// content, linked by relationship `rel_id`.
pub(crate) fn add_query_table_part(p: &mut Preserved, rel_id: String, n: u32, xml: String) {
    let name = format!("xl/queryTables/queryTable{n}.xml");
    p.rels.push(PreservedRel { id: rel_id, rel_type: REL_QUERY_TABLE.into(), target: name.clone(), external: false });
    p.parts.push(PreservedPart { name, content_type: CT_QUERY_TABLE.into(), data: xml.into_bytes().into(), rels: vec![], ..Default::default() });
}
