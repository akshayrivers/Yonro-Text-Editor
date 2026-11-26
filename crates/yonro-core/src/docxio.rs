//! Docx is a door. The rope and the marks stay the manuscript.
//!
//! `docx` 1.1.2 (PoiScript/docx-rs, MIT) reads and writes the package.
//! Keystrokes never call this. Export writes the path it is given and does
//! not open any other file for writing.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use docx::document::{BodyContent, Hyperlink, Paragraph, ParagraphContent, Run, RunContent};
use docx::formatting::{CharacterProperty, JustificationVal};
use docx::{Docx, DocxFile};
use unicode_segmentation::UnicodeSegmentation;

use super::format::{flat_len, Align, Format, ListKind, MarkKind, ParaStyle, Paragraph as Para};

/// What came in from a docx, plus the parts that were left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxDoor {
    /// Plain prose. No markup.
    pub text: String,
    /// Baseline marks and paragraph props.
    pub format: Format,
    /// Images, tables, or tracked changes that were not brought in.
    pub skipped: Vec<String>,
}

/// Why a package could not be read or written.
#[derive(Debug)]
pub enum DocxDoorError {
    /// The path could not be opened or written.
    Io(String),
    /// The package has no main document.
    Package(String),
}

impl std::fmt::Display for DocxDoorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) | Self::Package(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for DocxDoorError {}

/// Read a docx into prose and marks. The file is not modified.
///
/// # Errors
/// `Io` when the path cannot be read. `Package` when it is not a docx.
pub fn import_docx(path: &Path) -> Result<DocxDoor, DocxDoorError> {
    let xml = document_xml(path)?;
    let skipped = skip_notes(&xml);
    let file =
        DocxFile::from_file(path).map_err(|err| DocxDoorError::Package(format!("{err:?}")))?;
    match file.parse() {
        Ok(docx) => Ok(from_document(&docx.document.body.content, skipped)),
        Err(_) => Ok(DocxDoor {
            text: plain_from_xml(&xml),
            format: Format::default(),
            skipped,
        }),
    }
}

/// Write a new docx. Does not open any path other than `dest`.
///
/// # Errors
/// `Io` when `dest` cannot be created.
pub fn export_docx(text: &str, format: &Format, dest: &Path) -> Result<(), DocxDoorError> {
    let mut docx = Docx::default();
    let mut flat = 0usize;
    for (line_idx, line) in text.split('\n').enumerate() {
        if line_idx > 0 {
            flat = flat.saturating_add(1);
        }
        let paragraph = paragraph_for(line, line_idx, flat, format);
        docx.document.push(paragraph);
        flat = flat.saturating_add(flat_len(line));
    }
    docx.write_file(dest)
        .map_err(|err| DocxDoorError::Io(format!("{err:?}")))?;
    Ok(())
}

fn from_document(content: &[BodyContent<'_>], mut skipped: Vec<String>) -> DocxDoor {
    let mut text = String::new();
    let mut format = Format::default();
    let mut flat = 0usize;
    let mut line_idx = 0usize;
    for (index, block) in content.iter().enumerate() {
        match block {
            BodyContent::Table(_) => {
                if !skipped.iter().any(|note| note == "tables") {
                    skipped.push("tables".to_string());
                }
            }
            BodyContent::Paragraph(paragraph) => {
                if index > 0 && !text.is_empty() {
                    text.push('\n');
                    flat = flat.saturating_add(1);
                }
                apply_paragraph(paragraph, line_idx, &mut format);
                flat = push_paragraph(paragraph, flat, &mut text, &mut format);
                line_idx = line_idx.saturating_add(1);
            }
        }
    }
    DocxDoor {
        text,
        format,
        skipped,
    }
}

fn apply_paragraph(paragraph: &Paragraph<'_>, line: usize, format: &mut Format) {
    let style = paragraph
        .property
        .style_id
        .as_ref()
        .map(|id| heading(&id.value))
        .unwrap_or_default();
    let align = paragraph
        .property
        .justification
        .as_ref()
        .map(|just| align_of(&just.value))
        .unwrap_or_default();
    let (list, indent) = paragraph
        .property
        .numbering
        .as_ref()
        .map_or((ListKind::None, 0), |num| {
            let list = if num.id.value == 2 {
                ListKind::Number
            } else {
                ListKind::Bullet
            };
            let indent = u8::try_from(num.level.value).unwrap_or(u8::MAX);
            (list, indent)
        });
    if style != ParaStyle::Normal || align != Align::Left || list != ListKind::None || indent > 0 {
        format.set_paragraph(Para {
            line,
            style,
            align,
            list,
            indent,
        });
    }
}

fn push_paragraph(
    paragraph: &Paragraph<'_>,
    mut flat: usize,
    text: &mut String,
    format: &mut Format,
) -> usize {
    for content in &paragraph.content {
        match content {
            ParagraphContent::Run(run) => flat = push_run(run, None, flat, text, format),
            ParagraphContent::Link(link) => {
                let href = link
                    .id
                    .as_deref()
                    .or(link.anchor.as_deref())
                    .unwrap_or("")
                    .to_string();
                flat = push_run(&link.content, Some(href.as_str()), flat, text, format);
            }
            ParagraphContent::BookmarkStart(_) | ParagraphContent::BookmarkEnd(_) => {}
        }
    }
    flat
}

fn push_run(
    run: &Run<'_>,
    href: Option<&str>,
    mut flat: usize,
    text: &mut String,
    format: &mut Format,
) -> usize {
    for content in &run.content {
        let RunContent::Text(piece) = content else {
            continue;
        };
        let piece = piece.text.as_ref();
        let len = flat_len(piece);
        if len == 0 {
            continue;
        }
        mark_run(&run.property, flat, flat.saturating_add(len), href, format);
        text.push_str(piece);
        flat = flat.saturating_add(len);
    }
    flat
}

fn mark_run(
    property: &CharacterProperty<'_>,
    start: usize,
    end: usize,
    href: Option<&str>,
    format: &mut Format,
) {
    if property
        .bold
        .as_ref()
        .is_some_and(|flag| flag.value.unwrap_or(true))
    {
        format.add_mark(start, end, MarkKind::Bold);
    }
    if property
        .italics
        .as_ref()
        .is_some_and(|flag| flag.value.unwrap_or(true))
    {
        format.add_mark(start, end, MarkKind::Italic);
    }
    if property.underline.is_some() {
        format.add_mark(start, end, MarkKind::Underline);
    }
    if property
        .strike
        .as_ref()
        .is_some_and(|flag| flag.value.unwrap_or(true))
    {
        format.add_mark(start, end, MarkKind::Strike);
    }
    if let Some(size) = &property.size {
        let points = u16::try_from(size.value.saturating_div(2)).unwrap_or(u16::MAX);
        format.add_mark(start, end, MarkKind::Size { points });
    }
    if property.color.is_some() {
        format.add_mark(start, end, MarkKind::Color { swatch: 1 });
    }
    if let Some(href) = href {
        if !href.is_empty() {
            format.add_mark(
                start,
                end,
                MarkKind::Link {
                    href: href.to_string(),
                },
            );
        }
    }
}

fn heading(style_id: &str) -> ParaStyle {
    let id = style_id.to_ascii_lowercase();
    if id.contains("heading1") || id == "1" {
        ParaStyle::H1
    } else if id.contains("heading2") || id == "2" {
        ParaStyle::H2
    } else if id.contains("heading3") || id == "3" {
        ParaStyle::H3
    } else {
        ParaStyle::Normal
    }
}

fn align_of(value: &JustificationVal) -> Align {
    match value {
        JustificationVal::Center => Align::Center,
        JustificationVal::Right | JustificationVal::End => Align::Right,
        JustificationVal::Both | JustificationVal::Distribute => Align::Justify,
        JustificationVal::Left | JustificationVal::Start => Align::Left,
    }
}

fn paragraph_for(
    line: &str,
    line_idx: usize,
    line_flat: usize,
    format: &Format,
) -> Paragraph<'static> {
    let mut property = docx::formatting::ParagraphProperty::default();
    if let Some(para) = format
        .paragraphs()
        .iter()
        .find(|para| para.line == line_idx)
    {
        property = match para.style {
            ParaStyle::H1 => property.style_id("Heading1"),
            ParaStyle::H2 => property.style_id("Heading2"),
            ParaStyle::H3 => property.style_id("Heading3"),
            ParaStyle::Normal => property,
        };
        property = match para.align {
            Align::Center => property.justification(JustificationVal::Center),
            Align::Right => property.justification(JustificationVal::Right),
            Align::Justify => property.justification(JustificationVal::Both),
            Align::Left => property,
        };
        let level = usize::from(para.indent);
        property = match para.list {
            ListKind::Bullet => property.numbering((1, level)),
            ListKind::Number => property.numbering((2, level)),
            ListKind::None => property,
        };
    }
    let mut paragraph = Paragraph::default().property(property);
    let cuts = cut_points(line, line_flat, format);
    for window in cuts.windows(2) {
        let start = window[0];
        let end = window[1];
        if end <= start {
            continue;
        }
        let slice = grapheme_slice(line, start, end);
        if slice.is_empty() {
            continue;
        }
        let flat_start = line_flat.saturating_add(start);
        let kinds = covering(format, flat_start);
        paragraph = push_piece(paragraph, &slice, &kinds);
    }
    if paragraph.content.is_empty() {
        paragraph = paragraph.push_text("");
    }
    paragraph
}

fn cut_points(line: &str, line_flat: usize, format: &Format) -> Vec<usize> {
    let len = line.graphemes(true).count();
    let mut cuts = vec![0, len];
    for mark in format.marks() {
        if mark.start > line_flat && mark.start < line_flat.saturating_add(len) {
            cuts.push(mark.start.saturating_sub(line_flat));
        }
        if mark.end > line_flat && mark.end < line_flat.saturating_add(len) {
            cuts.push(mark.end.saturating_sub(line_flat));
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

fn grapheme_slice(line: &str, start: usize, end: usize) -> String {
    line.graphemes(true)
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn covering(format: &Format, at: usize) -> Vec<MarkKind> {
    format
        .marks()
        .iter()
        .filter(|mark| mark.start <= at && at < mark.end)
        .map(|mark| mark.kind.clone())
        .collect()
}

fn push_piece(
    paragraph: Paragraph<'static>,
    slice: &str,
    kinds: &[MarkKind],
) -> Paragraph<'static> {
    let mut property = CharacterProperty::default();
    let mut href = None;
    for kind in kinds {
        match kind {
            MarkKind::Bold => property = property.bold(true),
            MarkKind::Italic => property = property.italics(true),
            MarkKind::Underline => {
                property = property.underline(docx::formatting::Underline::default());
            }
            MarkKind::Strike => property = property.strike(true),
            MarkKind::Size { points } => {
                property = property.size(usize::from(points.saturating_mul(2)));
            }
            MarkKind::Color { .. } | MarkKind::Highlight { .. } => {
                property = property.color("8A5A2A");
            }
            MarkKind::Link { href: link } => href = Some(link.clone()),
        }
    }
    let run = Run::default()
        .property(property)
        .push_text(slice.to_string());
    if let Some(href) = href {
        paragraph.push(Hyperlink::default().id(href).content(run))
    } else {
        paragraph.push(run)
    }
}

fn document_xml(path: &Path) -> Result<String, DocxDoorError> {
    let file =
        File::open(path).map_err(|err| DocxDoorError::Io(format!("{}: {err}", path.display())))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|err| DocxDoorError::Package(format!("{err}")))?;
    let mut entry = zip
        .by_name("word/document.xml")
        .map_err(|err| DocxDoorError::Package(format!("{err}")))?;
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|err| DocxDoorError::Io(format!("{err}")))?;
    Ok(xml)
}

fn skip_notes(xml: &str) -> Vec<String> {
    let mut notes = Vec::new();
    if xml.contains("w:drawing") || xml.contains("w:pict") {
        notes.push("images".to_string());
    }
    if xml.contains("w:tbl") {
        notes.push("tables".to_string());
    }
    if xml.contains("w:ins") || xml.contains("w:del") {
        notes.push("tracked changes".to_string());
    }
    notes
}

fn plain_from_xml(xml: &str) -> String {
    let mut lines = Vec::new();
    for para in xml.split("</w:p>") {
        if !para.contains("<w:p") {
            continue;
        }
        lines.push(texts_in(para));
    }
    lines.join("\n")
}

fn texts_in(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<w:t") {
        let after = &rest[start..];
        let Some(open) = after.find('>') else {
            break;
        };
        let Some(close) = after[open..].find("</w:t>") else {
            break;
        };
        out.push_str(&unescape(
            &after[open.saturating_add(1)..open.saturating_add(close)],
        ));
        rest = &after[open.saturating_add(close).saturating_add(6)..];
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use crate::format::{ListKind, MarkKind, ParaStyle};

    #[test]
    fn bold_heading_bullet_and_devanagari_round_trip() {
        let dir = std::env::temp_dir().join(format!("yonro-docx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.docx");
        let dest = dir.join("copy.docx");
        let mut format = Format::default();
        format.add_mark(0, 4, MarkKind::Bold);
        format.set_paragraph(Para {
            line: 0,
            style: ParaStyle::H1,
            align: Align::Left,
            list: ListKind::None,
            indent: 0,
        });
        format.set_paragraph(Para {
            line: 1,
            style: ParaStyle::Normal,
            align: Align::Left,
            list: ListKind::Bullet,
            indent: 0,
        });
        let text = "नमस्ते\nitem";
        export_docx(text, &format, &source).unwrap();
        let before = std::fs::read(&source).unwrap();
        export_docx(text, &format, &dest).unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), before);
        let brought = import_docx(&dest).unwrap();
        assert_eq!(brought.text, text);
        assert!(brought
            .format
            .marks()
            .iter()
            .any(|mark| matches!(mark.kind, MarkKind::Bold)));
        assert!(brought
            .format
            .paragraphs()
            .iter()
            .any(|para| para.style == ParaStyle::H1));
        assert!(brought
            .format
            .paragraphs()
            .iter()
            .any(|para| para.list == ListKind::Bullet));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_docx_keeps_the_words_and_the_file() {
        let dir = std::env::temp_dir().join(format!("yonro-docx-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("picture.docx");
        write_image_docx(&path);
        let before = std::fs::read(&path).unwrap();
        let brought = import_docx(&path).unwrap();
        assert!(brought.text.contains("hello"));
        assert!(brought.skipped.iter().any(|note| note == "images"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn write_image_docx(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::FileOptions::default();
        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#).unwrap();
        zip.start_file("_rels/.rels", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#).unwrap();
        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>hello image</w:t><w:drawing/></w:r></w:p></w:body></w:document>"#).unwrap();
        zip.finish().unwrap();
    }
}
