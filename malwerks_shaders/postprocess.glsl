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
layout(set = 0, binding = 0) uniform sampler PointSampler;
layout(set = 0, binding = 1) uniform sampler LinearSampler;
layout(set = 0, binding = 2) uniform texture2D FrameBuffer;

layout(location = 0) in vec2 VS_uv;
layout(location = 0) out vec4 Target0;

// Hejl Richard tone map
// http://filmicworlds.com/blog/filmic-tonemapping-operators/
vec3 tone_map(vec3 hdr)
{
    hdr = max(vec3(0.0), hdr - vec3(0.004));
    return vec3(hdr * (6.2 * hdr + .5)) / (hdr * (6.2 * hdr + 1.7) + 0.06);
}

void main() {
    vec3 frame_sample = texture(sampler2D(FrameBuffer, PointSampler), VS_uv).rgb;
    Target0 = vec4(tone_map(frame_sample), 1.0);
}
#endif
