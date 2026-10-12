// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Sequential suite runner. Success does not imply visual review.
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Stdio},
};

const APPS: [&str; 6] = [
    "ibisoffice",
    "msoffice-powerpoint",
    "msoffice-word",
    "msoffice-excel",
    "libreoffice",
    "onlyoffice",
];

fn kinds(app: &str) -> Vec<&'static str> {
    match app {
        "msoffice-powerpoint" => vec!["pptx"],
        "msoffice-word" => vec!["docx"],
        "msoffice-excel" => vec!["xlsx"],
        _ => vec!["pptx", "docx", "xlsx"],
    }
}

pub fn jobs(app_filter: &str, scenario_filter: &str) -> Result<Vec<(String, Vec<String>)>> {
    anyhow::ensure!(
        scenario_filter.is_empty()
            || ["startup", "firstview", "end", "pdf"].contains(&scenario_filter),
        "unknown suite scenario"
    );
    anyhow::ensure!(
        app_filter.is_empty() || APPS.iter().any(|a| a.contains(app_filter)),
        "unknown suite app"
    );
    let mut jobs = Vec::new();
    for scenario in ["startup", "firstview", "end", "pdf"] {
        if !scenario_filter.is_empty() && scenario != scenario_filter {
            continue;
        }
        for app in APPS {
            if !app_filter.is_empty() && !app.contains(app_filter) {
                continue;
            }
            if scenario == "startup" {
                jobs.push((format!("startup-{app}"), vec!["startup".into(), app.into()]));
            } else {
                for kind in kinds(app) {
                    let args = if scenario == "pdf" {
                        vec!["pdf".into(), app.into(), kind.into()]
                    } else {
                        vec!["run".into(), app.into(), scenario.into(), kind.into()]
                    };
                    jobs.push((format!("{scenario}-{app}-{kind}"), args));
                }
            }
        }
    }
    Ok(jobs)
}

pub fn run(root: &Path, app_filter: &str, scenario_filter: &str, plan_only: bool) -> Result<()> {
    let jobs = jobs(app_filter, scenario_filter)?;
    if plan_only {
        for (tag, args) in &jobs {
            println!("{tag}: ibis-bench {}", args.join(" "));
        }
        println!("{} sequential measurements; no apps launched", jobs.len());
        return Ok(());
    }
    anyhow::ensure!(
        !crate::metrics::build_running()?,
        "build running; suite not started"
    );
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let work = root
        .join("target/benchmark-windows")
        .join(format!("suite-{stamp}"));
    let runs = root.join("tasks/benchmark/runs-windows");
    std::fs::create_dir_all(&work)?;
    std::fs::create_dir_all(&runs)?;
    environment(&runs, stamp)?;
    std::fs::copy(runs.join("environment.json"), work.join("environment.json"))?;
    let mut completed = Vec::<Value>::new();
    for (tag, args) in jobs {
        let path = runs.join(format!("{tag}.json"));
        let old = std::fs::read(&path).ok();
        if let Some(bytes) = &old {
            std::fs::write(work.join(format!("previous-{tag}.json")), bytes)?;
        }
        println!("suite: {tag}");
        let log_path = work.join(format!("{tag}.log"));
        let log = std::fs::File::create(&log_path)?;
        let status = Command::new(std::env::current_exe()?)
            .current_dir(root)
            .args(&args)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .status()?;
        let new = std::fs::read(&path).ok();
        let fresh = new.is_some() && new != old;
        let ok = status.success() && fresh && !crate::metrics::build_running()?;
        completed.push(serde_json::json!({"tag":tag,"exit_code":status.code(),"fresh_result":fresh,"ok":ok,"log":log_path}));
        std::fs::write(
            work.join("suite.json"),
            serde_json::to_vec_pretty(&completed)?,
        )?;
        if fresh {
            let mut result: Value = serde_json::from_slice(&new.context("missing run result")?)?;
            result["formal"] = ok.into();
            result["suite_exit_ok"] = status.success().into();
            result["reviewed"] = false.into();
            result["suite_id"] = stamp.into();
            result["status"] = if ok {
                "measured; image/PDF review pending"
            } else {
                "failed; see suite log"
            }
            .into();
            std::fs::write(&path, serde_json::to_vec_pretty(&result)?)?;
        }
        anyhow::ensure!(
            ok,
            "suite stopped at {tag}; inspect {}; existing processes left untouched",
            log_path.display()
        );
    }
    println!(
        "suite finished; {} results need review; logs {}",
        completed.len(),
        work.display()
    );
    Ok(())
}

fn environment(runs: &Path, recorded_at: u64) -> Result<()> {
    // Product and hardware metadata only: no host name, user name, serials, or keys.
    let script = "$os=Get-CimInstance Win32_OperatingSystem; $cpu=Get-CimInstance Win32_Processor; $paths=@('C:\\Program Files\\Microsoft Office\\root\\Office16\\WINWORD.EXE','C:\\Program Files\\LibreOffice\\program\\soffice.exe','C:\\Program Files\\ONLYOFFICE\\DesktopEditors\\DesktopEditors.exe'); $versions=@{}; foreach($p in $paths){if(Test-Path -LiteralPath $p){$versions[[IO.Path]::GetFileName($p)]=(Get-Item -LiteralPath $p).VersionInfo.FileVersion}}; @{os=$os.Caption; os_version=$os.Version; cpu=@($cpu.Name); memory_bytes=$os.TotalVisibleMemorySize*1024; versions=$versions}|ConvertTo-Json -Depth 5 -Compress";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()?;
    anyhow::ensure!(output.status.success(), "environment query failed");
    let mut value: Value = serde_json::from_slice(&output.stdout)?;
    value["recorded_at_unix"] = recorded_at.into();
    value["window_physical_pixels"] = serde_json::json!([1440, 900]);
    unsafe {
        use windows::Win32::UI::{HiDpi::GetDpiForSystem, WindowsAndMessaging::*};
        value["display_pixels"] =
            serde_json::json!([GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)]);
        value["system_dpi"] = GetDpiForSystem().into();
    }
    std::fs::write(
        runs.join("environment.json"),
        serde_json::to_vec_pretty(&value)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suite_covers_all_42_measurements_with_correct_office_apps() {
        let plan = jobs("", "").unwrap();
        assert_eq!(plan.len(), 42);
        assert_eq!(
            plan.iter()
                .filter(|(tag, _)| tag.starts_with("startup-"))
                .count(),
            6
        );
        assert_eq!(
            plan.iter()
                .filter(|(tag, _)| tag.starts_with("pdf-"))
                .count(),
            12
        );
        assert!(plan.iter().any(|(tag, _)| tag == "end-msoffice-excel-xlsx"));
        assert!(!plan.iter().any(|(tag, _)| tag == "end-msoffice-word-xlsx"));
        assert_eq!(jobs("libreoffice", "end").unwrap().len(), 3);
        assert!(jobs("", "typo").is_err());
    }
}
