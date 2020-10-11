// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

#ifdef VERTEX_STAGE
layout(location = 0) out vec2 VS_uv;

void main() {
    VS_uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(VS_uv * 2.0f + -1.0f, 0.0f, 1.0f);
}
#endif

#ifdef FRAGMENT_STAGE
layout (set = 0, binding = 0, input_attachment_index = 0) uniform usubpassInput DrawIDBuffer;
layout (std430, set = 0, binding = 1) restrict writeonly buffer OutputVisibilityBuffer {
    uvec4 output_visibility[][2];
};

layout(location = 0) in vec2 VS_uv;

void main() {
    uint draw_id = subpassLoad(DrawIDBuffer).r;
    if (draw_id != 0xFFFFFFFF) {
        output_visibility[draw_id][0] = uvec4(1, draw_id, 0, 0);
    }
}
#endif
