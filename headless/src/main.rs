//! `ezpzxdw-headless`: the runner structural-mcp starts for DocuWorks files (protocol v1).
//!
//! ```text
//! ezpzxdw-headless inspect --input <file> --output-dir <dir>
//! ezpzxdw-headless text    --input <file> --output-dir <dir> [--include-hidden true|false]
//! ezpzxdw-headless png     --input <file> --output-dir <dir> --pages <3|2-5|1,4|all> [--dpi <30-300>]
//! ```
//!
//! stdout is one JSON envelope (protocolVersion 1); the exit code is 0 when `ok` is true and 2 otherwise. The input is
//! only read; the artifacts (`document.json`, or `text.txt` and `text.json`) are written into the output folder, which
//! must exist (`png` writes `page-0001.png`, ... and `pages.json`; text is drawn with the fonts found beside the runner,
//! in `EZPZXDW_FONTS` and on the PC). Nothing is decided about a design, so `engineeringStatus` is always `not_evaluated`.

mod sha256;

use ezpzxdw_core::doc::Document;
use ezpzxdw_core::error::Error as XdwError;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const ENGINE: &str = "ezpzxdw";
/// The engine's version (engine/Cargo.toml's workspace version); a test checks that the two agree.
const ENGINE_VERSION: &str = "0.3.0";
const READINESS: &str = "experimental";

/// What went wrong, as protocol v1's `errors[]` says it.
#[derive(Debug)]
struct Failure {
    category: &'static str,
    code: &'static str,
    message: String,
}

fn failure(category: &'static str, code: &'static str, message: impl Into<String>) -> Failure {
    Failure { category, code, message: message.into() }
}

#[derive(Debug, PartialEq)]
enum Command {
    Inspect,
    Text,
    Png,
}

#[derive(Debug)]
struct Request {
    command: Command,
    input: PathBuf,
    output_dir: PathBuf,
    include_hidden: bool,
    pages: Option<String>,
    dpi: f32,
}

/// Most pages in one `png` call, and the dpi range (a page is about 1 s at 100 dpi).
const MAX_PNG_PAGES: usize = 50;
const MIN_DPI: f32 = 30.0;
const MAX_DPI: f32 = 300.0;
const DEFAULT_DPI: f32 = 100.0;

/// "3", "2-5", "1,4,7-8" or "all" as 0-based page numbers, in order, without repeats.
fn page_list(spec: &str, count: usize) -> Result<Vec<usize>, String> {
    let bad = || format!("--pages は 3・2-5・1,4,7・all のどれかの形です: {spec}");
    let mut out: Vec<usize> = Vec::new();
    let ranges: Vec<(usize, usize)> = if spec == "all" {
        vec![(1, count)]
    } else {
        let mut v = Vec::new();
        for part in spec.split(',') {
            let (a, b) = match part.split_once('-') {
                Some((a, b)) => (a.parse::<usize>().map_err(|_| bad())?, b.parse::<usize>().map_err(|_| bad())?),
                None => {
                    let n = part.parse::<usize>().map_err(|_| bad())?;
                    (n, n)
                }
            };
            if a == 0 || b < a {
                return Err(bad());
            }
            v.push((a, b));
        }
        v
    };
    for (a, b) in ranges {
        if b > count {
            return Err(format!("ページ {b} はありません（全 {count} ページ）"));
        }
        for p in a..=b {
            if !out.contains(&(p - 1)) {
                out.push(p - 1);
            }
        }
    }
    if out.is_empty() {
        return Err("ページがありません".into());
    }
    Ok(out)
}

fn parse(args: &[String]) -> Result<Request, Failure> {
    let request = |m: String| failure("request", "INVALID_ARGUMENT", m);
    let command = match args.first().map(String::as_str) {
        Some("inspect") => Command::Inspect,
        Some("text") => Command::Text,
        Some("png") => Command::Png,
        Some(other) => return Err(request(format!("知らないコマンドです: {other}（inspect・text・png）"))),
        None => return Err(request("コマンドがありません（inspect・text・png）".into())),
    };
    let (mut input, mut output_dir, mut include_hidden, mut pages, mut dpi) = (None, None, None, None, None);
    let mut rest = args[1..].iter();
    while let Some(name) = rest.next() {
        let value = rest.next().ok_or_else(|| request(format!("{name} に値がありません")))?;
        let slot = match (name.as_str(), &command) {
            ("--input", _) => &mut input,
            ("--output-dir", _) => &mut output_dir,
            ("--include-hidden", Command::Text) => &mut include_hidden,
            ("--pages", Command::Png) => &mut pages,
            ("--dpi", Command::Png) => &mut dpi,
            _ => return Err(request(format!("知らない引数です: {name}"))),
        };
        if slot.replace(value.clone()).is_some() {
            return Err(request(format!("{name} が 2 回あります")));
        }
    }
    let include_hidden = match include_hidden.as_deref() {
        None | Some("false") => false,
        Some("true") => true,
        Some(other) => return Err(request(format!("--include-hidden は true か false です: {other}"))),
    };
    if command == Command::Png && pages.is_none() {
        return Err(request("--pages がありません（3・2-5・1,4,7・all）".into()));
    }
    let dpi = match dpi.as_deref() {
        None => DEFAULT_DPI,
        Some(v) => match v.parse::<f32>() {
            Ok(n) if (MIN_DPI..=MAX_DPI).contains(&n) => n,
            _ => return Err(request(format!("--dpi は {MIN_DPI}〜{MAX_DPI} の数です: {v}"))),
        },
    };
    Ok(Request {
        command,
        input: input.ok_or_else(|| request("--input がありません".into()))?.into(),
        output_dir: output_dir.ok_or_else(|| request("--output-dir がありません".into()))?.into(),
        include_hidden,
        pages,
        dpi,
    })
}

fn open(path: &Path) -> Result<Document, Failure> {
    let bytes = std::fs::read(path).map_err(|e| failure("io", "IO_ERROR", format!("入力を読めません: {e}")))?;
    Document::open(bytes).map_err(|e| {
        let (code, text) = match &e {
            XdwError::NotXdw(_) => ("NOT_XDW", "DocuWorks の文書（.xdw）でもバインダー（.xbd）でもありません"),
            XdwError::Corrupt(_) => ("DAMAGED", "ファイルが壊れています"),
            XdwError::Unsupported(_) => ("UNSUPPORTED", "このエンジンがまだ扱えない作りの文書です"),
            XdwError::Protected => (
                "PROTECTED",
                "パスワードか電子証明書で保護された文書です。DocuWorks でセキュリティを解除してから渡してください",
            ),
            XdwError::TooLarge(_) => ("TOO_LARGE", "大きすぎるか壊れています（エンジンの上限を超えました）"),
        };
        failure("input", code, format!("{text}（{e}）"))
    })
}

/// 1/100 mm to mm.
fn mm(v: i64) -> f64 {
    v as f64 / 100.0
}

/// The binder's documents as (1-based index, name, first page, page count); empty for a single document.
fn documents(d: &Document) -> Vec<(usize, String, usize, usize)> {
    d.binder_docs().into_iter().enumerate().map(|(k, b)| (k + 1, b.name, b.first_page, b.pages)).collect()
}

/// The binder document (1-based) holding page `k` (0-based), or None for a single document.
fn document_of(docs: &[(usize, String, usize, usize)], k: usize) -> Option<usize> {
    docs.iter().find(|(_, _, first, count)| (*first..first + count).contains(&k)).map(|(index, ..)| *index)
}

fn kind(d: &Document) -> &'static str {
    if d.is_binder() {
        "binder"
    } else {
        "document"
    }
}

/// Writes one artifact and returns its protocol v1 record.
fn artifact(dir: &Path, name: &str, kind: &str, media_type: &str, data: &[u8]) -> Result<Value, Failure> {
    std::fs::write(dir.join(name), data).map_err(|e| failure("io", "IO_ERROR", format!("{name} を書けません: {e}")))?;
    Ok(json!({ "kind": kind, "mediaType": media_type, "relativePath": name, "bytes": data.len(), "sha256": sha256::hex(data) }))
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut s = serde_json::to_string_pretty(v).expect("a JSON value always serializes");
    s.push('\n');
    s.into_bytes()
}

struct Outcome {
    summary: Value,
    messages: Vec<Value>,
    artifacts: Vec<Value>,
}

fn inspect(d: &Document, dir: &Path) -> Result<Outcome, Failure> {
    let docs = documents(d);
    let (mut annotations, mut signatures) = (0usize, 0usize);
    let pages: Vec<Value> = d
        .pages
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let objects: Vec<Value> = p
                .objects
                .iter()
                .enumerate()
                .map(|(oi, o)| {
                    if o.kind_name != "page" {
                        annotations += 1;
                    }
                    let signature = d.signature_of(k, oi).map(|s| {
                        signatures += 1;
                        json!({ "type": if s.stamp { "stamp" } else { "certificate" }, "module": s.module, "version": s.version })
                    });
                    json!({
                        "index": oi, "kind": o.kind_name, "kindCode": o.kind,
                        "xMm": mm(o.x), "yMm": mm(o.y), "widthMm": mm(o.w), "heightMm": mm(o.h), "rotation": o.rotation,
                        "text": o.text, "signature": signature,
                    })
                })
                .collect();
            json!({ "page": k + 1, "document": document_of(&docs, k), "widthMm": mm(p.w), "heightMm": mm(p.h), "objects": objects })
        })
        .collect();
    let report = json!({
        "schemaVersion": 1,
        "kind": kind(d),
        "binderName": d.binder_name(),
        "generation": d.container.generation,
        "pageCount": d.pages.len(),
        "documents": docs.iter().map(|(index, name, first, count)| json!({
            "index": index, "name": name, "firstPage": first + 1, "lastPage": first + count, "pageCount": count,
        })).collect::<Vec<_>>(),
        "pages": pages,
    });
    let mut messages = Vec::new();
    if signatures > 0 {
        messages.push(json!({ "level": "info", "code": "SIGNED", "text": format!("署名が {signatures} 個あります（署名の有効性は確かめていません）"), "path": null }));
    }
    Ok(Outcome {
        summary: json!({
            "command": "inspect", "kind": kind(d), "binderName": d.binder_name(), "pageCount": d.pages.len(),
            "documentCount": docs.len(), "annotationCount": annotations, "signatureCount": signatures,
        }),
        messages,
        artifacts: vec![artifact(dir, "document.json", "document", "application/json", &pretty(&report))?],
    })
}

fn text(mut d: Document, dir: &Path, include_hidden: bool) -> Result<Outcome, Failure> {
    d.include_hidden = include_hidden;
    let docs = documents(&d);
    let (mut plain, mut pages, mut messages) = (String::new(), Vec::new(), Vec::new());
    let (mut characters, mut hidden) = (0usize, 0usize);
    let (mut unreadable, mut empty) = (Vec::new(), Vec::new());
    for k in 0..d.pages.len() {
        if let Some((index, name, ..)) = docs.iter().find(|(_, _, first, _)| *first == k) {
            plain.push_str(&format!("=== document {index} {name}\n"));
        }
        let document = document_of(&docs, k);
        match d.render(k) {
            Ok(display) => {
                let page_text = display.text();
                let count = page_text.chars().filter(|c| !c.is_whitespace()).count();
                characters += count;
                hidden += display.hidden_chars;
                if count == 0 {
                    empty.push(k + 1);
                }
                plain.push_str(&format!("--- page {}\n{page_text}\n", k + 1));
                pages.push(json!({ "page": k + 1, "document": document, "readable": true, "text": page_text,
                    "characters": count, "hiddenChars": display.hidden_chars, "error": null }));
            }
            Err(e) => {
                unreadable.push(k + 1);
                plain.push_str(&format!("--- page {} (not readable: {e})\n", k + 1));
                messages.push(json!({ "level": "warning", "code": "PAGE_UNREADABLE",
                    "text": format!("{} ページ目を描けず、文字を取り出せませんでした（{e}）", k + 1), "path": format!("pages[{k}]") }));
                pages.push(json!({ "page": k + 1, "document": document, "readable": false, "text": "",
                    "characters": 0, "hiddenChars": 0, "error": e.to_string() }));
            }
        }
    }
    if hidden > 0 {
        messages.push(json!({ "level": "info", "code": "HIDDEN_TEXT_LEFT_OUT", "path": null,
            "text": format!("あとから描かれた四角形や画像で覆われた {hidden} 文字を外しました（includeHidden で残せます）") }));
    }
    if !empty.is_empty() {
        messages.push(json!({ "level": "info", "code": "NO_TEXT_PAGES", "path": null,
            "text": format!("文字のないページが {} ページあります（スキャンした画像だけのページかもしれません）", empty.len()) }));
    }
    let report = json!({ "schemaVersion": 1, "kind": kind(&d), "includeHidden": include_hidden, "pageCount": d.pages.len(), "pages": pages });
    Ok(Outcome {
        summary: json!({
            "command": "text", "kind": kind(&d), "pageCount": d.pages.len(), "documentCount": docs.len(),
            "characters": characters, "hiddenChars": hidden, "includeHidden": include_hidden,
            "pagesWithoutText": empty, "unreadablePages": unreadable,
        }),
        messages,
        artifacts: vec![
            artifact(dir, "text.txt", "text", "text/plain; charset=utf-8", plain.as_bytes())?,
            artifact(dir, "text.json", "text-pages", "application/json", &pretty(&report))?,
        ],
    })
}

/// Fonts for the pages: the `fonts` folder beside the runner first (what the installer ships), then `EZPZXDW_FONTS` and the PC's.
fn fonts() -> ezpzxdw_raster::Fonts {
    let mut f = ezpzxdw_raster::Fonts::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("fonts"))) {
        f.add_dir(&dir);
    }
    f.add_system();
    f
}

fn png(d: &Document, dir: &Path, pages: &str, dpi: f32) -> Result<Outcome, Failure> {
    let list = page_list(pages, d.pages.len()).map_err(|m| failure("request", "INVALID_ARGUMENT", m))?;
    if list.len() > MAX_PNG_PAGES {
        return Err(failure("request", "TOO_MANY_PAGES", format!("1 回に画像にできるのは {MAX_PNG_PAGES} ページまでです（{} ページ指定されました）", list.len())));
    }
    let fonts = fonts();
    let (mut records, mut artifacts, mut messages) = (Vec::new(), Vec::new(), Vec::new());
    let (mut text_runs, mut missing, mut unreadable) = (0usize, 0usize, Vec::new());
    for k in list {
        let rendered = d.render(k).map_err(|e| e.to_string()).and_then(|display| {
            let r = ezpzxdw_raster::render_with(&display, dpi, Some(&fonts))?;
            let data = r.png()?;
            Ok((display, r, data))
        });
        match rendered {
            Ok((display, r, data)) => {
                let name = format!("page-{:04}.png", k + 1);
                artifacts.push(artifact(dir, &name, "page-image", "image/png", &data)?);
                let runs = display.items.iter().filter(|i| matches!(i, ezpzxdw_core::gfx::Item::Text { .. })).count();
                text_runs += r.skipped_text;
                missing += r.missing_glyphs;
                records.push(json!({ "page": k + 1, "file": name, "widthPx": r.pixmap.width(), "heightPx": r.pixmap.height(),
                    "textRuns": runs, "textNotDrawn": r.skipped_text, "missingGlyphs": r.missing_glyphs,
                    "picturesNotDecoded": r.skipped_images, "skipped": display.skipped }));
            }
            Err(e) => {
                unreadable.push(k + 1);
                messages.push(json!({ "level": "warning", "code": "PAGE_UNREADABLE",
                    "text": format!("{} ページ目を画像にできませんでした（{e}）", k + 1), "path": format!("pages[{k}]") }));
                records.push(json!({ "page": k + 1, "file": null, "error": e }));
            }
        }
    }
    if text_runs > 0 {
        messages.push(json!({ "level": "warning", "code": "NO_FONTS", "path": null,
            "text": format!("フォントが見つからず、文字 {text_runs} 個所を描けませんでした（文字は xdw_text で読めます）") }));
    }
    if missing > 0 {
        messages.push(json!({ "level": "warning", "code": "MISSING_GLYPHS", "path": null,
            "text": format!("どのフォントにも字形がなく、空白にした文字が {missing} 字あります") }));
    }
    let report = json!({ "schemaVersion": 1, "kind": kind(d), "dpi": dpi, "pageCount": d.pages.len(), "images": records });
    let images = artifacts.len();
    artifacts.push(artifact(dir, "pages.json", "page-images", "application/json", &pretty(&report))?);
    Ok(Outcome {
        summary: json!({ "command": "png", "kind": kind(d), "pageCount": d.pages.len(), "dpi": dpi,
            "imageCount": images, "unreadablePages": unreadable, "textNotDrawn": text_runs, "missingGlyphs": missing }),
        messages,
        artifacts,
    })
}

fn run(args: &[String]) -> Result<Outcome, Failure> {
    let request = parse(args)?;
    if !request.output_dir.is_dir() {
        return Err(failure("request", "INVALID_ARGUMENT", "--output-dir のフォルダーがありません"));
    }
    let d = open(&request.input)?;
    match request.command {
        Command::Inspect => inspect(&d, &request.output_dir),
        Command::Text => text(d, &request.output_dir, request.include_hidden),
        Command::Png => png(&d, &request.output_dir, request.pages.as_deref().unwrap_or("1"), request.dpi),
    }
}

fn envelope(result: Result<Outcome, Failure>) -> (bool, Value) {
    let (ok, summary, messages, artifacts, errors) = match result {
        Ok(o) => (true, o.summary, o.messages, o.artifacts, vec![]),
        Err(f) => (
            false,
            Value::Null,
            vec![json!({ "level": "error", "code": f.code, "text": f.message, "path": null })],
            vec![],
            vec![json!({ "category": f.category, "code": f.code, "message": f.message })],
        ),
    };
    let value = json!({
        "protocolVersion": 1, "engine": ENGINE, "engineVersion": ENGINE_VERSION, "readiness": READINESS, "ok": ok,
        "executionStatus": if ok { "success" } else { "failed" }, "engineeringStatus": "not_evaluated",
        "summary": summary, "messages": messages, "artifacts": artifacts, "errors": errors,
    });
    (ok, value)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (ok, value) = envelope(run(&args));
    println!("{value}");
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn engine_version_is_the_engines() {
        let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../engine/Cargo.toml")).unwrap();
        let version = manifest.lines().find_map(|l| l.strip_prefix("version = \"")).and_then(|l| l.strip_suffix('"'));
        assert_eq!(version, Some(ENGINE_VERSION));
    }

    #[test]
    fn parses_the_commands() {
        let r = parse(&args(&["text", "--input", "a.xdw", "--output-dir", "o", "--include-hidden", "true"])).unwrap();
        assert_eq!((r.command, r.include_hidden), (Command::Text, true));
        let r = parse(&args(&["inspect", "--output-dir", "o", "--input", "a.xdw"])).unwrap();
        assert_eq!((r.command, r.include_hidden), (Command::Inspect, false));
    }

    #[test]
    fn lists_pages() {
        assert_eq!(page_list("all", 3).unwrap(), vec![0, 1, 2]);
        assert_eq!(page_list("2-3,1,2", 5).unwrap(), vec![1, 2, 0]);
        for bad in ["0", "4", "3-2", "a", "", "1,,2", "1-9"] {
            assert!(page_list(bad, 3).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_bad_arguments() {
        for bad in [
            &["render", "--input", "a", "--output-dir", "o"][..],
            &[],
            &["inspect", "--input", "a"],
            &["inspect", "--input", "a", "--output-dir", "o", "--include-hidden", "true"],
            &["text", "--input", "a", "--output-dir", "o", "--include-hidden", "yes"],
            &["text", "--input", "a", "--input", "b", "--output-dir", "o"],
            &["text", "--input"],
            &["png", "--input", "a", "--output-dir", "o"],
            &["png", "--input", "a", "--output-dir", "o", "--pages", "1", "--dpi", "10"],
            &["png", "--input", "a", "--output-dir", "o", "--pages", "1", "--include-hidden", "true"],
        ] {
            let f = parse(&args(bad)).unwrap_err();
            assert_eq!((f.category, f.code), ("request", "INVALID_ARGUMENT"), "{bad:?}");
        }
    }
}
