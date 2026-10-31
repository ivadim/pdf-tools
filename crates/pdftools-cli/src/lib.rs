//! Every PDF Tools operation from the command line.
//!
//! This is also the WebAssembly module behind the web version (built for
//! `wasm32-wasip1`): the page writes the inputs into an in-memory file
//! system, runs a command and reads the outputs back. Results are printed as
//! one line of JSON on stdout; errors go to stderr with exit status 1. The
//! iOS app links this library and calls [`run`] directly.
//!
//! Options are `--name=value` (or a bare `--name` for switches), so values
//! may start with a dash. Page lists use the app's syntax: `1-3, 5, 8-`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pdftools_core::compress::{compress_document, Level};
use pdftools_core::grayscale::grayscale_document;
use pdftools_core::images::{ImageLayout, ImagePage, PageOrientation, PageSize};
use pdftools_core::organize::{split_file, SplitPart};
use pdftools_core::security::{protect_document, Protection};
use pdftools_core::stamps::{add_image_stamp, add_page_numbers, add_signature, ImageStamp, PageNumbers, Placement};
use pdftools_core::{Anchor, HAlign, Metadata, PageSpec, VAlign};
use pdftools_render::pages::{render, DocCache, PreviewRequest};
use pdftools_render::ranges::parse_list;

const USAGE: &str = "\
usage: pdftools-cli <command> [arguments] [--option=value ...]

  info        IN [--password=P] [--images]           --images also counts images
                                                       (reads the whole file)
  render      IN OUTDIR --width=PX [--pages=LIST]      writes OUTDIR/<n>.rgba
  merge       OUT IN... [--pages=SRC:PAGE:ROT,...]     SRC is 0-based
  split       IN OUTDIR --parts=LIST;LIST... [--name=BASE]
  organize    IN OUT --pages=PAGE:ROT,...              rotate and rearrange
  compress    IN OUT [--level=lossless|balanced|strong] [--keep=LIST]
  grayscale   IN OUT [--text-only] [--keep=LIST]
  protect     IN OUT --password=P [--owner=P] [--no-print] [--no-copy]
              [--no-modify] [--no-annotate] [--keep=LIST]
  unlock      IN OUT [--password=P]
  metadata    IN OUT [--title=] [--author=] [--subject=] [--keywords=]
              [--creator=] [--producer=] [--keep=LIST]
  numbers     IN OUT [--format='{n}'] [--position=bottom-center] [--margin=PT]
              [--size=PT] [--start=N] [--color=RRGGBB] [--pages=LIST] [--keep=LIST]
  stamp       IN OUT --image=PNG [--position=middle-center] [--width=FRACTION]
              [--angle=DEG] [--opacity=0..1] [--under] [--pages=LIST] [--keep=LIST]
  signature   IN OUT --image=PNG --place=PAGE:X:Y:W:H,...   fractions of the page
  images      OUT IMG[:ROT]... [--size=a4|letter|fit] [--orientation=auto|portrait|landscape]
              [--margin=PT]

--pages for numbers and stamp count pages after --keep has removed the rest.
Every command takes --threads=N: images and streams processed at once
(default 4; fewer saves memory).";

/// Positional arguments and `--name=value` options.
struct Args {
    positional: Vec<String>,
    options: HashMap<String, String>,
}

impl Args {
    fn parse(args: &[String]) -> Self {
        let mut positional = Vec::new();
        let mut options = HashMap::new();
        for arg in args {
            match arg.strip_prefix("--") {
                Some(option) => {
                    let (name, value) = option.split_once('=').unwrap_or((option, ""));
                    options.insert(name.to_owned(), value.to_owned());
                }
                None => positional.push(arg.clone()),
            }
        }
        Self { positional, options }
    }

    fn path(&self, index: usize, what: &str) -> Result<PathBuf, String> {
        self.positional.get(index).map(PathBuf::from).ok_or_else(|| format!("missing {what}\n\n{USAGE}"))
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }

    fn flag(&self, name: &str) -> bool {
        self.options.contains_key(name)
    }

    fn number<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.get(name) {
            None | Some("") => Ok(default),
            Some(value) => value.parse().map_err(|_| format!("--{name}: “{value}” isn’t a number")),
        }
    }

    /// The pages of `input` named by option `name`, if given.
    fn pages(&self, name: &str, total: impl FnOnce() -> Result<usize, String>) -> Result<Option<Vec<usize>>, String> {
        match self.get(name) {
            None | Some("") => Ok(None),
            Some(text) => parse_list(text, total()?).map(Some).map_err(|e| format!("--{name}: {e}")),
        }
    }
}

/// Runs one command (`args[0]`) and returns its result as one line of JSON,
/// or an error message. The iOS app calls this in-process.
pub fn run(args: &[String]) -> Result<String, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(USAGE.into());
    };
    let args = Args::parse(rest);
    if args.get("threads").is_some() {
        pdftools_core::set_max_threads(args.number("threads", pdftools_core::DEFAULT_THREADS)?);
    }
    match command.as_str() {
        "info" => info(&args),
        "render" => render_pages(&args),
        "merge" => merge(&args),
        "split" => split(&args),
        "organize" => organize(&args),
        "compress" => {
            let level = match args.get("level").unwrap_or("balanced") {
                "lossless" => Level::Lossless,
                "balanced" => Level::Balanced,
                "strong" => Level::Strong,
                other => return Err(format!("--level: unknown level “{other}”")),
            };
            let (input, output) = (args.path(0, "input")?, args.path(1, "output")?);
            let before = pdftools_core::file_size(&input);
            let images = edit(&args, |doc| Ok(compress_document(doc, level)))?;
            let after = pdftools_core::file_size(&output);
            Ok(format!(r#"{{"before":{before},"after":{after},"images":{images}}}"#))
        }
        "grayscale" => {
            let summary = edit(&args, |doc| Ok(grayscale_document(doc, !args.flag("text-only"))))?;
            Ok(format!(r#"{{"streams":{},"images":{},"skipped":{}}}"#, summary.streams, summary.images, summary.skipped))
        }
        "protect" => {
            let password = args.get("password").unwrap_or_default();
            if password.is_empty() {
                return Err("--password: enter a password".into());
            }
            let protection = Protection {
                owner_password: args.get("owner").filter(|p| !p.is_empty()).map(str::to_owned),
                allow_print: !args.flag("no-print"),
                allow_copy: !args.flag("no-copy"),
                allow_modify: !args.flag("no-modify"),
                allow_annotate: !args.flag("no-annotate"),
                ..Protection::with_password(password)
            };
            edit(&args, |doc| protect_document(doc, &protection))?;
            Ok("{}".into())
        }
        "unlock" => {
            let (input, output) = (args.path(0, "input")?, args.path(1, "output")?);
            pdftools_core::security::unlock_file(&input, args.get("password").unwrap_or_default(), &output).map_err(|e| e.to_string())?;
            Ok("{}".into())
        }
        "metadata" => {
            let text = |name: &str| args.get(name).map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
            let metadata = Metadata {
                title: text("title"),
                author: text("author"),
                subject: text("subject"),
                keywords: text("keywords"),
                creator: text("creator"),
                producer: text("producer"),
                created: None,
                modified: None,
            };
            edit(&args, |doc| pdftools_core::metadata::write(doc, &metadata))?;
            Ok("{}".into())
        }
        "numbers" => {
            let options = PageNumbers {
                anchor: anchor(args.get("position").unwrap_or("bottom-center"))?,
                margin: args.number("margin", 30.0)?,
                font_size: args.number("size", 11.0)?,
                format: args.get("format").filter(|f| !f.is_empty()).unwrap_or("{n}").to_owned(),
                start_at: args.number("start", 1)?,
                pages: stamped_pages(&args)?,
                color: color(args.get("color").unwrap_or("000000"))?,
            };
            let count = edit(&args, |doc| add_page_numbers(doc, &options))?;
            Ok(format!(r#"{{"pages":{count}}}"#))
        }
        "stamp" => {
            let stamp = ImageStamp {
                image: read_option_file(&args, "image")?,
                anchor: anchor(args.get("position").unwrap_or("middle-center"))?,
                margin: args.number("margin", 0.05)?,
                width: args.number("width", 0.6)?,
                rotation: args.number("angle", 0.0)?,
                opacity: args.number("opacity", 0.3)?,
                under: args.flag("under"),
                pages: stamped_pages(&args)?,
            };
            let count = edit(&args, |doc| add_image_stamp(doc, &stamp))?;
            Ok(format!(r#"{{"pages":{count}}}"#))
        }
        "signature" => {
            let image = read_option_file(&args, "image")?;
            let placements = args
                .get("place")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|item| {
                    let parts: Vec<&str> = item.trim().split(':').collect();
                    let number = |i: usize| parts.get(i).and_then(|v| v.parse::<f32>().ok());
                    match (parts.first().and_then(|p| p.parse().ok()), number(1), number(2), number(3), number(4)) {
                        (Some(page), Some(x), Some(y), Some(width), Some(height)) if parts.len() == 5 => {
                            Ok(Placement { page, x, y, width, height })
                        }
                        _ => Err(format!("--place: “{item}” isn’t PAGE:X:Y:W:H")),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let count = edit(&args, |doc| add_signature(doc, &image, &placements))?;
            Ok(format!(r#"{{"pages":{count}}}"#))
        }
        "images" => images(&args),
        "help" | "--help" | "-h" => Ok(json_string(USAGE)),
        other => Err(format!("unknown command “{other}”\n\n{USAGE}")),
    }
}

/// Loads IN (keeping only `--keep`), changes it and saves it to OUT.
fn edit<T>(args: &Args, change: impl FnOnce(&mut lopdf::Document) -> Result<T, pdftools_core::Error>) -> Result<T, String> {
    let (input, output) = (args.path(0, "input")?, args.path(1, "output")?);
    let keep = args.pages("keep", || page_count(&input))?;
    let mut doc = pdftools_core::load_pages(&input, keep.as_deref()).map_err(|e| e.to_string())?;
    let result = change(&mut doc).map_err(|e| e.to_string())?;
    pdftools_core::save(&mut doc, &output).map_err(|e| e.to_string())?;
    Ok(result)
}

/// `--pages` for the stamping tools, numbered after `--keep` is applied.
fn stamped_pages(args: &Args) -> Result<Option<Vec<usize>>, String> {
    let input = args.path(0, "input")?;
    args.pages("pages", || match args.pages("keep", || page_count(&input))? {
        Some(keep) => Ok(keep.len()),
        None => page_count(&input),
    })
}

fn page_count(path: &Path) -> Result<usize, String> {
    pdftools_core::page_count(path).map_err(|e| e.to_string())
}

fn read_option_file(args: &Args, name: &str) -> Result<Vec<u8>, String> {
    let path = args.get(name).filter(|p| !p.is_empty()).ok_or_else(|| format!("--{name}: choose a file"))?;
    std::fs::read(path).map_err(|e| format!("--{name}: {e}"))
}

fn info(args: &Args) -> Result<String, String> {
    let input = args.path(0, "input")?;
    if let Some(password) = args.get("password") {
        let doc = pdftools_core::security::load_with_password(&input, password).map_err(|e| e.to_string())?;
        return Ok(format!(r#"{{"pages":{},"needs_password":false,"encrypted":true}}"#, doc.get_pages().len()));
    }
    let counted = args.flag("images");
    let info = if counted { pdftools_core::inspect(&input) } else { pdftools_render::info::quick_inspect(&input) };
    let info = info.map_err(|e| e.to_string())?;
    let m = &info.metadata;
    let field = |v: &Option<String>| v.as_deref().map_or("null".to_owned(), json_string);
    Ok(format!(
        concat!(
            r#"{{"pages":{},"needs_password":{},"encrypted":{},"repaired":{},"file_size":{},"images":{},"image_bytes":{},"#,
            r#""metadata":{{"title":{},"author":{},"subject":{},"keywords":{},"creator":{},"producer":{}}}}}"#
        ),
        info.pages.map_or("null".to_owned(), |p| p.to_string()),
        info.needs_password,
        info.encrypted,
        info.repaired,
        info.file_size,
        // Only counted with --images.
        if counted { info.images.to_string() } else { "null".into() },
        if counted { info.image_bytes.to_string() } else { "null".into() },
        field(&m.title),
        field(&m.author),
        field(&m.subject),
        field(&m.keywords),
        field(&m.creator),
        field(&m.producer),
    ))
}

/// Writes each page as raw premultiplied RGBA (opaque: pages are drawn on
/// white), which the browser puts straight into an `ImageData`.
fn render_pages(args: &Args) -> Result<String, String> {
    let (input, dir) = (args.path(0, "input")?, args.path(1, "output folder")?);
    let width: u32 = args.number("width", 200)?;
    let pages = match args.pages("pages", || page_count(&input))? {
        Some(pages) => pages,
        None => (1..=page_count(&input)?).collect(),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let docs = DocCache::new();
    let mut out = Vec::new();
    for page in pages {
        let request = PreviewRequest { path: input.clone(), page, width, rotation: 0, gray: false };
        let rendered = render(&request, &docs)?;
        std::fs::write(dir.join(format!("{page}.rgba")), &rendered.pixels).map_err(|e| e.to_string())?;
        out.push(format!(
            r#"{{"page":{page},"width":{},"height":{},"points":[{},{}]}}"#,
            rendered.width, rendered.height, rendered.page_size.0, rendered.page_size.1
        ));
    }
    Ok(format!("[{}]", out.join(",")))
}

fn merge(args: &Args) -> Result<String, String> {
    let output = args.path(0, "output")?;
    let inputs: Vec<PathBuf> = args.positional.iter().skip(1).map(PathBuf::from).collect();
    let pages = match args.get("pages") {
        None | Some("") => None,
        Some(text) => Some(
            text.split(',')
                .map(|item| {
                    let parts: Vec<&str> = item.trim().split(':').collect();
                    let n = |i: usize| parts.get(i).and_then(|v| v.parse::<i64>().ok());
                    match (n(0), n(1), n(2).or(if parts.len() == 2 { Some(0) } else { None })) {
                        (Some(s), Some(p), Some(r)) if s >= 0 && p >= 1 => Ok(PageSpec::new(s as usize, p as usize).rotated(r)),
                        _ => Err(format!("--pages: “{item}” isn’t SRC:PAGE:ROT")),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    let summary = pdftools_core::merge_files(&inputs, pages.as_deref(), &output, |_| {}).map_err(|e| e.to_string())?;
    Ok(format!(r#"{{"files":{},"pages":{}}}"#, summary.files, summary.pages))
}

fn split(args: &Args) -> Result<String, String> {
    let (input, dir) = (args.path(0, "input")?, args.path(1, "output folder")?);
    let total = page_count(&input)?;
    let base = args.get("name").filter(|n| !n.trim().is_empty()).unwrap_or("part").trim();
    let groups: Vec<&str> = args.get("parts").unwrap_or_default().split(';').filter(|g| !g.trim().is_empty()).collect();
    if groups.is_empty() {
        return Err("--parts: enter page ranges, e.g. 1-3;4-".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        let pages = pdftools_render::ranges::parse_groups(group, total)?.into_iter().flatten().map(|p| PageSpec::new(0, p)).collect();
        parts.push(SplitPart { pages, output: dir.join(pdftools_core::split_output_filename(base, i + 1, groups.len())) });
    }
    let count = split_file(&input, &parts, |_| {}).map_err(|e| e.to_string())?;
    let files: Vec<String> = parts.iter().map(|p| json_string(&p.output.to_string_lossy())).collect();
    Ok(format!(r#"{{"count":{count},"files":[{}]}}"#, files.join(",")))
}

fn organize(args: &Args) -> Result<String, String> {
    let (input, output) = (args.path(0, "input")?, args.path(1, "output")?);
    let pages = args
        .get("pages")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|item| {
            let (page, rotation) = item.trim().split_once(':').unwrap_or((item.trim(), "0"));
            match (page.parse::<usize>(), rotation.parse::<i64>()) {
                (Ok(page), Ok(rotation)) if page >= 1 => Ok(PageSpec::new(0, page).rotated(rotation)),
                _ => Err(format!("--pages: “{item}” isn’t PAGE:ROT")),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = pdftools_core::organize::organize_file(&input, &pages, &output).map_err(|e| e.to_string())?;
    Ok(format!(r#"{{"pages":{count}}}"#))
}

fn images(args: &Args) -> Result<String, String> {
    let output = args.path(0, "output")?;
    let pages = args
        .positional
        .iter()
        .skip(1)
        .map(|item| match item.rsplit_once(':') {
            Some((path, rotation)) if rotation.parse::<i32>().is_ok() => ImagePage::new(path).rotated(rotation.parse().unwrap()),
            _ => ImagePage::new(item),
        })
        .collect::<Vec<_>>();
    let layout = ImageLayout {
        page_size: match args.get("size").unwrap_or("a4") {
            "a4" => PageSize::A4,
            "letter" => PageSize::Letter,
            "fit" => PageSize::FitImage,
            other => return Err(format!("--size: unknown size “{other}”")),
        },
        orientation: match args.get("orientation").unwrap_or("auto") {
            "auto" => PageOrientation::Auto,
            "portrait" => PageOrientation::Portrait,
            "landscape" => PageOrientation::Landscape,
            other => return Err(format!("--orientation: unknown orientation “{other}”")),
        },
        margin: args.number("margin", 0.0)?,
    };
    let count = pdftools_core::images::images_to_pdf_file(&pages, &layout, &output, |_| {}).map_err(|e| e.to_string())?;
    Ok(format!(r#"{{"pages":{count}}}"#))
}

/// "bottom-center", "top-left", "middle-right"…
fn anchor(text: &str) -> Result<Anchor, String> {
    let (v, h) = text.split_once('-').ok_or_else(|| format!("--position: “{text}” isn’t like bottom-center"))?;
    let v = match v {
        "top" => VAlign::Top,
        "middle" => VAlign::Middle,
        "bottom" => VAlign::Bottom,
        _ => return Err(format!("--position: “{text}” isn’t like bottom-center")),
    };
    let h = match h {
        "left" => HAlign::Left,
        "center" => HAlign::Center,
        "right" => HAlign::Right,
        _ => return Err(format!("--position: “{text}” isn’t like bottom-center")),
    };
    Ok(Anchor::new(h, v))
}

fn color(text: &str) -> Result<[f32; 3], String> {
    let hex = text.trim_start_matches('#');
    let channel = |i: usize| hex.get(i..i + 2).and_then(|c| u8::from_str_radix(c, 16).ok()).map(|c| f32::from(c) / 255.0);
    match (hex.len(), channel(0), channel(2), channel(4)) {
        (6, Some(r), Some(g), Some(b)) => Ok([r, g, b]),
        _ => Err(format!("--color: “{text}” isn’t RRGGBB")),
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options_and_positions() {
        let args = Args::parse(&["a.pdf".into(), "--format=-{n}-".into(), "--under".into(), "b.pdf".into()]);
        assert_eq!(args.positional, ["a.pdf", "b.pdf"]);
        assert_eq!(args.get("format"), Some("-{n}-"));
        assert!(args.flag("under") && !args.flag("over"));
        assert_eq!(anchor("top-left").unwrap(), Anchor::new(HAlign::Left, VAlign::Top));
        assert!(anchor("left").is_err());
        assert_eq!(color("#ff0000").unwrap(), [1.0, 0.0, 0.0]);
        assert_eq!(json_string("a\"b\n"), r#""a\"b\n""#);
    }
}
