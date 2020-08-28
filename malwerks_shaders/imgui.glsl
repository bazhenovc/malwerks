// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

#ifdef VERTEX_STAGE
layout (push_constant) uniform PC {
    mat4 mvp_matrix;
};

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in uint in_color;

layout(location = 0) out vec2 vs_uv;
layout(location = 1) out vec4 vs_color;

void main() {
    vs_uv = in_uv;
    vs_color = vec4(
        (in_color >> 0 ) & 0xFF,
        (in_color >> 8 ) & 0xFF,
        (in_color >> 16) & 0xFF,
        (in_color >> 24) & 0xFF
    ) / 255.0;
    gl_Position = mvp_matrix * vec4(in_position.xy, 0.0, 1.0);
}
#endif

#ifdef FRAGMENT_STAGE
layout(set = 0, binding = 0) uniform sampler Sampler0;
layout(set = 0, binding = 1) uniform texture2D Texture0;

layout(location = 0) in vec2 vs_uv;
layout(location = 1) in vec4 vs_color;

layout(location = 0) out vec4 target0;

void main() {
    target0 = vs_color * texture(sampler2D(Texture0, Sampler0), vs_uv);
}
#endif