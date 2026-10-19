// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Startup timing starts before launch; display samples stay local under target/.
use crate::{
    analysis,
    capture::{Capture, Pixels},
};
use anyhow::Result;
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub fn measure(root: &Path, app: &str) -> Result<()> {
    let (exe, process, actual) = match app {
        "ibisoffice" => (
            root.join("target/release/AcaudaOffice.exe"),
            "AcaudaOffice",
            "AcaudaOffice.exe",
        ),
        "msoffice-word" => (
            "C:/Program Files/Microsoft Office/root/Office16/WINWORD.EXE".into(),
            "WINWORD",
            "WINWORD.EXE",
        ),
        "msoffice-excel" => (
            "C:/Program Files/Microsoft Office/root/Office16/EXCEL.EXE".into(),
            "EXCEL",
            "EXCEL.EXE",
        ),
        "msoffice-powerpoint" => (
            "C:/Program Files/Microsoft Office/root/Office16/POWERPNT.EXE".into(),
            "POWERPNT",
            "POWERPNT.EXE",
        ),
        "libreoffice" => (
            "C:/Program Files/LibreOffice/program/soffice.exe".into(),
            "soffice.bin",
            "soffice.bin",
        ),
        "onlyoffice" => (
            "C:/Program Files/ONLYOFFICE/DesktopEditors/DesktopEditors.exe".into(),
            "editors",
            "editors.exe",
        ),
        _ => anyhow::bail!("unknown app"),
    };
    let process_names = if app == "onlyoffice" {
        "'DesktopEditors','editors'".to_owned()
    } else {
        format!("'{process}'")
    };
    let preflight = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "if (Get-Process {process_names},rustc -ErrorAction SilentlyContinue) {{ exit 9 }}"
            ),
        ])
        .status()?;
    anyhow::ensure!(
        preflight.success(),
        "existing target app or build running; left untouched"
    );
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let work = root
        .join("target/benchmark-windows")
        .join(format!("startup-{app}-{stamp}"));
    std::fs::create_dir_all(&work)?;
    let mut capture = Capture::monitor()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let baseline = loop {
        if let Some(frame) = capture.next()? {
            break frame;
        }
        anyhow::ensure!(Instant::now() < deadline, "startup baseline missing");
        std::thread::sleep(Duration::from_millis(2));
    };
    let mut samples = vec![baseline.sample()?];
    let mut previous = baseline;
    let t0 = crate::capture::now()?;
    let mut command = std::process::Command::new(exe);
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x00000008 | 0x00000200); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    if app == "libreoffice" {
        command.arg("--norestore");
    }
    let child = command.spawn()?;
    let mut last_change = t0;
    let mut last_bgra = None;
    let mut before_end = None;
    let mut target = None;
    let mut target_rect = None;
    let mut capped = true;
    while crate::capture::now()? - t0 < 22.5 {
        let now = crate::capture::now()?;
        if target.is_none() {
            let candidates: Vec<_> = crate::platform::windows()?
                .into_iter()
                .filter(|w| {
                    if w.pid != child.id()
                        && !crate::platform::process_path(w.pid)
                            .ok()
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                            .is_some_and(|n| n.eq_ignore_ascii_case(actual))
                    {
                        return false;
                    }
                    let mut rect = windows::Win32::Foundation::RECT::default();
                    (unsafe {
                        windows::Win32::UI::WindowsAndMessaging::GetWindowRect(
                            windows::Win32::Foundation::HWND(w.hwnd as *mut std::ffi::c_void),
                            &mut rect,
                        )
                        .is_ok()
                    }) && rect.right - rect.left > 500
                        && rect.bottom - rect.top > 400
                })
                .collect();
            if candidates.len() == 1 {
                let window = candidates.into_iter().next().unwrap();
                let hwnd = windows::Win32::Foundation::HWND(window.hwnd as *mut std::ffi::c_void);
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::*;
                    SetWindowPos(hwnd, None, 20, 20, 1440, 900, SWP_NOZORDER | SWP_NOACTIVATE)?;
                    let mut rect = windows::Win32::Foundation::RECT::default();
                    GetWindowRect(hwnd, &mut rect)?;
                    anyhow::ensure!(
                        rect.left == 20
                            && rect.top == 20
                            && rect.right - rect.left == 1440
                            && rect.bottom - rect.top == 900,
                        "startup fixture size or position mismatch"
                    );
                    target_rect =
                        Some((rect.left as usize, rect.top as usize, 1440usize, 900usize));
                }
                eprintln!(
                    "startup {app}: normalized owned window {}, pid {}",
                    window.hwnd, window.pid
                );
                target = Some(window);
            }
        }
        if let Some(frame) = capture.next()? {
            let sample = frame.sample()?;
            if analysis::changed(&sample.grid, &samples.last().unwrap().grid, analysis::FULL)
                > analysis::CHANGE
                && sample.time > t0
            {
                last_change = sample.time;
                before_end = Some(previous);
                last_bgra = Some(frame.bgra.clone());
            }
            samples.push(sample);
            previous = frame;
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
        if target.is_some() && last_change > t0 && now - last_change > 2.5 {
            capped = last_change - t0 >= 20.0;
            break;
        }
    }
    previous.save(&work.join("last.png"))?;
    if let Some((x, y, w, h)) = target_rect {
        previous
            .crop(x, y, w, h)?
            .save(&work.join("review-last.png"))?;
    }
    if let Some(before) = before_end {
        before.save(&work.join("before-end.png"))?;
        if let Some((x, y, w, h)) = target_rect {
            before
                .crop(x, y, w, h)?
                .save(&work.join("review-before-end.png"))?;
        }
    }
    if let Some(bgra) = last_bgra {
        let end = Pixels { bgra, ..previous };
        end.save(&work.join("end.png"))?;
        if let Some((x, y, w, h)) = target_rect {
            end.crop(x, y, w, h)?.save(&work.join("review-end.png"))?;
        }
    }
    let result = analysis::analyze(&samples, t0, analysis::FULL)?;
    capped |= result.end.is_none_or(|t| t >= 20.0);
    std::fs::write(work.join("samples.json"), serde_json::to_vec(&samples)?)?;
    let runs = root.join("tasks/benchmark/runs-windows");
    std::fs::create_dir_all(&runs)?;
    let tag = format!("startup-{app}");
    std::fs::write(
        runs.join(format!("{tag}.analysis.json")),
        serde_json::to_vec_pretty(&result)?,
    )?;
    std::fs::write(
        runs.join(format!("{tag}.json")),
        serde_json::to_vec_pretty(
            &serde_json::json!({"tag":tag,"app":app,"scenario":"startup","t0":t0,"cap":20.0,"capped":capped,"frames_dir":work,"review_rect":target_rect,"build_running_after":crate::metrics::build_running()?,"status":"validation; final suite and screenshot review pending","method":"display capture started before process launch; QPC clock; owned window normalized to 1440x900 at first detection; review crops use the same recorded display frames"}),
        )?,
    )?;
    drop(capture);
    if let Some(target) = target {
        if app.starts_with("msoffice") {
            crate::office::close_owned_fixture(app, root, target.hwnd, target.pid)?;
        } else {
            crate::platform::close_fixture(target.hwnd, target.pid)?;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while crate::platform::process_path(target.pid).is_ok() {
            anyhow::ensure!(
                Instant::now() < deadline,
                "startup fixture did not exit normally; pid {} left running",
                target.pid
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    } else {
        anyhow::bail!(
            "startup target window not found; launcher pid {} left for review",
            child.id()
        );
    }
    println!("{tag}: {:?}s, capped {capped}", result.end);
    Ok(())
}
