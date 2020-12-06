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
layout(set = 0, binding = 1) uniform texture2D SourceColorImage;
layout(set = 0, binding = 2) uniform texture2D SourceDepthImage;
layout(set = 0, binding = 3) uniform texture2D FrameImage;

layout (std140, set = 1, binding = 0) uniform PerFrame {
    mat4 ViewProjection;
    mat4 InverseViewProjection;
    mat4 ViewReprojection;
    vec4 CameraPosition;
    vec4 CameraOrientation;
    vec4 ViewportSize;
};

layout(location = 0) in vec2 VS_uv;
layout(location = 0) out vec4 Target0;

float luminance(vec3 color) {
    return dot(color, vec3(0.2125, 0.7154, 0.0721));
}

vec2 reproject_uv(mat4 view_reprojection, vec2 uv, float depth) {
    vec4 reprojection = view_reprojection * vec4(uv * 2.0 - vec2(1.0), depth, 1.0);
    reprojection /= reprojection.w;
    return reprojection.xy * 0.5 + vec2(0.5);
}

void sample_clip_min_max(texture2D tex, sampler samp, vec2 uv, vec3 source_sample,
    out vec3 clip_min, out vec3 clip_max) {
    vec3 clip_sample = vec3(0);
    clip_min = source_sample;
    clip_max = source_sample;
    
    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(-1, -1)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);

    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(-1, 0)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);

    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(-1, 1)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);

    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(0, -1)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);

    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(0, 1)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);

    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(1, -1)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);

    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(1, 0)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);

    clip_sample = textureOffset(sampler2D(tex, samp), uv, ivec2(1, 1)).rgb;
    clip_min = min(clip_sample, clip_min);
    clip_max = max(clip_sample, clip_max);
}

vec3 clip_color(vec3 clip_min, vec3 clip_max, vec3 frame) {
    vec3 center = 0.5 * (clip_max + clip_min);
    vec3 extent = 0.5 * (clip_max - clip_min);
    vec3 clip = frame - center;
    vec3 unit = abs(clip.xyz / extent);

    float max_unit = max(unit.x, max(unit.y, unit.z));
    if (max_unit > 1.0)
        return center + clip / max_unit;
    else
        return frame;
}

vec4 lanczos_weight(float x) {
    const float RADIUS = 2.0;
    const float PI = 3.1415926535897932384626433832795;
    vec4 taps = max(PI * vec4(1.0 + x, x, 1.0 - x, 2.0 - x), 1e-5);
    vec4 weight = sin(taps) * sin(taps / RADIUS) / (taps * taps);
    return weight / dot(weight, vec4(1.0));
}

vec3 sample_line_rgb(texture2D image, sampler samp, float ypos, vec4 xpos, vec4 line_taps) {
    return mat4x3(
        texture(sampler2D(image, samp), vec2(xpos.x, ypos)).rgb,
        texture(sampler2D(image, samp), vec2(xpos.y, ypos)).rgb,
        texture(sampler2D(image, samp), vec2(xpos.z, ypos)).rgb,
        texture(sampler2D(image, samp), vec2(xpos.w, ypos)).rgb) * line_taps;
}

vec3 sample_lanczos_rgb(texture2D image, sampler samp, vec2 uv) {
    vec2 stepxy = ViewportSize.zw; // 1.0 / textureSize(sampler2D(image, samp), 0)
    vec2 pos = uv + stepxy * 0.5;
    vec2 f = fract(pos / stepxy);

    vec2 xystart = (-1.5 - f) * stepxy + pos;
    vec4 xpos = vec4(
        xystart.x,
        xystart.x + stepxy.x,
        xystart.x + stepxy.x * 2.0,
        xystart.x + stepxy.x * 3.0);

    vec4 line_taps = lanczos_weight(f.x);
    vec4 column_taps = lanczos_weight(f.y);

    return mat4x3(
        sample_line_rgb(image, samp, xystart.y, xpos, line_taps),
        sample_line_rgb(image, samp, xystart.y + stepxy.y, xpos, line_taps),
        sample_line_rgb(image, samp, xystart.y + stepxy.y * 2.0, xpos, line_taps),
        sample_line_rgb(image, samp, xystart.y + stepxy.y * 3.0, xpos, line_taps)) * column_taps;
}

void main() {
    float depth_sample = texture(sampler2D(SourceDepthImage, PointSampler), VS_uv).r;
    
    vec3 source_sample = texture(sampler2D(SourceColorImage, PointSampler), VS_uv).rgb;
    vec3 clip_min = source_sample;
    vec3 clip_max = source_sample;
    sample_clip_min_max(SourceColorImage, PointSampler, VS_uv, source_sample, clip_min, clip_max);

    vec2 uv = reproject_uv(ViewReprojection, VS_uv, depth_sample);
    vec3 frame_sample = sample_lanczos_rgb(FrameImage, PointSampler, uv);

    Target0 = vec4(mix(source_sample, clip_color(clip_min, clip_max, frame_sample), 0.9), 1.0);
}
#endif
