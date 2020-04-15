// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

extern crate depgraph;
extern crate shaderc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    std::fs::create_dir_all(format!("{}/shaders", &out_dir))?;

    let mut graph = depgraph::DepGraphBuilder::new();
    for item in std::fs::read_dir("../malwerks_shaders")? {
        let item = item?;

        if item.file_type()?.is_file() {
            let file_path = item.path();

            let shader_type = file_path
                .extension()
                .and_then(|ext| match ext.to_string_lossy().as_ref() {
                    // regular
                    "vert" => Some(shaderc::ShaderKind::Vertex),
                    "frag" => Some(shaderc::ShaderKind::Fragment),

                    // tessellation
                    "tesc" => Some(shaderc::ShaderKind::TessControl),
                    "tese" => Some(shaderc::ShaderKind::TessEvaluation),

                    // ray-tracing
                    "rgen" => Some(shaderc::ShaderKind::RayGeneration),
                    "miss" => Some(shaderc::ShaderKind::Miss),
                    "rchit" => Some(shaderc::ShaderKind::ClosestHit),
                    "rahit" => Some(shaderc::ShaderKind::AnyHit),

                    // mesh shaders
                    "task" => Some(shaderc::ShaderKind::Task),
                    "mesh" => Some(shaderc::ShaderKind::Mesh),

                    // compute
                    "comp" => Some(shaderc::ShaderKind::Compute),

                    // don't use it
                    "geom" => Some(shaderc::ShaderKind::Geometry),
                    _ => None,
                });

            if let Some(shader_type) = shader_type {
                let out_path = format!(
                    "{}/shaders/{}.spv",
                    &out_dir,
                    file_path.file_name().unwrap().to_string_lossy()
                );

                graph = graph.add_rule(out_path, &[&file_path], move |out, deps| {
                    println!("compiling GLSL: {:?} -> {:?}", deps[0], out);

                    let source = std::fs::read_to_string(deps[0]).unwrap();

                    let mut compile_options = shaderc::CompileOptions::new().unwrap();
                    compile_options.set_source_language(shaderc::SourceLanguage::GLSL);
                    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
                    compile_options.set_warnings_as_errors();

                    let mut compiler = shaderc::Compiler::new().unwrap();
                    let spirv_binary = compiler
                        .compile_into_spirv(
                            &source,
                            shader_type,
                            &out.to_string_lossy(),
                            "main",
                            Some(&compile_options),
                        )
                        .unwrap();
                    std::fs::write(out, spirv_binary.as_binary_u8()).unwrap();

                    Ok(())
                });
            }
        }
    }

    graph.build().unwrap().make(depgraph::MakeParams::None).unwrap();

    Ok(())
}
