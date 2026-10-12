// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use anyhow::Result;
use serde::Serialize;
use windows::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    },
};
use windows::core::BOOL;

#[derive(Debug, Serialize)]
pub struct Window {
    pub hwnd: usize,
    pub pid: u32,
    pub title: String,
}
pub fn windows() -> Result<Vec<Window>> {
    unsafe extern "system" fn callback(hwnd: HWND, param: LPARAM) -> BOOL {
        if unsafe { IsWindowVisible(hwnd).as_bool() } {
            let mut text = [0u16; 1024];
            let length = unsafe { GetWindowTextW(hwnd, &mut text) };
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
            }
            if length >= 0 {
                let list = unsafe { &mut *(param.0 as *mut Vec<Window>) };
                list.push(Window {
                    hwnd: hwnd.0 as usize,
                    pid,
                    title: String::from_utf16_lossy(&text[..length as usize]),
                });
            }
        }
        BOOL(1)
    }
    let mut list = Vec::new();
    unsafe {
        EnumWindows(
            Some(callback),
            LPARAM(&mut list as *mut Vec<Window> as isize),
        )?;
    }
    Ok(list)
}

pub fn process_path(pid: u32) -> Result<std::path::PathBuf> {
    use windows::{
        Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        core::PWSTR,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)? };
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    result?;
    Ok(String::from_utf16_lossy(&buffer[..length as usize]).into())
}

pub fn doctor(root: &std::path::Path) -> Result<()> {
    let office = std::path::Path::new("C:/Program Files/Microsoft Office/root/Office16");
    let apps = [
        ("ibisoffice", root.join("target/release/ibisOffice.exe")),
        ("msoffice-powerpoint", office.join("POWERPNT.EXE")),
        ("msoffice-word", office.join("WINWORD.EXE")),
        ("msoffice-excel", office.join("EXCEL.EXE")),
        (
            "libreoffice",
            "C:/Program Files/LibreOffice/program/soffice.exe".into(),
        ),
        (
            "onlyoffice",
            "C:/Program Files/ONLYOFFICE/DesktopEditors/DesktopEditors.exe".into(),
        ),
    ];
    let checks: Vec<_> = apps.iter().map(|(app, path)| serde_json::json!({"app": app, "path": path, "available": path.is_file()})).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "graphics_capture": windows::Graphics::Capture::GraphicsCaptureSession::IsSupported()?,
            "apps": checks,
            "note": "Read-only preflight. OS display/security settings are never changed."
        }))?
    );
    Ok(())
}

pub fn record(hwnd: usize, seconds: f64, out: &std::path::Path) -> Result<()> {
    anyhow::ensure!(
        seconds.is_finite() && seconds > 0.0 && seconds <= 120.0,
        "capture duration must be 0..120 seconds"
    );
    anyhow::ensure!(
        windows()?.iter().any(|w| w.hwnd == hwnd),
        "target must be a visible returned window"
    );
    std::fs::create_dir_all(out)?;
    let mut capture = crate::capture::Capture::window(HWND(hwnd as *mut std::ffi::c_void))?;
    let start = crate::capture::now()?;
    let mut samples = Vec::new();
    let mut last = None;
    while crate::capture::now()? - start < seconds {
        if let Some(frame) = capture.next()? {
            samples.push(frame.sample()?);
            if samples.len() == 1 {
                frame.save(&out.join("first.png"))?;
            }
            last = Some(frame);
        } else {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    anyhow::ensure!(!samples.is_empty(), "no frames captured");
    if let Some(last) = last {
        last.save(&out.join("last.png"))?;
    }
    std::fs::write(out.join("samples.json"), serde_json::to_vec(&samples)?)?;
    println!(
        "captured {} frames locally into {}",
        samples.len(),
        out.display()
    );
    Ok(())
}

pub fn close_fixture(hwnd: usize, owned_pid: u32) -> Result<()> {
    use windows::Win32::{
        Foundation::WPARAM,
        UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE},
    };
    let list = windows()?;
    anyhow::ensure!(
        list.iter().any(|w| w.hwnd == hwnd && w.pid == owned_pid),
        "fixture ownership mismatch"
    );
    // The caller supplies the PID it created. Do not enumerate or terminate other apps.
    for window in list
        .iter()
        .filter(|w| w.pid == owned_pid && w.hwnd != hwnd && w.title == "開く")
    {
        unsafe {
            PostMessageW(
                Some(HWND(window.hwnd as *mut std::ffi::c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            )?;
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut std::ffi::c_void)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        )?;
    }
    Ok(())
}
