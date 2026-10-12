// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::{
    analysis::{self, Sample},
    capture::{Capture, Pixels},
    input,
};
use anyhow::{Context, Result};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HWND,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub fn measure(root: &Path, hwnd: usize, app: &str, scenario: &str, kind: &str) -> Result<()> {
    anyhow::ensure!(
        matches!(scenario, "firstview" | "end"),
        "scenario must be firstview or end"
    );
    anyhow::ensure!(
        [
            "ibisoffice",
            "msoffice-word",
            "msoffice-excel",
            "msoffice-powerpoint",
            "libreoffice",
            "onlyoffice"
        ]
        .contains(&app),
        "unknown benchmark app"
    );
    let file = crate::pdf::DOCS
        .iter()
        .find(|v| v.0 == kind)
        .context("unknown document kind")?
        .1;
    let target = crate::platform::windows()?
        .into_iter()
        .find(|w| w.hwnd == hwnd)
        .context("target window not found")?;
    let pid = target.pid;
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    let tag = format!("{scenario}-{app}-{kind}");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let work = root
        .join("target/benchmark-windows")
        .join(format!("{tag}-{stamp}"));
    std::fs::create_dir_all(&work)?;
    let source = work.join(file);
    std::fs::copy(root.join("test/benchmark").join(file), &source)?;
    unsafe {
        SetWindowPos(hwnd, None, 20, 20, 1440, 900, SWP_NOZORDER | SWP_NOACTIVATE)?;
    }
    eprintln!("measure {app}: copy ready; activating hwnd {}", target.hwnd);
    input::activate(hwnd)?;
    eprintln!("measure {app}: focus verified; sending file-open shortcut");
    // Prepare the common file-open dialog before t0. The timed operation is its Open action.
    // This is recorded as a Windows methodology difference from macOS open -a.
    input::key(
        pid,
        if app.starts_with("msoffice") {
            VK_F12
        } else {
            VK_O
        },
        Some(VK_CONTROL),
    )?;
    eprintln!("measure {app}: file-open shortcut sent; waiting for dialog");
    let deadline = Instant::now() + Duration::from_secs(10);
    let dialog = loop {
        let dialogs: Vec<_> = crate::platform::windows()?
            .into_iter()
            .filter(|w| {
                if w.pid != pid || w.hwnd == target.hwnd {
                    return false;
                }
                let mut class = [0u16; 64];
                let length =
                    unsafe { GetClassNameW(HWND(w.hwnd as *mut std::ffi::c_void), &mut class) };
                String::from_utf16_lossy(&class[..length.max(0) as usize]) == "#32770"
            })
            .collect();
        if dialogs.len() == 1 {
            break HWND(dialogs[0].hwnd as *mut std::ffi::c_void);
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "file-open dialog not found; input not sent"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    eprintln!("measure {app}: dialog found; setting verified file field");
    input::set_file_field(dialog, pid, &source.to_string_lossy().replace('/', "\\"))?;
    eprintln!("prepared {app} file-open dialog for pid {pid}");
    let mut capture = Capture::window(hwnd)?;
    let wait = Instant::now();
    let baseline: Pixels = loop {
        if let Some(frame) = capture.next()? {
            break frame;
        }
        anyhow::ensure!(wait.elapsed().as_secs() < 5, "no baseline frame");
        std::thread::sleep(Duration::from_millis(2));
    };
    baseline.save(&work.join("baseline.png"))?;
    let mut samples: Vec<Sample> = vec![baseline.sample()?];
    let t0 = crate::capture::now()?;
    input::key(pid, VK_RETURN, None)?;
    eprintln!("opening {kind}; capture t0 {t0:.6}");
    let cap = (match kind {
        "pptx" => 40.0,
        "docx" => 30.0,
        _ => 45.0,
    }) + if scenario == "end" { 30.0 } else { 0.0 };
    let area = if kind == "pptx" {
        if scenario == "end" {
            analysis::SLIDES
        } else {
            analysis::SLIDE
        }
    } else {
        analysis::DOC
    };
    let mut last_change = t0;
    let mut previous = baseline;
    let mut before_end = None;
    let mut end_frame = None;
    let mut nav_times = Vec::new();
    let mut last_nav = t0 - 1.0;
    let mut spreadsheet_nav_sent = false;
    let mut nav_log = Vec::<String>::new();
    let mut shown = false;
    let mut visually_shown = false;
    let mut presentation_ready =
        app != "msoffice-powerpoint" && !(app == "onlyoffice" && kind == "pptx");
    let mut presentation_end_reached = false;
    let mut last_ready_check = t0 - 1.0;
    let mut last_font_check = t0 - 1.0;
    let mut color_fraction = 0.0;
    let mut color_samples = Vec::new();
    let mut capped = true;
    let mut cpu = crate::metrics::Cpu::new(pid)?;
    let mut cpu_samples = Vec::new();
    let mut last_busy = t0;
    let quiet = if scenario == "end" && kind == "pptx" {
        20.0
    } else if scenario == "end" {
        3.0
    } else {
        2.5
    };
    loop {
        let now = crate::capture::now()?;
        if app == "libreoffice" && now - last_font_check >= 0.25 {
            if input::dismiss_libreoffice_dialog(hwnd, pid, None)? {
                nav_log.push(format!(
                    "{:.3}s: closed observed LibreOffice tip with OK; preferences unchanged",
                    now - t0
                ));
            }
            last_font_check = now;
        }
        if app == "msoffice-powerpoint" && !presentation_ready && now - last_font_check >= 0.25 {
            if input::dismiss_embedded_font_failure(pid)? {
                nav_log.push(format!("{:.3}s: acknowledged embedded-font installation failure with OK; no fonts installed", now - t0));
            }
            last_font_check = now;
        }
        let cores = cpu.update()?;
        if cores >= 0.15 {
            last_busy = now;
        }
        if cpu_samples
            .last()
            .is_none_or(|sample: &(f64, f64)| now - t0 - sample.0 >= 0.2)
        {
            cpu_samples.push((now - t0, cores));
        }
        // Preserve a tail after the measurement cap to verify the last frame.
        // Completion later than cap remains N/A, even if the tail becomes quiet.
        if now - t0 >= cap + quiet || (!shown && now - t0 >= cap) {
            break;
        }
        // Graphics Capture is window-scoped and works while occluded. Focus is
        // required only for input; dialog teardown can momentarily change it.
        if let Some(frame) = capture.next()? {
            color_fraction = frame.color_fraction();
            color_samples.push((frame.time - t0, color_fraction));
            let sample = frame.sample()?;
            let fraction = analysis::changed(&sample.grid, &samples.last().unwrap().grid, area);
            if fraction > analysis::CHANGE && sample.time > t0 {
                last_change = sample.time;
                before_end = Some(previous);
                end_frame = Some(frame.bgra.clone());
            }
            let threshold = if app == "onlyoffice" {
                0.03
            } else if app == "libreoffice" {
                0.40
            } else {
                0.10
            };
            visually_shown |= analysis::changed(&sample.grid, &samples[0].grid, analysis::FULL)
                > threshold
                && analysis::has_content(&sample.grid, area);
            samples.push(sample);
            previous = frame;
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
        if !presentation_ready && visually_shown && now - last_ready_check >= 0.5 {
            presentation_ready = if app == "onlyoffice" {
                input::has_named_element(hwnd, pid, "スライド 1/31")?
            } else {
                crate::office::presentation_ready(&source)
            };
            last_ready_check = now;
        }
        shown |= visually_shown && presentation_ready;
        if scenario == "end"
            && shown
            && app == "libreoffice"
            && kind == "pptx"
            && !presentation_end_reached
            && now - last_nav >= 0.5
        {
            presentation_end_reached =
                input::has_named_element(hwnd, pid, "PageShape: スライド 31")?;
            if presentation_end_reached {
                nav_log.push(format!(
                    "{:.3}s: observed last slide canvas; stopped repeated End input",
                    now - t0
                ));
            }
        }
        if scenario == "end"
            && shown
            && now - last_nav >= 0.5
            && !spreadsheet_nav_sent
            && !presentation_end_reached
            && (kind != "xlsx" || color_fraction > 0.005)
        {
            if kind == "xlsx" {
                if app == "msoffice-excel" {
                    input::key(pid, VK_F5, None)?;
                    let field_id = input::set_focused_field(pid, "R213", None)?;
                    nav_log.push(format!(
                        "Go To R213; UIA value readback verified; field {field_id}"
                    ));
                    input::key(pid, VK_RETURN, None)?;
                } else if app == "ibisoffice" {
                    input::key(pid, VK_G, Some(VK_CONTROL))?;
                    // GPUI does not expose its jump field to UIA. Review the
                    // snapshots to verify the resulting chart position.
                    std::thread::sleep(Duration::from_millis(300));
                    input::text(pid, "A193:R213")?;
                    input::key(pid, VK_RETURN, None)?;
                    nav_log.push("GPUI jump A193:R213; UIA field unavailable; screenshot verification required".into());
                } else if app == "onlyoffice" {
                    input::navigate_name_box(
                        hwnd,
                        pid,
                        windows::Win32::UI::Accessibility::UIA_AutomationIdPropertyId.0,
                        "ce-cell-name",
                        "A203",
                    )?;
                    nav_log.push(
                        "Name box A203; observed UIA ce-cell-name; typed value readback verified"
                            .into(),
                    );
                } else if app == "libreoffice" {
                    input::navigate_name_box(
                        hwnd,
                        pid,
                        windows::Win32::UI::Accessibility::UIA_NamePropertyId.0,
                        "A1",
                        "A193:R213",
                    )?;
                    nav_log.push("Name box A193:R213; observed initial UIA Edit named A1; typed value readback verified".into());
                } else {
                    anyhow::bail!("unsupported spreadsheet navigation app");
                }
                spreadsheet_nav_sent = true;
            } else {
                input::key(
                    pid,
                    VK_END,
                    if kind == "docx" {
                        Some(VK_CONTROL)
                    } else {
                        None
                    },
                )?;
            }
            nav_times.push(now - t0);
            last_nav = now;
        }
        if shown
            && (kind != "xlsx" || color_fraction > 0.005)
            && now - last_change > quiet
            && (now - last_busy > 2.5 || now - last_change > 8.0)
            && (scenario != "end" || !nav_times.is_empty())
        {
            capped = false;
            break;
        }
    }
    previous.save(&work.join("last.png"))?;
    if let Some(frame) = before_end {
        frame.save(&work.join("before-end.png"))?;
    }
    if let Some(bgra) = end_frame {
        Pixels { bgra, ..previous }.save(&work.join("end.png"))?;
    }
    let analysis = analysis::analyze(&samples, t0, area)?;
    capped |= analysis.end.is_none_or(|end| end >= cap);
    let runs = root.join("tasks/benchmark/runs-windows");
    std::fs::create_dir_all(&runs)?;
    std::fs::write(work.join("samples.json"), serde_json::to_vec(&samples)?)?;
    let result = serde_json::json!({"tag":tag,"app":app,"file":file,"scenario":scenario,"t0":t0,"cap":cap,"capped":capped,
        "stopped":crate::capture::now()? - t0,"shown":shown,"nav_times":nav_times,"nav_log":nav_log,
        "frames":samples.iter().enumerate().map(|(i,s)|(s.time,s.received,if i==0 {1.0} else {analysis::changed(&s.grid,&samples[i-1].grid,analysis::FULL)})).collect::<Vec<_>>(),
        "cpu":cpu_samples,"cpu_scope":"target window process; helpers are not included",
        "color_samples":color_samples,"chart_color_threshold":0.005,
        "build_waited":0,"build_running_after":crate::metrics::build_running()?,
        "status":"validation; final benchmark suite and screenshot review pending",
        "samples":work.join("samples.json"),"frames_dir":work,"method":"prepared file-open dialog; timing starts immediately before Open"});
    std::fs::write(
        runs.join(format!("{tag}.json")),
        serde_json::to_vec_pretty(&result)?,
    )?;
    std::fs::write(
        runs.join(format!("{tag}.analysis.json")),
        serde_json::to_vec_pretty(&analysis)?,
    )?;
    println!("{tag}: {:?}s, capped {capped}, shown {shown}", analysis.end);
    Ok(())
}

pub fn fixture_run(root: &Path, app: &str, scenario: &str, kind: &str) -> Result<()> {
    let (executable, process, actual_executable) = match app {
        "ibisoffice" => (
            root.join("target/release/ibisOffice.exe"),
            "ibisOffice",
            "ibisOffice.exe",
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
    let existing = std::process::Command::new("powershell.exe")
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
        existing.success(),
        "existing target app or build running; left untouched"
    );
    let mut command = std::process::Command::new(&executable);
    // GUI launchers must not share the runner's console/control-event group.
    // ONLYOFFICE's launcher exits before its actual editor process.
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x00000008 | 0x00000200); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    if app == "libreoffice" {
        command.arg("--norestore");
    }
    let mut child = command.spawn()?;
    let launch_pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(25);
    let target = loop {
        let candidates: Vec<_> = crate::platform::windows()?
            .into_iter()
            .filter(|w| {
                let matches = if w.pid == launch_pid {
                    true
                } else {
                    crate::platform::process_path(w.pid)
                        .ok()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                        .is_some_and(|name| name.eq_ignore_ascii_case(actual_executable))
                };
                if !matches {
                    return false;
                }
                let mut rect = windows::Win32::Foundation::RECT::default();
                (unsafe { GetWindowRect(HWND(w.hwnd as *mut std::ffi::c_void), &mut rect).is_ok() })
                    && rect.right - rect.left > 500
                    && rect.bottom - rect.top > 400
            })
            .collect();
        if candidates.len() == 1 {
            break candidates.into_iter().next().unwrap();
        }
        if app == "ibisoffice" {
            anyhow::ensure!(
                child.try_wait()?.is_none(),
                "fixture exited before its window opened"
            );
        }
        anyhow::ensure!(Instant::now() < deadline, "fixture window not found");
        std::thread::sleep(Duration::from_millis(20));
    };
    let pid = target.pid;
    eprintln!(
        "fixture {app}: launcher {launch_pid}, window pid {pid}, hwnd {}, executable {}",
        target.hwnd,
        crate::platform::process_path(pid)?.display()
    );
    let hwnd = HWND(target.hwnd as *mut std::ffi::c_void);
    unsafe {
        SetWindowPos(hwnd, None, 20, 20, 1440, 900, SWP_NOZORDER | SWP_NOACTIVATE)?;
    }
    let mut capture = Capture::window(hwnd)?;
    let start = Instant::now();
    eprintln!("fixture capture ready; waiting for initial screen");
    let mut last_change = Instant::now();
    let mut grid: Option<Vec<u8>> = None;
    while start.elapsed() < Duration::from_secs(20) {
        if let Some(frame) = capture.next()? {
            let next = frame.sample()?.grid;
            if grid.as_ref().is_none_or(|previous| {
                analysis::changed(previous, &next, analysis::FULL) > analysis::CHANGE
            }) {
                last_change = Instant::now();
            }
            grid = Some(next);
        }
        if grid.is_some() && last_change.elapsed() >= Duration::from_millis(1500) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    drop(capture);
    eprintln!("fixture initial screen settled; opening benchmark copy");
    let result = measure(root, target.hwnd, app, scenario, kind);
    // Owned fixture only; preserve failures if normal close cannot complete.
    let cleanup = if app.starts_with("msoffice") {
        crate::office::close_owned_fixture(app, root, target.hwnd, pid)
    } else if app == "libreoffice" {
        (|| -> Result<()> {
            crate::platform::close_fixture(target.hwnd, pid)?;
            let file = crate::pdf::DOCS
                .iter()
                .find(|v| v.0 == kind)
                .context("unknown document kind")?
                .1;
            let deadline = Instant::now() + Duration::from_secs(10);
            while crate::platform::windows()?
                .iter()
                .any(|w| w.hwnd == target.hwnd && w.pid == pid)
            {
                if input::dismiss_libreoffice_dialog(hwnd, pid, Some(file))? {
                    eprintln!("closed verified LibreOffice fixture notification without saving");
                }
                if !crate::platform::windows()?
                    .iter()
                    .any(|w| w.hwnd == target.hwnd && w.pid == pid)
                {
                    break;
                }
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "LibreOffice close incomplete; owned pid {pid} left running"
                );
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        })()
    } else {
        crate::platform::close_fixture(target.hwnd, pid)
    };
    result?;
    cleanup?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while crate::platform::process_path(pid).is_ok() {
        anyhow::ensure!(
            Instant::now() < deadline,
            "fixture did not exit normally; pid {pid} left running"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
