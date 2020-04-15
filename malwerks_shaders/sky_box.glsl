// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 450 core

layout (std140, set = 0, binding = 0) uniform PerFrame {
    mat4 view_projection;
    mat4 inverse_view_projection;
    vec4 camera_position;
    vec4 camera_orientation;
};

#ifdef VERTEX_STAGE
layout (location = 0) out vec3 VS_uv;

void main() {
    vec2 uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    vec4 position = vec4(uv * 2.0f + -1.0f, 0.0f, 1.0f);

    VS_uv = (inverse_view_projection * position).xyz;
    gl_Position = position;
}

#endif

#ifdef FRAGMENT_STAGE
layout (set = 1, binding = 0) uniform sampler LinearSampler;
layout (set = 1, binding = 1) uniform textureCube SkyBox;

layout (location = 0) in vec3 VS_uv;
layout (location = 0) out vec4 Target0;

void main() {
    Target0 = texture(samplerCube(SkyBox, LinearSampler), VS_uv);
}
#endif
