// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use std::env;
use image::GenericImageView;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: inspect_crop <crop.png> [more.png...]");
        return;
    }

    // `--diff a.png b.png`: the differing pixels of two same-size images, by
    // 64-pixel tiles, to see where two renderings part.
    if args[1] == "--diff" && args.len() == 4 {
        let a = image::open(&args[2]).expect("a").to_rgba8();
        let b = image::open(&args[3]).expect("b").to_rgba8();
        println!("a={:?} b={:?}", a.dimensions(), b.dimensions());
        let (w, h) = (a.width().min(b.width()), a.height().min(b.height()));
        let mut tiles = std::collections::BTreeMap::new();
        for y in 0..h {
            for x in 0..w {
                let (p, q) = (a.get_pixel(x, y), b.get_pixel(x, y));
                if (0..3).any(|i| (p[i] as i32 - q[i] as i32).abs() > 24) {
                    let e = tiles.entry((y / 64, x / 64)).or_insert((0u32, u32::MAX, 0u32, u32::MAX, 0u32));
                    e.0 += 1; e.1 = e.1.min(x); e.2 = e.2.max(x); e.3 = e.3.min(y); e.4 = e.4.max(y);
                }
            }
        }
        for ((ty, tx), (n, x0, x1, y0, y1)) in &tiles {
            println!("tile r{ty} c{tx}: {n} px, x={x0}..{x1} y={y0}..{y1}");
        }
        return;
    }
    if args[1] == "--row" && args.len() == 4 {
        let img = image::open(&args[2]).expect("crop").to_rgba8();
        let target_y: u32 = args[3].parse().expect("y");
        let (total_w, h) = img.dimensions();
        let w = (total_w.saturating_sub(8)) / 3;
        let ref_x0 = 0;
        let cand_x0 = w + 4;
        println!("Panel w={w}, h={h}, y={target_y}");
        for x in 0..w {
            let p = img.get_pixel(ref_x0 + x, target_y);
            let q = img.get_pixel(cand_x0 + x, target_y);
            if p[0] < 240 || p[1] < 240 || p[2] < 240 || q[0] < 240 || q[1] < 240 || q[2] < 240 {
                println!("  x={x:3}: Ref={:?}  Cand={:?}", p, q);
            }
        }
        return;
    }
    if args[1] == "--panel-diff" && args.len() == 3 {
        let img = image::open(&args[2]).expect("crop").to_rgba8();
        let (total_w, h) = img.dimensions();
        let w = (total_w.saturating_sub(8)) / 3;
        let ref_x0 = 0;
        let cand_x0 = w + 4;
        println!("Panel w={w}, h={h}");
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(ref_x0 + x, y);
                let q = img.get_pixel(cand_x0 + x, y);
                let dist = (0..3).map(|i| (p[i] as i32 - q[i] as i32).abs()).max().unwrap_or(0);
                if dist > 24 {
                    // Check if nearby has match within r=1
                    let mut has_near = false;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let nx = x as i32 + dx;
                            let ny = y as i32 + dy;
                            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                let nq = img.get_pixel(cand_x0 + nx as u32, ny as u32);
                                if (0..3).map(|i| (p[i] as i32 - nq[i] as i32).abs()).max().unwrap_or(0) <= 24 {
                                    has_near = true;
                                    break;
                                }
                            }
                        }
                        if has_near { break; }
                    }
                    if !has_near {
                        println!("DIFF at ({x}, {y}): Ref={:?}, Cand={:?}", p, q);
                    }
                }
            }
        }
        return;
    }
    if args[1] == "--bands" && args.len() == 3 {
        let img = image::open(&args[2]).expect("crop").to_rgba8();
        let (total_w, h) = img.dimensions();
        let w = (total_w.saturating_sub(8)) / 3;
        for (label, x0) in [("Ref", 0), ("Cand", w + 4)] {
            println!("  {label}:");
            let mut dark_rows = Vec::new();
            for y in 0..h {
                let cnt = (0..w).filter(|&x| {
                    let p = img.get_pixel(x0 + x, y);
                    p[0] < 128 && p[1] < 128 && p[2] < 128
                }).count();
                if cnt > 0 {
                    dark_rows.push((y, cnt));
                }
            }
            let mut bands = Vec::new();
            let mut cur: Vec<(u32, usize)> = Vec::new();
            for (y, cnt) in dark_rows {
                if cur.is_empty() || y == cur.last().unwrap().0 + 1 {
                    cur.push((y, cnt));
                } else {
                    bands.push((cur.first().unwrap().0, cur.last().unwrap().0, cur.iter().map(|c| c.1).sum::<usize>()));
                    cur = vec![(y, cnt)];
                }
            }
            if !cur.is_empty() {
                bands.push((cur.first().unwrap().0, cur.last().unwrap().0, cur.iter().map(|c| c.1).sum::<usize>()));
            }
            for b in bands {
                println!("    y={}..{} ({} px)", b.0, b.1, b.2);
            }
        }
        return;
    }
    // `--zoom N in.png out.png`: enlarge by N with nearest pixels, to look at.
    if args[1] == "--zoom" && args.len() == 5 {
        let n: u32 = args[2].parse().expect("N");
        let img = image::open(&args[3]).expect("in").to_rgba8();
        let (w, h) = img.dimensions();
        image::imageops::resize(&img, w * n, h * n, image::imageops::FilterType::Nearest).save(&args[4]).expect("out");
        return;
    }
    for path in &args[1..] {
        let img = match image::open(path) {
            Ok(img) => img,
            Err(e) => {
                eprintln!("Failed to open {path}: {e}");
                continue;
            }
        };

        let (total_w, h) = img.dimensions();
        let w = (total_w.saturating_sub(8)) / 3;
        println!("=== {} (w={}, h={}) ===", path, w, h);

        let quiet = env::var("BBOX_ONLY").is_ok();
        let analyze_panel = |x0: u32, label: &str| {
            // Bounding box of the non-white pixels.
            let (mut x_min, mut x_max, mut y_min, mut y_max) = (u32::MAX, 0, u32::MAX, 0);
            for y in 0..h {
                for x in 0..w {
                    let p = img.get_pixel(x0 + x, y);
                    if p[0] < 240 || p[1] < 240 || p[2] < 240 {
                        x_min = x_min.min(x); x_max = x_max.max(x); y_min = y_min.min(y); y_max = y_max.max(y);
                    }
                }
            }
            println!("  [{label}] bbox x={x_min}..{x_max} y={y_min}..{y_max}");
            if quiet { return; }
            for y in 0..h {
                let mut non_white = Vec::new();
                for x in 0..w {
                    let p = img.get_pixel(x0 + x, y);
                    if p[0] < 240 || p[1] < 240 || p[2] < 240 {
                        non_white.push((x, p));
                    }
                }
                if !non_white.is_empty() {
                    let min_x = non_white.first().unwrap().0;
                    let max_x = non_white.last().unwrap().0;
                    let span = max_x - min_x + 1;
                    let count = non_white.len();
                    // Detect if this line looks like an underline (continuous wide line of black/dark pixels)
                    let is_underline = span > 40 && count as f32 / span as f32 > 0.85;
                    let tag = if is_underline { " [UNDERLINE?]" } else { "" };
                    println!("    y={:2}: count={:3}, span={:3}..{:3} ({:3}px){tag} sample={:?}",
                        y, count, min_x, max_x, span, non_white[0].1);
                }
            }
        };

        analyze_panel(0, "Ref");
        analyze_panel(w + 4, "Cand");
        analyze_panel((w + 4) * 2, "Diff");
        println!();
    }
}
