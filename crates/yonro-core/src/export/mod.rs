//! Manuscript export (`P5.5`): outline-order markdown/HTML compilation.
//!
//! * Order: project title, then `# Act`, `## Chapter`, scene bodies split
//!   by [`SCENE_SEPARATOR`]. Scenes without a file are skipped; scene
//!   titles stay in the outline (novel convention: untitled scenes).
//! * `words` counts draft words in scene bodies (same word-bound rule as
//!   buffer stats); titles and separators are not counted.
//! * HTML is one self-contained file (inline `<style>`, `@media print`
//!   rules) so print-to-PDF works with no extra crate.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use unicode_segmentation::UnicodeSegmentation;

use super::manuscript::{Manuscript, NodeId};

/// Scene separator (literal in markdown, centered in HTML).
pub const SCENE_SEPARATOR: &str = "* * *";

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Html,
}

impl ExportFormat {
    /// Parse `"md"`/`"markdown"`/`"html"` (case-insensitive, trimmed).
    ///
    /// # Errors
    /// `BadFormat` naming the offending value.
    pub fn parse(raw: &str) -> Result<Self, ExportError> {
        match raw.trim().to_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Markdown),
            "html" => Ok(Self::Html),
            _ => Err(ExportError::BadFormat(raw.to_string())),
        }
    }

    /// File extension without the dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Html => "html",
        }
    }
}

/// Export failures (message names the path/value plus the reason).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// Unknown format string.
    BadFormat(String),
    /// Filesystem failure (message already includes the path).
    Io(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadFormat(value) => write!(
                formatter,
                "unknown export format {value:?}: expected md or html"
            ),
            Self::Io(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Compiled manuscript (before writing).
#[derive(Debug, Clone)]
pub struct CompiledExport {
    pub text: String,
    /// Draft words across exported scene bodies.
    pub words: usize,
    /// Number of exported scenes.
    pub scenes: usize,
}

/// Written export receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    /// Absolute path written.
    pub path: String,
    pub words: usize,
    pub scenes: usize,
}

/// Draft-word counter (same rule as buffer stats: word-bound chunks with an
/// alphanumeric).
#[must_use]
pub fn count_words(text: &str) -> usize {
    let mut words = 0usize;
    for chunk in text.split_word_bounds() {
        if chunk.chars().any(char::is_alphanumeric) {
            words = words.saturating_add(1);
        }
    }
    words
}

/// Filename slug for the default export path (lowercase, dashes).
#[must_use]
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for ch in title.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("manuscript");
    }
    out
}

fn esc_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Outline-order sections: title, acts, chapters, and scene bodies for
/// scenes present in `scene_texts` (file-backed scenes only).
fn collect_sections(
    manuscript: &Manuscript,
    scene_texts: &BTreeMap<NodeId, String>,
) -> (String, Vec<Section>) {
    let mut sections = Vec::new();
    let root = manuscript.root();
    for act in manuscript.children(root) {
        sections.push(Section::Act(act.title.clone()));
        for chapter in manuscript.children(act.id) {
            sections.push(Section::Chapter(chapter.title.clone()));
            for scene in manuscript.children(chapter.id) {
                if let Some(body) = scene_texts.get(&scene.id) {
                    sections.push(Section::Scene(body.clone()));
                }
            }
        }
    }
    (manuscript.title().to_string(), sections)
}

#[derive(Debug, Clone)]
enum Section {
    Act(String),
    Chapter(String),
    Scene(String),
}

/// Compile the manuscript in outline order (no I/O).
#[must_use]
pub fn compile_manuscript(
    manuscript: &Manuscript,
    scene_texts: &BTreeMap<NodeId, String>,
    format: ExportFormat,
) -> CompiledExport {
    let (title, sections) = collect_sections(manuscript, scene_texts);
    match format {
        ExportFormat::Markdown => compile_markdown(&title, &sections),
        ExportFormat::Html => compile_html(&title, &sections),
    }
}

fn compile_markdown(title: &str, sections: &[Section]) -> CompiledExport {
    let mut text = format!("# {title}\n");
    let mut words = 0usize;
    let mut scenes = 0usize;
    let mut first_scene = true;
    for section in sections {
        match section {
            Section::Act(name) => {
                let _ = writeln!(text, "\n# {name}");
            }
            Section::Chapter(name) => {
                let _ = writeln!(text, "\n## {name}");
            }
            Section::Scene(body) => {
                if first_scene {
                    first_scene = false;
                } else {
                    let _ = writeln!(text, "\n{SCENE_SEPARATOR}");
                }
                let _ = writeln!(text, "\n{}", body.trim());
                words = words.saturating_add(count_words(body));
                scenes = scenes.saturating_add(1);
            }
        }
    }
    CompiledExport {
        text,
        words,
        scenes,
    }
}

fn html_paragraphs(body: &str) -> String {
    let mut out = String::new();
    let mut current: Vec<String> = Vec::new();
    let flush = |current: &mut Vec<String>, out: &mut String| {
        if current.is_empty() {
            return;
        }
        out.push_str("    <p>");
        out.push_str(&current.join("<br>"));
        out.push_str("</p>\n");
        current.clear();
    };
    for line in body.split('\n') {
        if line.trim().is_empty() {
            flush(&mut current, &mut out);
        } else {
            current.push(esc_html(line.trim()));
        }
    }
    flush(&mut current, &mut out);
    out
}

fn compile_html(title: &str, sections: &[Section]) -> CompiledExport {
    let mut text = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>\nbody {{ font-family: Georgia, 'Times New Roman', serif; line-height: 1.7; max-width: 70ch; margin: 2rem auto; padding: 0 1rem; color: #222; background: #fff; }}\nh1, h2, h3 {{ line-height: 1.3; }}\n.separator {{ text-align: center; letter-spacing: 0.5em; margin: 2em 0; }}\n@media print {{\n  body {{ max-width: none; margin: 0; }}\n  h1, h2, h3 {{ break-after: avoid; }}\n  p {{ orphans: 3; widows: 3; }}\n}}\n</style>\n</head>\n<body>\n<h1>{}</h1>\n",
        esc_html(title),
        esc_html(title)
    );
    let mut words = 0usize;
    let mut scenes = 0usize;
    let mut first_scene = true;
    for section in sections {
        match section {
            Section::Act(name) => {
                let _ = writeln!(text, "<h2>{}</h2>", esc_html(name));
            }
            Section::Chapter(name) => {
                let _ = writeln!(text, "<h3>{}</h3>", esc_html(name));
            }
            Section::Scene(body) => {
                if first_scene {
                    first_scene = false;
                } else {
                    let _ = writeln!(text, "<p class=\"separator\">{SCENE_SEPARATOR}</p>");
                }
                text.push_str(&html_paragraphs(body));
                words = words.saturating_add(count_words(body));
                scenes = scenes.saturating_add(1);
            }
        }
    }
    text.push_str("</body>\n</html>\n");
    CompiledExport {
        text,
        words,
        scenes,
    }
}

/// Compile and write the manuscript.
///
/// `path`: explicit destination (relative joins `root`, parents created).
/// `None` writes `<root>/export/<title-slug>.<ext>` (dir created).
///
/// # Errors
/// `Io` naming the path plus the reason.
pub fn export_manuscript(
    manuscript: &Manuscript,
    scene_texts: &BTreeMap<NodeId, String>,
    root: &Path,
    format: ExportFormat,
    path: Option<&str>,
) -> Result<ExportSummary, ExportError> {
    let compiled = compile_manuscript(manuscript, scene_texts, format);
    let dest = match path {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(ExportError::Io("export path is empty".to_string()));
            }
            let candidate = PathBuf::from(trimmed);
            if candidate.is_absolute() {
                candidate
            } else {
                root.join(candidate)
            }
        }
        None => root.join(format!(
            "export/{}.{}",
            slugify(manuscript.title()),
            format.extension()
        )),
    };
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| ExportError::Io(format!("{}: {err}", dest.display())))?;
        }
    }
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, compiled.text.as_bytes())
        .map_err(|err| ExportError::Io(format!("{}: {err}", tmp.display())))?;
    let handle = std::fs::File::open(&tmp)
        .map_err(|err| ExportError::Io(format!("{}: {err}", tmp.display())))?;
    handle
        .sync_all()
        .map_err(|err| ExportError::Io(format!("{}: {err}", tmp.display())))?;
    drop(handle);
    std::fs::rename(&tmp, &dest)
        .map_err(|err| ExportError::Io(format!("{}: {err}", dest.display())))?;
    Ok(ExportSummary {
        path: dest.to_string_lossy().to_string(),
        words: compiled.words,
        scenes: compiled.scenes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> (Manuscript, BTreeMap<NodeId, String>) {
        let mut manuscript = Manuscript::new("Sample Novel");
        let act = manuscript.add_act("Act I").unwrap();
        let ch = manuscript.add_chapter(act, "Chapter 1").unwrap();
        let first = manuscript.add_scene(ch, "Gate").unwrap();
        let second = manuscript.add_scene(ch, "River").unwrap();
        let _unlinked = manuscript.add_scene(ch, "Draftless").unwrap();
        let mut texts = BTreeMap::new();
        texts.insert(first, "Mara walked home.".to_string());
        texts.insert(second, "Joren <waved> & smiled.".to_string());
        (manuscript, texts)
    }

    #[test]
    fn slugify_lowercases_and_dashes() {
        assert_eq!(slugify("Sample Novel!"), "sample-novel");
        assert_eq!(slugify("  "), "manuscript");
        assert_eq!(slugify("A  B"), "a-b");
    }

    #[test]
    fn markdown_orders_sections_and_skips_fileless_scenes() {
        let (manuscript, texts) = seed();
        let compiled = compile_manuscript(&manuscript, &texts, ExportFormat::Markdown);
        assert_eq!(compiled.scenes, 2);
        assert_eq!(compiled.words, 6);
        let text = compiled.text;
        assert!(text.starts_with("# Sample Novel\n"));
        let act_at = text.find("# Act I").unwrap();
        let ch_at = text.find("## Chapter 1").unwrap();
        let first_at = text.find("Mara walked home.").unwrap();
        let sep_at = text.find(SCENE_SEPARATOR).unwrap();
        let second_at = text.find("Joren <waved>").unwrap();
        assert!(act_at < ch_at && ch_at < first_at && first_at < sep_at && sep_at < second_at);
        assert!(!text.contains("Draftless"));
    }

    #[test]
    fn html_escapes_is_self_contained_and_counts_words() {
        let (manuscript, texts) = seed();
        let compiled = compile_manuscript(&manuscript, &texts, ExportFormat::Html);
        assert_eq!(compiled.scenes, 2);
        assert_eq!(compiled.words, 6);
        assert!(compiled.text.contains("<style>"));
        assert!(compiled.text.contains("@media print"));
        assert!(compiled.text.contains("<p class=\"separator\">* * *</p>"));
        assert!(compiled.text.contains("Joren &lt;waved&gt; &amp; smiled."));
        assert!(!compiled.text.contains("<waved>"));
    }

    #[test]
    fn export_writes_default_and_custom_paths() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("yonro-export-{n}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (manuscript, texts) = seed();
        let summary =
            export_manuscript(&manuscript, &texts, &root, ExportFormat::Markdown, None).unwrap();
        assert!(summary.path.ends_with("export/sample-novel.md"));
        assert_eq!((summary.words, summary.scenes), (6, 2));
        assert!(std::fs::read_to_string(&summary.path)
            .unwrap()
            .contains("# Act I"));
        let custom = export_manuscript(
            &manuscript,
            &texts,
            &root,
            ExportFormat::Html,
            Some("out/book.html"),
        )
        .unwrap();
        assert!(custom.path.ends_with("out/book.html"));
        assert_eq!(ExportFormat::parse("MD").unwrap(), ExportFormat::Markdown);
        assert!(ExportFormat::parse("pdf").is_err());
        assert!(export_manuscript(
            &manuscript,
            &texts,
            &root,
            ExportFormat::Markdown,
            Some("  ")
        )
        .is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
