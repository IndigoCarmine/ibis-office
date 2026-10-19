// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Turns an EMF or WMF file into SVG: `emf_svg <in.emf|in.wmf> <out.svg>`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: emf_svg <in.emf|in.wmf> <out.svg>");
        std::process::exit(2);
    };
    let data = std::fs::read(input).expect("read");
    if let Some(ibis_emf::Picture::Image(format, bytes)) = ibis_emf::emf_picture(&data) {
        eprintln!("only a bitmap: {format:?}, {} bytes", bytes.len());
    }
    let svg = ibis_emf::emf_to_svg(&data).or_else(|| ibis_emf::wmf_to_svg(&data));
    match svg {
        Some(svg) => std::fs::write(output, svg).expect("write"),
        None => {
            eprintln!("nothing drawn");
            std::process::exit(1);
        }
    }
}
