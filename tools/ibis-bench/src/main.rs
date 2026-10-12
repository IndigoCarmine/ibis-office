// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
mod analysis;
#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod metrics;
#[cfg(windows)]
mod office;
#[cfg(windows)]
mod pdf;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod report;
#[cfg(windows)]
mod screen;
#[cfg(windows)]
mod startup;
#[cfg(windows)]
mod suite;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    unsafe {
        windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        )?;
    }
    #[cfg(windows)]
    unsafe {
        // Applies only to this benchmark process, leaving the OS display scale unchanged.
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        #[cfg(windows)]
        Some("pdf-review") => {
            anyhow::ensure!(args.len() == 2, "usage: ibis-bench pdf-review pdf-app-kind");
            pdf::review_images(&std::env::current_dir()?, &args[1])?;
        }
        #[cfg(windows)]
        Some("suite") => {
            let positional: Vec<_> = args
                .iter()
                .skip(1)
                .filter(|a| a.as_str() != "--plan")
                .collect();
            anyhow::ensure!(
                positional.len() <= 2,
                "usage: ibis-bench suite [app] [scenario] [--plan]"
            );
            suite::run(
                &std::env::current_dir()?,
                positional.first().map_or("", |s| s.as_str()),
                positional.get(1).map_or("", |s| s.as_str()),
                args.iter().any(|a| a == "--plan"),
            )?;
        }
        #[cfg(windows)]
        Some("report") => report::run(&std::env::current_dir()?)?,
        #[cfg(windows)]
        Some("startup") => {
            anyhow::ensure!(args.len() == 2, "usage: ibis-bench startup app");
            startup::measure(&std::env::current_dir()?, &args[1])?;
        }
        #[cfg(windows)]
        Some("run") => {
            anyhow::ensure!(
                args.len() == 4,
                "usage: ibis-bench run app firstview|end pptx|docx|xlsx"
            );
            screen::fixture_run(&std::env::current_dir()?, &args[1], &args[2], &args[3])?;
        }
        #[cfg(windows)]
        Some("close-fixture") => {
            anyhow::ensure!(args.len() == 3, "close-fixture hwnd owned-pid");
            platform::close_fixture(args[1].parse()?, args[2].parse()?)?;
        }
        #[cfg(windows)]
        Some("measure") => {
            anyhow::ensure!(
                args.len() == 5,
                "usage: ibis-bench measure hwnd app firstview|end pptx|docx|xlsx"
            );
            screen::measure(
                &std::env::current_dir()?,
                args[1].parse()?,
                &args[2],
                &args[3],
                &args[4],
            )?;
        }
        #[cfg(windows)]
        Some("office-pdf") => {
            anyhow::ensure!(args.len() == 4, "office-pdf app source output");
            office::convert(
                &args[1],
                std::path::Path::new(&args[2]),
                std::path::Path::new(&args[3]),
            )?;
        }
        #[cfg(windows)]
        Some("pdf") => pdf::run(
            &std::env::current_dir()?,
            args.get(1).map_or("", String::as_str),
            args.get(2).map_or("", String::as_str),
        )?,
        #[cfg(windows)]
        Some("doctor") => platform::doctor(&std::env::current_dir()?)?,
        #[cfg(windows)]
        Some("windows") => println!("{}", serde_json::to_string_pretty(&platform::windows()?)?),
        #[cfg(windows)]
        Some("record") => {
            anyhow::ensure!(
                args.len() == 4,
                "usage: ibis-bench record hwnd seconds output-directory"
            );
            platform::record(
                args[1].parse()?,
                args[2].parse()?,
                std::path::Path::new(&args[3]),
            )?;
        }
        Some("analyze") => {
            anyhow::ensure!(
                args.len() == 4,
                "usage: ibis-bench analyze samples.json t0 startup|docx|pptx|end-pptx"
            );
            let samples: Vec<analysis::Sample> = serde_json::from_slice(&std::fs::read(&args[1])?)?;
            let area = match args[3].as_str() {
                "startup" => analysis::FULL,
                "pptx" => analysis::SLIDE,
                "end-pptx" => analysis::SLIDES,
                "docx" | "xlsx" => analysis::DOC,
                _ => anyhow::bail!("unknown analysis area"),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&analysis::analyze(
                    &samples,
                    args[2].parse()?,
                    area
                )?)?
            );
        }
        None | Some("help" | "--help" | "-h") => println!(
            "ibis-bench: Windows benchmark tool\n  doctor\n  windows\n  startup app\n  run app firstview|end pptx|docx|xlsx\n  pdf [app] [kind]\n  suite [app] [scenario] [--plan]\n  pdf-review pdf-app-kind\n  report\n  record hwnd seconds output-directory\n  analyze samples.json t0 startup|docx|xlsx|pptx|end-pptx"
        ),
        Some(command) => anyhow::bail!("unknown command {command}; use --help"),
    }
    Ok(())
}
