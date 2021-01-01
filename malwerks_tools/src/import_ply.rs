// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_ply::*;

#[logging_timer::time("info")]
fn load_ply_from_file(ply_path: &str) -> Ply {
    parse_ply(
        &mut std::fs::OpenOptions::new()
            .read(true)
            .open(ply_path)
            .expect("failed to open input file"),
    )
    .expect("failed to parse ply")
}

fn main() {
    if std::env::var("CARGO_MANIFEST_DIR").is_ok() {
        std::env::set_var("RUST_LOG", "info");
    }

    pretty_env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let ply_path = &args[1];

    let ply = load_ply_from_file(&ply_path);
    log::info!("ply_header: {:?}", &ply.ply_header);
    log::info!("used memory: {} bytes", ply.ply_data.compute_used_memory());
}
