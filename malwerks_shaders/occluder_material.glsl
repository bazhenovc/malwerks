// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

#ifdef VERTEX_STAGE
layout (push_constant) uniform PC_Parameters {
    layout (offset = 0) mat4 ViewProjection;
    layout (offset = 64) uvec4 Parameters;
};

layout (location = 0) in vec3 IN_position;
layout (location = 0) out flat uint VS_draw_id;

layout (std430, set = 0, binding = 0) readonly buffer InstanceDataBuffer {
    mat4 WorldTransforms[];
};

void main() {
    mat4 world_transform = WorldTransforms[gl_InstanceIndex];
    vec3 position = (world_transform * vec4(IN_position.xyz, 1.0)).xyz;
    VS_draw_id = Parameters.x + gl_DrawID;
    gl_Position = ViewProjection * vec4(position.xyz, 1.0);
}
#endif

#ifdef FRAGMENT_STAGE
layout (location = 0) in flat uint VS_draw_id;
layout (location = 0) out uvec4 Target0;

layout (early_fragment_tests) in;
void main() {
    Target0 = uvec4(VS_draw_id, 0, 0, 0);
}
#endif
