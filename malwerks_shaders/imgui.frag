// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 450

layout(set = 0, binding = 0) uniform sampler Sampler0;
layout(set = 0, binding = 1) uniform texture2D Texture0;

layout(location = 0) in vec2 vs_uv;
layout(location = 1) in vec4 vs_color;

layout(location = 0) out vec4 target0;

void main() {
    target0 = vs_color * texture(sampler2D(Texture0, Sampler0), vs_uv);
}
