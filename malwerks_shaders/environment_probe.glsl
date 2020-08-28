// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

#ifdef RAY_TRACING
#extension GL_NV_ray_tracing:enable
#endif

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
    vec4 position = vec4(uv * 2.0 - 1.0, 0.0, 1.0);

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

#ifdef RAY_GEN_STAGE
struct PrimaryRayPayload {
    vec4 color_and_distance;
    // vec4 normal_and_id;
};

layout (set = 0, binding = 0) uniform accelerationStructureNV TopLevelAccelerationStructure;
layout (set = 0, binding = 1, rgba32f) uniform image2D OutputImage;

layout (location = 0) rayPayloadNV PrimaryRayPayload PrimaryRay;

void main() {
    const uint MAX_RECURSION_DEPTH = 4;
    const uint RAY_FLAGS = gl_RayFlagsOpaqueNV;
    const uint CULL_MASK = 0xFF;

    vec2 pixel_center = vec2(gl_LaunchIDNV) + vec2(0.5);
    vec2 uv = pixel_center / vec2(gl_LaunchIDNV);
    vec2 uv_ndc = uv * 2.0 - vec2(1.0);
    float aspect_ratio = float(gl_LaunchIDNV.x) / float(gl_LaunchIDNV.y);

    vec3 origin = vec3(0.0, 0.0, 0.0);
    float tmin = 0.0;
    float tmax = 9999.0;
    vec3 direction = normalize(vec3(uv_ndc.x * aspect_ratio, -uv_ndc.y, 1.0));

    traceNV(
        TopLevelAccelerationStructure,
        RAY_FLAGS,
        CULL_MASK,
        0, // sbtRecordOffset
        0, // sbtRecordStride
        0, // missIndex
        origin, tmin,
        direction, tmax,
        0 // payload
    );

    imageStore(OutputImage, ivec2(gl_LaunchIDNV.xy), PrimaryRay.color_and_distance);
}
#endif
