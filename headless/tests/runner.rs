//! The runner as structural-mcp starts it: a real .xdw made with the engine, the envelope on stdout, the exit code and
//! the artifacts with their sizes and SHA-256.

use ezpzxdw_core::doc::Document;
use ezpzxdw_core::edit::Shape;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn work(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ezpzxdw-headless-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("output")).unwrap();
    dir
}

/// Two A4 pages: a text annotation on the first, the second blank; with `cover`, a black box over the text.
fn sample(dir: &Path, cover: bool) -> PathBuf {
    let mut d = Document::blank(21000.0, 29700.0).unwrap();
    let text = Shape::Text { text: "設計条件 P7橋脚".into(), size: 12.0, color: 0, bold: false, background: None, frame: None };
    d.add_annotation(0, 2000.0, 3000.0, 0.0, 0.0, &text).unwrap();
    if cover {
        let o = d.pages[0].objects[0].clone();
        let shape = Shape::Rect { stroke: None, width: 1.0, fill: Some(0), highlight: false };
        d.add_annotation(0, o.x as f64 - 100.0, o.y as f64 - 100.0, o.w as f64 + 200.0, o.h as f64 + 200.0, &shape).unwrap();
    }
    d.insert_blank_page(1, 21000.0, 29700.0).unwrap();
    let path = dir.join("model.xdw");
    std::fs::write(&path, d.save().unwrap()).unwrap();
    path
}

fn run(args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_ezpzxdw-headless")).args(args).output().unwrap();
    let value: Value = serde_json::from_slice(&out.stdout).expect("stdout is one JSON envelope");
    (out.status.code().unwrap(), value)
}

fn sha256(path: &Path) -> String {
    let out = Command::new(if cfg!(windows) { "certutil" } else { "shasum" })
        .args(if cfg!(windows) { vec!["-hashfile", path.to_str().unwrap(), "SHA256"] } else { vec!["-a", "256", path.to_str().unwrap()] })
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = if cfg!(windows) { text.lines().nth(1).unwrap().replace(' ', "") } else { text.split_whitespace().next().unwrap().to_string() };
    line.to_lowercase()
}

fn check_envelope(v: &Value, ok: bool) {
    assert_eq!(v["protocolVersion"], 1);
    assert_eq!(v["engine"], "ezpzxdw");
    assert_eq!(v["readiness"], "experimental");
    assert_eq!(v["ok"], ok);
    assert_eq!(v["executionStatus"], if ok { "success" } else { "failed" });
    assert_eq!(v["engineeringStatus"], "not_evaluated");
}

/// Every artifact exists in `output` with the bytes and SHA-256 the envelope declares.
fn check_artifacts(v: &Value, output: &Path) -> Vec<String> {
    v["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            let path = output.join(a["relativePath"].as_str().unwrap());
            assert_eq!(std::fs::metadata(&path).unwrap().len(), a["bytes"].as_u64().unwrap());
            assert_eq!(sha256(&path), a["sha256"].as_str().unwrap());
            a["kind"].as_str().unwrap().to_string()
        })
        .collect()
}

#[test]
fn inspect_reports_pages_and_annotations() {
    let dir = work("inspect");
    let input = sample(&dir, false);
    let output = dir.join("output");
    let (code, v) = run(&["inspect", "--input", input.to_str().unwrap(), "--output-dir", output.to_str().unwrap()]);
    assert_eq!(code, 0, "{v}");
    check_envelope(&v, true);
    assert_eq!(check_artifacts(&v, &output), ["document"]);
    assert_eq!(v["summary"]["kind"], "document");
    assert_eq!(v["summary"]["pageCount"], 2);
    assert_eq!(v["summary"]["annotationCount"], 1);
    assert_eq!(v["summary"]["signatureCount"], 0);
    let report: Value = serde_json::from_slice(&std::fs::read(output.join("document.json")).unwrap()).unwrap();
    assert_eq!(report["pages"][0]["widthMm"], 210.0);
    assert_eq!(report["pages"][0]["heightMm"], 297.0);
    let text = report["pages"][0]["objects"].as_array().unwrap().iter().find(|o| o["kind"] == "text").unwrap();
    assert_eq!(text["text"], "設計条件 P7橋脚");
    assert_eq!(text["xMm"], 20.0);
    assert_eq!(report["documents"], Value::Array(vec![]));
}

#[test]
fn text_leaves_out_covered_text_unless_asked() {
    let dir = work("text");
    let input = sample(&dir, true);
    let output = dir.join("output");
    let (code, v) = run(&["text", "--input", input.to_str().unwrap(), "--output-dir", output.to_str().unwrap()]);
    assert_eq!(code, 0, "{v}");
    check_envelope(&v, true);
    assert_eq!(check_artifacts(&v, &output), ["text", "text-pages"]);
    assert_eq!(v["summary"]["characters"], 0);
    assert!(v["summary"]["hiddenChars"].as_u64().unwrap() > 0);
    assert_eq!(v["summary"]["pagesWithoutText"], serde_json::json!([1, 2]));
    let codes: Vec<&str> = v["messages"].as_array().unwrap().iter().map(|m| m["code"].as_str().unwrap()).collect();
    assert_eq!(codes, ["HIDDEN_TEXT_LEFT_OUT", "NO_TEXT_PAGES"]);

    let kept = dir.join("kept");
    std::fs::create_dir_all(&kept).unwrap();
    let (code, v) = run(&["text", "--input", input.to_str().unwrap(), "--output-dir", kept.to_str().unwrap(), "--include-hidden", "true"]);
    assert_eq!(code, 0, "{v}");
    check_artifacts(&v, &kept);
    assert_eq!(v["summary"]["includeHidden"], true);
    assert_eq!(v["summary"]["pagesWithoutText"], serde_json::json!([2]));
    let plain = std::fs::read_to_string(kept.join("text.txt")).unwrap();
    assert!(plain.starts_with("--- page 1\n"), "{plain}");
    assert!(plain.contains("設計条件"), "{plain}");
    assert!(plain.contains("--- page 2\n"), "{plain}");
    let pages: Value = serde_json::from_slice(&std::fs::read(kept.join("text.json")).unwrap()).unwrap();
    assert_eq!(pages["pages"][0]["readable"], true);
    assert!(pages["pages"][0]["text"].as_str().unwrap().contains("P7橋脚"));
}

#[test]
fn a_file_that_is_not_docuworks_fails_without_artifacts() {
    let dir = work("not-xdw");
    let input = dir.join("model.xdw");
    std::fs::write(&input, b"{\"not\": \"a DocuWorks file\"}").unwrap();
    let output = dir.join("output");
    let (code, v) = run(&["text", "--input", input.to_str().unwrap(), "--output-dir", output.to_str().unwrap()]);
    assert_eq!(code, 2);
    check_envelope(&v, false);
    assert_eq!(v["errors"][0]["category"], "input");
    assert_eq!(v["errors"][0]["code"], "NOT_XDW");
    assert_eq!(v["artifacts"], Value::Array(vec![]));
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
}

#[test]
fn a_bad_request_is_an_envelope_too() {
    let (code, v) = run(&["render"]);
    assert_eq!(code, 2);
    check_envelope(&v, false);
    assert_eq!(v["errors"][0]["code"], "INVALID_ARGUMENT");
    let dir = work("missing-output");
    let input = sample(&dir, false);
    let (code, v) = run(&["inspect", "--input", input.to_str().unwrap(), "--output-dir", dir.join("nowhere").to_str().unwrap()]);
    assert_eq!(code, 2);
    assert_eq!(v["errors"][0]["code"], "INVALID_ARGUMENT");
    let (code, v) = run(&["inspect", "--input", dir.join("missing.xdw").to_str().unwrap(), "--output-dir", dir.join("output").to_str().unwrap()]);
    assert_eq!(code, 2);
    assert_eq!(v["errors"][0]["code"], "IO_ERROR");
}

#[test]
fn png_draws_the_pages_asked_for() {
    let dir = work("png");
    let input = sample(&dir, false);
    let output = dir.join("output");
    let (code, v) = run(&["png", "--input", input.to_str().unwrap(), "--output-dir", output.to_str().unwrap(), "--pages", "2,1", "--dpi", "50"]);
    assert_eq!(code, 0, "{v}");
    check_envelope(&v, true);
    assert_eq!(check_artifacts(&v, &output), ["page-image", "page-image", "page-images"]);
    assert_eq!(v["summary"]["imageCount"], 2);
    assert_eq!(v["summary"]["dpi"], 50.0);
    for name in ["page-0001.png", "page-0002.png"] {
        assert_eq!(&std::fs::read(output.join(name)).unwrap()[..8], b"\x89PNG\r\n\x1a\n", "{name}");
    }
    let report: Value = serde_json::from_slice(&std::fs::read(output.join("pages.json")).unwrap()).unwrap();
    assert_eq!(report["images"][0]["page"], 2); // in the order asked for
    assert_eq!((report["images"][1]["widthPx"].as_u64(), report["images"][1]["heightPx"].as_u64()), (Some(414), Some(585)));
    assert_eq!(report["images"][1]["textRuns"], 1);
    // a PC with fonts draws the text; one without says so
    let drawn = v["summary"]["textNotDrawn"].as_u64().unwrap();
    let codes: Vec<&str> = v["messages"].as_array().unwrap().iter().map(|m| m["code"].as_str().unwrap()).collect();
    assert_eq!(codes.contains(&"NO_FONTS"), drawn > 0, "{codes:?}");
}

#[test]
fn png_refuses_pages_that_are_not_there_or_too_many() {
    let dir = work("png-bad");
    let input = sample(&dir, false);
    let output = dir.join("output");
    for pages in ["3", "0", "x"] {
        let (code, v) = run(&["png", "--input", input.to_str().unwrap(), "--output-dir", output.to_str().unwrap(), "--pages", pages]);
        assert_eq!(code, 2, "{pages}");
        assert_eq!(v["errors"][0]["code"], "INVALID_ARGUMENT", "{pages}");
    }
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
}
