// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

fn halton(prime: u64, index: u64) -> f32 {
    let mut r = 0.0;
    let mut f = 1.0;
    let mut i = index;

    while i > 0 {
        f /= prime as f64;
        r += f * ((i % prime) as f64);
        i = ((i as f64) / (prime as f64)).floor() as u64;
    }

    r as f32
}

fn main() {
    for i in 2..10 {
        println!("[{}, {}],", halton(2, i) * 2.0 - 1.0, halton(3, i) * 2.0 - 1.0);
    }
}
