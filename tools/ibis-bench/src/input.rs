// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Benchmark input: refuse every keystroke unless the target process has focus.
use anyhow::Result;
use windows::Win32::{
    Foundation::HWND,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub fn focused(pid: u32) -> bool {
    let mut foreground_pid = 0;
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut foreground_pid));
    }
    foreground_pid == pid
}
/// The foreground window's process and title, for the activation error.
fn foreground_description() -> String {
    let (mut pid, mut title) = (0, [0u16; 128]);
    unsafe {
        let hwnd = GetForegroundWindow();
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let len = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        format!("hwnd {:?} pid {pid} title {:?}", hwnd.0, String::from_utf16_lossy(&title[..len]))
    }
}
fn keyboard(key: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
fn send(pid: u32, inputs: &[INPUT]) -> Result<()> {
    anyhow::ensure!(
        focused(pid),
        "target lost focus; refusing to send benchmark input"
    );
    anyhow::ensure!(
        unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) } == inputs.len() as u32,
        "benchmark input rejected"
    );
    Ok(())
}
pub fn key(pid: u32, key: VIRTUAL_KEY, modifier: Option<VIRTUAL_KEY>) -> Result<()> {
    let mut inputs = Vec::new();
    if let Some(modifier) = modifier {
        inputs.push(keyboard(modifier, 0, KEYBD_EVENT_FLAGS(0)));
    }
    inputs.push(keyboard(key, 0, KEYBD_EVENT_FLAGS(0)));
    inputs.push(keyboard(key, 0, KEYEVENTF_KEYUP));
    if let Some(modifier) = modifier {
        inputs.push(keyboard(modifier, 0, KEYEVENTF_KEYUP));
    }
    send(pid, &inputs)
}
pub fn text(pid: u32, text: &str) -> Result<()> {
    let mut inputs = Vec::new();
    for scan in text.encode_utf16() {
        inputs.push(keyboard(VIRTUAL_KEY(0), scan, KEYEVENTF_UNICODE));
        inputs.push(keyboard(
            VIRTUAL_KEY(0),
            scan,
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        ));
    }
    send(pid, &inputs)
}
pub fn activate(hwnd: HWND) -> Result<()> {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        unsafe {
            let _ = SetForegroundWindow(hwnd);
        }
        if focused(pid) {
            return Ok(());
        }
        // Windows lets only the process holding the foreground (or sharing
        // its input queue) take it: join the foreground thread's input for
        // one call. No key is sent.
        unsafe {
            use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
            let foreground = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let ours = GetCurrentThreadId();
            let attached = foreground != 0 && foreground != ours && AttachThreadInput(ours, foreground, true).as_bool();
            let _ = SetForegroundWindow(hwnd);
            if attached {
                let _ = AttachThreadInput(ours, foreground, false);
            }
            if !focused(pid) {
                // The switch the task switcher uses; not held by the lock.
                SwitchToThisWindow(hwnd, true);
            }
        }
        if focused(pid) {
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "could not activate benchmark target; no input sent (wanted pid {pid}, foreground {})",
            foreground_description()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

pub fn set_focused_field(pid: u32, text: &str, automation_id: Option<&str>) -> Result<String> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Accessibility::*,
        },
        core::BSTR,
    };
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        anyhow::ensure!(
            focused(pid),
            "target lost focus while waiting for input field"
        );
        let field = unsafe { automation.GetFocusedElement()? };
        let valid = unsafe {
            field.CurrentProcessId()? == pid as i32
                && field.CurrentControlType()? == UIA_EditControlTypeId
        };
        let id = unsafe { field.CurrentAutomationId()? }.to_string();
        if valid && automation_id.is_none_or(|wanted| wanted == id) {
            let value: IUIAutomationValuePattern =
                unsafe { field.GetCurrentPatternAs(UIA_ValuePatternId)? };
            let desired = BSTR::from(text);
            unsafe {
                value.SetValue(&desired)?;
            }
            let readback = unsafe { value.CurrentValue()? }.to_string();
            anyhow::ensure!(
                readback == text,
                "UI Automation field readback does not match input"
            );
            return Ok(id);
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "verified editable field not ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

pub fn set_file_field(dialog: HWND, pid: u32, text: &str) -> Result<()> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            UI::Accessibility::*,
        },
        core::BSTR,
    };
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let root = unsafe { automation.ElementFromHandle(dialog)? };
    let id =
        unsafe { automation.CreatePropertyCondition(UIA_AutomationIdPropertyId, &"1148".into())? };
    let edit = unsafe {
        automation
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_EditControlTypeId.0.into())?
    };
    let condition = unsafe { automation.CreateAndCondition(&id, &edit)? };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Ok(field) = unsafe { root.FindFirst(TreeScope_Descendants, &condition) } {
            anyhow::ensure!(
                unsafe { field.CurrentProcessId()? } == pid as i32,
                "file field belongs to another process"
            );
            unsafe {
                field.SetFocus()?;
            }
            let value: IUIAutomationValuePattern =
                unsafe { field.GetCurrentPatternAs(UIA_ValuePatternId)? };
            unsafe {
                value.SetValue(&BSTR::from(text))?;
            }
            anyhow::ensure!(
                unsafe { value.CurrentValue()? }.to_string() == text,
                "file path readback mismatch"
            );
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "file name edit not ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Use the observed editor name box, with keyboard events (CEF SetValue alone
/// does not commit the application's navigation state). Verify its value too.
pub fn navigate_name_box(
    hwnd: HWND,
    pid: u32,
    property: i32,
    selector: &str,
    address: &str,
) -> Result<()> {
    use windows::Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::Accessibility::*,
    };
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let root = unsafe { automation.ElementFromHandle(hwnd)? };
    anyhow::ensure!(
        unsafe { root.CurrentProcessId()? } == pid as i32,
        "name box ownership mismatch"
    );
    let selector_condition =
        unsafe { automation.CreatePropertyCondition(UIA_PROPERTY_ID(property), &selector.into())? };
    let edit_condition = unsafe {
        automation
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_EditControlTypeId.0.into())?
    };
    let condition = unsafe { automation.CreateAndCondition(&selector_condition, &edit_condition)? };
    // The editor's accessibility tree may not hold the name box yet (the page
    // is still being built): look again for up to 5 s while there is none.
    // More than one is never accepted.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let fields = loop {
        let fields = unsafe { root.FindAll(TreeScope_Descendants, &condition)? };
        let found = unsafe { fields.Length()? };
        if found == 1 {
            break fields;
        }
        anyhow::ensure!(
            found == 0 && std::time::Instant::now() < deadline,
            "name box missing or ambiguous"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let field = unsafe { fields.GetElement(0)? };
    anyhow::ensure!(
        unsafe { field.CurrentProcessId()? } == pid as i32
            && unsafe { field.CurrentIsEnabled()? }.as_bool()
            && !unsafe { field.CurrentIsOffscreen()? }.as_bool(),
        "name box unavailable"
    );
    let value: IUIAutomationValuePattern =
        unsafe { field.GetCurrentPatternAs(UIA_ValuePatternId)? };
    let rect = unsafe { field.CurrentBoundingRectangle()? };
    let point = windows::Win32::Foundation::POINT {
        x: (rect.left + rect.right) / 2,
        y: (rect.top + rect.bottom) / 2,
    };
    let mut point_pid = 0;
    unsafe {
        GetWindowThreadProcessId(WindowFromPoint(point), Some(&mut point_pid));
    }
    anyhow::ensure!(
        point_pid == pid && rect.right > rect.left && rect.bottom > rect.top,
        "name box is occluded; no input sent"
    );
    activate(hwnd)?;
    unsafe {
        SetCursorPos(point.x, point.y)?;
    }
    let mouse = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    send(
        pid,
        &[mouse(MOUSEEVENTF_LEFTDOWN), mouse(MOUSEEVENTF_LEFTUP)],
    )?;
    key(pid, VK_A, Some(VK_CONTROL))?;
    text(pid, address)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while unsafe { value.CurrentValue()? }.to_string() != address {
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "name box typed value readback mismatch"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    key(pid, VK_RETURN, None)?;
    Ok(())
}

/// Read only a specific, empirically observed UIA marker in the owned window.
pub fn has_named_element(hwnd: HWND, pid: u32, name: &str) -> Result<bool> {
    use windows::Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::Accessibility::*,
    };
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let root = unsafe { automation.ElementFromHandle(hwnd)? };
    anyhow::ensure!(
        unsafe { root.CurrentProcessId()? } == pid as i32,
        "readiness marker ownership mismatch"
    );
    let condition =
        unsafe { automation.CreatePropertyCondition(UIA_NamePropertyId, &name.into())? };
    let found = unsafe { root.FindAll(TreeScope_Descendants, &condition)? };
    for index in 0..unsafe { found.Length()? } {
        let element = unsafe { found.GetElement(index)? };
        if unsafe { element.CurrentProcessId()? } == pid as i32
            && !unsafe { element.CurrentIsOffscreen()? }.as_bool()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Acknowledge only PowerPoint's observed embedded-font failure notification.
/// This neither installs fonts nor accepts an installation/permission prompt.
pub fn dismiss_embedded_font_failure(pid: u32) -> Result<bool> {
    use windows::Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::Accessibility::*,
    };
    let dialogs: Vec<_> = crate::platform::windows()?
        .into_iter()
        .filter(|w| w.pid == pid && w.title == "埋め込みフォントのインストール")
        .collect();
    if dialogs.is_empty() {
        return Ok(false);
    }
    anyhow::ensure!(
        dialogs.len() == 1,
        "ambiguous embedded-font dialog; left untouched"
    );
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let root =
        unsafe { automation.ElementFromHandle(HWND(dialogs[0].hwnd as *mut std::ffi::c_void))? };
    let condition = unsafe { automation.CreateTrueCondition()? };
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition)? };
    let mut failure = false;
    let mut buttons = Vec::new();
    for index in 0..unsafe { elements.Length()? } {
        let element = unsafe { elements.GetElement(index)? };
        if unsafe { element.CurrentProcessId()? } != pid as i32 {
            continue;
        }
        let name = unsafe { element.CurrentName()? }.to_string();
        failure |= name.contains("フォントのインストール失敗");
        if name.trim() == "OK"
            && unsafe { element.CurrentControlType()? } == UIA_ButtonControlTypeId
        {
            buttons.push(element);
        }
    }
    anyhow::ensure!(
        failure && buttons.len() == 1,
        "unrecognized embedded-font notification; left untouched"
    );
    let button: IUIAutomationInvokePattern =
        unsafe { buttons[0].GetCurrentPatternAs(UIA_InvokePatternId)? };
    unsafe {
        button.Invoke()?;
    }
    Ok(true)
}

/// Inspect an owned app's modal subtree before invoking a known harmless action.
pub fn dismiss_libreoffice_dialog(hwnd: HWND, pid: u32, file: Option<&str>) -> Result<bool> {
    let owned_window_exists = || -> Result<bool> {
        Ok(crate::platform::windows()?
            .iter()
            .any(|w| w.hwnd == hwnd.0 as usize && w.pid == pid))
    };
    if !owned_window_exists()? {
        return Ok(false);
    }
    match inspect_libreoffice_dialog(hwnd, pid, file) {
        Err(_) if !owned_window_exists()? => Ok(false), // Normal close can race UIA reads.
        result => result,
    }
}

fn inspect_libreoffice_dialog(hwnd: HWND, pid: u32, file: Option<&str>) -> Result<bool> {
    use windows::Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::Accessibility::*,
    };
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let root = unsafe { automation.ElementFromHandle(hwnd)? };
    anyhow::ensure!(
        unsafe { root.CurrentProcessId()? } == pid as i32,
        "LibreOffice ownership mismatch"
    );
    let all = unsafe { automation.CreateTrueCondition()? };
    let windows_only = unsafe {
        automation
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_WindowControlTypeId.0.into())?
    };
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &windows_only)? };
    for index in 0..unsafe { elements.Length()? } {
        let dialog = unsafe { elements.GetElement(index)? };
        if unsafe { dialog.CurrentProcessId()? } != pid as i32
            || unsafe { dialog.CurrentControlType()? } != UIA_WindowControlTypeId
        {
            continue;
        }
        let name = unsafe { dialog.CurrentName()? }.to_string();
        let tip = name.starts_with("今日のヒント:");
        let save = file.is_some() && name == "ドキュメントを保存しますか？";
        if !tip && !save {
            continue;
        }
        let children = unsafe { dialog.FindAll(TreeScope_Descendants, &all)? };
        let mut verified_file = !save;
        let mut buttons = Vec::new();
        for child_index in 0..unsafe { children.Length()? } {
            let child = unsafe { children.GetElement(child_index)? };
            if unsafe { child.CurrentProcessId()? } != pid as i32 {
                continue;
            }
            let text = unsafe { child.CurrentName()? }.to_string();
            if let Some(file) = file {
                verified_file |=
                    text == format!("閉じる前にドキュメント“{file}” への変更を保存しますか？");
            }
            if unsafe { child.CurrentControlType()? } == UIA_ButtonControlTypeId
                && text == if tip { "OK" } else { "保存しない(N)" }
            {
                buttons.push(child);
            }
        }
        anyhow::ensure!(
            verified_file && buttons.len() == 1,
            "unrecognized LibreOffice modal; left untouched"
        );
        let button: IUIAutomationInvokePattern =
            unsafe { buttons[0].GetCurrentPatternAs(UIA_InvokePatternId)? };
        unsafe {
            button.Invoke()?;
        }
        return Ok(true);
    }
    Ok(false)
}
