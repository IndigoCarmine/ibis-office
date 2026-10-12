// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use anyhow::Result;
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

pub struct Cpu {
    handle: HANDLE,
    previous: f64,
    time: f64,
    pub cores: f64,
}
fn seconds(value: FILETIME) -> f64 {
    (((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64) as f64 / 10_000_000.0
}
impl Cpu {
    pub fn new(pid: u32) -> Result<Self> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)? };
        let mut result = Self {
            handle,
            previous: 0.0,
            time: crate::capture::now()?,
            cores: 0.0,
        };
        result.previous = result.total()?;
        Ok(result)
    }
    fn total(&self) -> Result<f64> {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe {
            GetProcessTimes(
                self.handle,
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )?;
        }
        Ok(seconds(kernel) + seconds(user))
    }
    pub fn update(&mut self) -> Result<f64> {
        let time = crate::capture::now()?;
        if time - self.time >= 0.2 {
            let total = self.total()?;
            self.cores = (total - self.previous) / (time - self.time);
            self.previous = total;
            self.time = time;
        }
        Ok(self.cores)
    }
}
impl Drop for Cpu {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

pub fn build_running() -> Result<bool> {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "if (Get-Process rustc -ErrorAction SilentlyContinue) { exit 9 }",
        ])
        .status()?;
    Ok(!output.success())
}
