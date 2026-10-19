// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! PDF conversion runs: copies of public test documents, no modification of originals.
use anyhow::{Context, Result};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use windows::{Data::Pdf::PdfDocument, Storage::StorageFile};

pub const DOCS: [(&str, &str); 3] = [
    ("pptx", "earnings-presentation.pptx"),
    ("docx", "securities-registration-statement.docx"),
    ("xlsx", "coffee-chain-sales-report.xlsx"),
];

/// Render first/last pages for local visual review; never marks a result reviewed.
pub fn review_images(root: &Path, tag: &str) -> Result<()> {
    anyhow::ensure!(
        tag.starts_with("pdf-") && !tag.contains(['/', '\\']) && !tag.contains(".."),
        "expected a PDF result tag"
    );
    let result: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("tasks/benchmark/runs-windows")
            .join(format!("{tag}.json")),
    )?)?;
    let output =
        std::path::PathBuf::from(result["pdf"].as_str().context("result has no PDF path")?);
    let canonical = output.canonicalize()?;
    anyhow::ensure!(
        canonical.starts_with(root.join("target/benchmark-windows").canonicalize()?),
        "PDF outside benchmark artifacts"
    );
    let file = StorageFile::GetFileFromPathAsync(&windows::core::HSTRING::from(
        canonical.to_string_lossy().trim_start_matches("\\\\?\\"),
    ))?
    .join()?;
    let document = PdfDocument::LoadFromFileAsync(&file)?.join()?;
    let pages = document.PageCount()?;
    anyhow::ensure!(pages > 0, "PDF has no pages");
    for (label, number) in [("first", 0), ("last", pages - 1)] {
        let page = document.GetPage(number)?;
        let stream = windows::Storage::Streams::InMemoryRandomAccessStream::new()?;
        let options = windows::Data::Pdf::PdfPageRenderOptions::new()?;
        options.SetDestinationWidth(1200)?;
        page.RenderWithOptionsToStreamAsync(&stream, &options)?
            .join()?;
        let reader =
            windows::Storage::Streams::DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        let length = u32::try_from(stream.Size()?)?;
        reader.LoadAsync(length)?.join()?;
        let mut bytes = vec![0; length as usize];
        reader.ReadBytes(&mut bytes)?;
        let path = output.with_file_name(format!("review-{label}.png"));
        std::fs::write(&path, bytes)?;
        page.Close()?;
        println!("page {} of {pages}: {}", number + 1, path.display());
    }
    Ok(())
}

pub fn run(root: &Path, filter: &str, kind_filter: &str) -> Result<()> {
    let runs = root.join("tasks/benchmark/runs-windows");
    let artifacts = root.join("target/benchmark-windows");
    std::fs::create_dir_all(&runs)?;
    std::fs::create_dir_all(&artifacts)?;
    for app in [
        "ibisoffice",
        "msoffice-powerpoint",
        "msoffice-word",
        "msoffice-excel",
        "libreoffice",
        "onlyoffice",
    ] {
        if !filter.is_empty() && !app.contains(filter) {
            continue;
        }
        for (kind, file) in DOCS {
            if !kind_filter.is_empty() && kind_filter != kind {
                continue;
            }
            if app.starts_with("msoffice")
                && !matches!(
                    (app, kind),
                    ("msoffice-powerpoint", "pptx")
                        | ("msoffice-word", "docx")
                        | ("msoffice-excel", "xlsx")
                )
            {
                continue;
            }
            let process = match app {
                "ibisoffice" => "ibisOffice",
                "msoffice-word" => "WINWORD",
                "msoffice-excel" => "EXCEL",
                "msoffice-powerpoint" => "POWERPNT",
                "libreoffice" => "soffice.bin",
                _ => "DesktopEditors",
            };
            // Refuse to disturb pre-existing instances or measure alongside a build.
            let check = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &format!("if (Get-Process '{process}',rustc -ErrorAction SilentlyContinue) {{ exit 9 }}")]).status()?;
            anyhow::ensure!(
                check.success(),
                "{app}: existing app or Rust build running; no processes were closed"
            );
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis();
            let work = artifacts.join(format!("pdf-{app}-{kind}-{stamp}"));
            std::fs::create_dir_all(&work)?;
            let source = work.join(file);
            std::fs::copy(root.join("test/benchmark").join(file), &source)?;
            let output = work.join(Path::new(file).with_extension("pdf"));
            let mut command = match app {
                "ibisoffice" => {
                    let mut c = Command::new(root.join("target/release/AcaudaOffice.exe"));
                    c.args(["--export", "pdf", "--out-dir"])
                        .arg(&work)
                        .arg(&source);
                    c
                }
                "libreoffice" => {
                    let mut c = Command::new("C:/Program Files/LibreOffice/program/soffice.com");
                    let profile = work
                        .join("profile")
                        .to_string_lossy()
                        .replace('\\', "/")
                        .replace(' ', "%20");
                    c.arg(format!("-env:UserInstallation=file:///{profile}"))
                        .args([
                            "--headless",
                            "--norestore",
                            "--convert-to",
                            "pdf",
                            "--outdir",
                        ])
                        .arg(&work)
                        .arg(&source);
                    c
                }
                "onlyoffice" => {
                    let fonts = std::path::PathBuf::from(std::env::var("LOCALAPPDATA")?)
                        .join("ONLYOFFICE/DesktopEditors/data/fonts");
                    anyhow::ensure!(
                        fonts.join("AllFonts.js").is_file(),
                        "ONLYOFFICE font list not initialized; open the desktop app once"
                    );
                    let xml_path = |path: &Path| {
                        path.to_string_lossy()
                            .replace('/', "\\")
                            .replace('&', "&amp;")
                            .replace('<', "&lt;")
                            .replace('>', "&gt;")
                    };
                    let xml = format!(
                        "<?xml version=\"1.0\" encoding=\"utf-8\"?><TaskQueueDataConvert xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\"><m_sFileFrom>{}</m_sFileFrom><m_sFileTo>{}</m_sFileTo><m_nFormatTo>513</m_nFormatTo><m_sFontDir>{}</m_sFontDir><m_sAllFontsPath>{}</m_sAllFontsPath><m_bIsNoBase64>true</m_bIsNoBase64><m_sTempDir>{}</m_sTempDir></TaskQueueDataConvert>",
                        xml_path(&source),
                        xml_path(&output),
                        xml_path(&fonts),
                        xml_path(&fonts.join("AllFonts.js")),
                        xml_path(&work)
                    );
                    let task = work.join("task.xml");
                    std::fs::write(&task, xml)?;
                    let mut c = Command::new(
                        "C:/Program Files/ONLYOFFICE/DesktopEditors/converter/x2t.exe",
                    );
                    c.current_dir("C:/Program Files/ONLYOFFICE/DesktopEditors/converter")
                        .arg(task);
                    c
                }
                _ => {
                    let mut c = Command::new(std::env::current_exe()?);
                    c.args(["office-pdf", app]).arg(&source).arg(&output);
                    c
                }
            };
            let log = std::fs::File::create(work.join("conversion.log"))?;
            command
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log));
            println!("starting {app} {kind}");
            let started = Instant::now();
            let mut child = command.spawn().context("start PDF converter")?;
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if started.elapsed() > Duration::from_secs(600) {
                    // Do not kill Office: a timed-out COM host could own unsaved/recovery state.
                    anyhow::bail!(
                        "{app} conversion timed out; child pid {} left running for review",
                        child.id()
                    );
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            let seconds = started.elapsed().as_secs_f64();
            let ok = status.success() && output.is_file();
            let pages = if ok {
                let path =
                    windows::core::HSTRING::from(output.to_string_lossy().replace('/', "\\"));
                let file = StorageFile::GetFileFromPathAsync(&path)?.join()?;
                Some(PdfDocument::LoadFromFileAsync(&file)?.join()?.PageCount()?)
            } else {
                None
            };
            let measured_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let result = serde_json::json!({"app":app,"kind":kind,"file":file,"seconds":seconds,"ok":ok,
                "pages":pages,"bytes":std::fs::metadata(&output).ok().map(|m|m.len()),"build_waited":0,
                "measured_at_unix":measured_at,"log":work.join("conversion.log"),"pdf":output,
                "method":"command spawn to process exit; Rust COM host for Microsoft Office",
                "status":"validation; final suite must run without concurrent builds or installers"});
            std::fs::write(
                runs.join(format!("pdf-{app}-{kind}.json")),
                serde_json::to_vec_pretty(&result)?,
            )?;
            println!("{app} {kind}: {seconds:.3}s, pages {pages:?}, ok {ok}");
        }
    }
    Ok(())
}
