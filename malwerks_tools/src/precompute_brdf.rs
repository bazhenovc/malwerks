// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_dds::*;
use ultraviolet::vec::*;

// http://holger.dammertz.org/stuff/notes_HammersleyOnHemisphere.html
#[allow(clippy::excessive_precision)]
fn radical_inverse_vdc(bits: u32) -> f32 {
    let mut bits = bits;
    bits = (bits << 16u32) | (bits >> 16u32);
    bits = ((bits & 0x5555_5555u32) << 1u32) | ((bits & 0xAAAA_AAAAu32) >> 1u32);
    bits = ((bits & 0x3333_3333u32) << 2u32) | ((bits & 0xCCCC_CCCCu32) >> 2u32);
    bits = ((bits & 0x0F0F_0F0Fu32) << 4u32) | ((bits & 0xF0F0_F0F0u32) >> 4u32);
    bits = ((bits & 0x00FF_00FFu32) << 8u32) | ((bits & 0xFF00_FF00u32) >> 8u32);
    (bits as f32) * 2.328_306_436_538_696_3e-10 // / 0x100000000
}

fn hammersley(i: u32, n: u32) -> Vec2 {
    Vec2::new((i as f32) / (n as f32), radical_inverse_vdc(i))
}

fn ggx_importance_sample(xi: Vec2, n: Vec3, roughness: f32) -> Vec3 {
    let a = roughness * roughness;

    let phi = 2.0 * std::f32::consts::PI * xi.x;
    let cos_theta = ((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y)).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();

    let h = Vec3::new(phi.cos() * sin_theta, phi.sin() * sin_theta, cos_theta);
    let up = if n.z.abs() < 0.999 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };

    let tangent = up.cross(n).normalized();
    let bitangent = n.cross(tangent).normalized();

    (tangent * h.x + bitangent * h.y + n * h.z).normalized()
}

fn ggx_geometry_schlick(dot_nv: f32, roughness: f32) -> f32 {
    let a = roughness;
    let k = (a * a) / 2.0;

    let nom = dot_nv;
    let denom = dot_nv * (1.0 - k) + k;

    nom / denom
}

fn geometry_smith(n: Vec3, v: Vec3, l: Vec3, roughness: f32) -> f32 {
    let dot_nv = n.dot(v).max(0.0);
    let dot_nl = n.dot(l).max(0.0);
    let ggx2 = ggx_geometry_schlick(dot_nv, roughness);
    let ggx1 = ggx_geometry_schlick(dot_nl, roughness);

    ggx1 * ggx2
}

fn integrate_brdf(dot_nv: f32, roughness: f32) -> Vec2 {
    let mut result_x = 0.0;
    let mut result_y = 0.0;

    let view = Vec3::new((1.0 - dot_nv * dot_nv).sqrt(), 0.0, dot_nv);
    let normal = Vec3::new(0.0, 0.0, 1.0);

    const SAMPLE_COUNT: u32 = 32768;
    for i in 0..SAMPLE_COUNT {
        let xi = hammersley(i, SAMPLE_COUNT);
        let h = ggx_importance_sample(xi, normal, roughness);
        let l = (2.0 * view.dot(h) * h - view).normalized();

        let dot_nl = l.z.max(0.0);
        let dot_nh = h.z.max(0.0);
        let dot_vh = view.dot(h).max(0.0);

        if dot_nl > 0.0 {
            let g = geometry_smith(normal, view, l, roughness);
            let vis = (g * dot_vh) / (dot_nh * dot_nv);
            let fc = (1.0 - dot_vh).powf(5.0);

            result_x += (1.0 - fc) * vis;
            result_y += fc * vis;
        }
    }

    result_x /= SAMPLE_COUNT as f32;
    result_y /= SAMPLE_COUNT as f32;

    Vec2::new(result_x, result_y)
}

const IMAGE_WIDTH: usize = 512;
const IMAGE_HEIGHT: usize = 512;
const TILE_WIDTH: usize = 32;
const TILE_HEIGHT: usize = 32;
const TILE_COUNT_X: usize = IMAGE_WIDTH / TILE_WIDTH;
const TILE_COUNT_Y: usize = IMAGE_HEIGHT / TILE_HEIGHT;

struct Tile {
    x: usize,
    y: usize,
    pixels: Vec<Vec2>,
}

impl Tile {
    fn new(x: usize, y: usize) -> Self {
        let mut pixels = Vec::with_capacity(TILE_WIDTH * TILE_HEIGHT);
        pixels.resize(pixels.capacity(), Vec2::new(0.0, 0.0));

        Self { x, y, pixels }
    }

    fn process(&mut self) {
        let pixel_size_x = 1.0 / (IMAGE_WIDTH as f32);
        let pixel_size_y = 1.0 / (IMAGE_HEIGHT as f32);
        let half_pixel_size_x = pixel_size_x * 0.5;
        let half_pixel_size_y = pixel_size_y * 0.5;
        for y in 0..TILE_HEIGHT {
            for x in 0..TILE_WIDTH {
                let image_x = self.x + x;
                let image_y = self.y + y;

                let uv_x = (image_x as f32) * pixel_size_x + half_pixel_size_x;
                let uv_y = (image_y as f32) * pixel_size_y + half_pixel_size_y;
                //let uv_x = 1.0 - ((image_x as f32) * pixel_size_x + half_pixel_size_x);
                //let uv_y = 1.0 - ((image_y as f32) * pixel_size_y + half_pixel_size_y);

                self.pixels[y * TILE_WIDTH + x] = integrate_brdf(uv_x, uv_y);
            }
        }
    }
}

fn main() {
    use rayon::prelude::*;

    pretty_env_logger::init();
    log::info!(
        "brdf: {}x{}, {} {}x{} tiles",
        IMAGE_WIDTH,
        IMAGE_HEIGHT,
        TILE_COUNT_X * TILE_COUNT_Y,
        TILE_WIDTH,
        TILE_HEIGHT
    );

    let mut tiles = Vec::with_capacity(TILE_COUNT_X * TILE_COUNT_Y);
    for y in 0..TILE_COUNT_Y {
        for x in 0..TILE_COUNT_X {
            tiles.push(Tile::new(x * TILE_WIDTH, y * TILE_HEIGHT));
        }
    }

    let progress = indicatif::ProgressBar::new(tiles.len() as _);
    tiles.par_iter_mut().for_each(|tile| {
        tile.process();
        progress.inc(1);
    });
    progress.finish_and_clear();

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Pixel(f32, f32);

    let mut final_pixels = Vec::with_capacity(IMAGE_WIDTH * IMAGE_HEIGHT);
    final_pixels.resize(IMAGE_WIDTH * IMAGE_HEIGHT, Pixel(1.0, 1.0));

    for tile_y in 0..TILE_COUNT_Y {
        for tile_x in 0..TILE_COUNT_X {
            let tile = &tiles[tile_y * TILE_COUNT_X + tile_x];

            for y in 0..TILE_HEIGHT {
                for x in 0..TILE_WIDTH {
                    let tile_pixel = tile.pixels[y * TILE_WIDTH + x];

                    let uv_x = tile.x + x;
                    let uv_y = tile.y + y;
                    final_pixels[uv_y * IMAGE_WIDTH + uv_x] = Pixel(tile_pixel.x, tile_pixel.y);
                }
            }
        }
    }

    let mut scratch_image = ScratchImage::new(
        IMAGE_WIDTH as _,
        IMAGE_HEIGHT as _,
        1,
        1,
        1,
        DXGI_FORMAT_R32G32_FLOAT,
        false,
    );
    scratch_image.as_typed_slice_mut().copy_from_slice(&final_pixels);
    scratch_image.save_to_file(&std::path::Path::new("brdf.dds"));
}
