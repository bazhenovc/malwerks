// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

#ifdef RAY_TRACING
#extension GL_NV_ray_tracing:enable
#extension GL_EXT_nonuniform_qualifier:enable
#endif

#include "generated://shader_prelude.glsl"

layout (std140, set = 0, binding = 0) uniform PerFrame {
    mat4 view_projection;
    mat4 inverse_view_projection;
    vec4 camera_position;
    vec4 camera_orientation;
};

#ifdef VERTEX_STAGE
layout (push_constant) uniform PC_ViewProjection {
    layout (offset = 0) mat4 ViewProjection;
};

void main() {
    vec4 position = generated_vertex_shader();
    gl_Position = ViewProjection * position;
}

#endif

#ifdef FRAGMENT_STAGE

layout (push_constant) uniform PC_MaterialInstance {
    layout (offset = 64) vec4 base_color_factor;
    layout (offset = 80) vec4 metallic_roughness_discard_unused;
    layout (offset = 96) vec4 emissive_rgb_unused;
    layout (offset = 112) vec4 unused;
};

layout (set = 2, binding = 0) uniform samplerCube IemTexture;
layout (set = 2, binding = 1) uniform samplerCube PmremTexture;
layout (set = 2, binding = 2) uniform sampler2D PrecomputedBrdf;

vec4 sample_base_color() {
    #ifdef HAS_BaseColorTexture
        vec4 color_sample = texture(BaseColorTexture, BaseColorTexture_UV) * base_color_factor;
        #ifdef HAS_AlphaDiscard
            //if (color_sample.a < metallic_roughness_discard_unused.z) {
            //    discard;
            //}
        #endif
        return color_sample;
    #else
        return base_color_factor;
    #endif
}

vec2 sample_metallic_roughness() {
    #ifdef HAS_MetallicRoughnessTexture
        return texture(MetallicRoughnessTexture, MetallicRoughnessTexture_UV).bg * metallic_roughness_discard_unused.xy;
    #else
        return metallic_roughness_discard_unused.xy;
    #endif
}

vec3 sample_normal() {
    #if !defined(HAS_VS_normal) || !defined(HAS_VS_tangent)
        vec3 ddx_pos = dFdx(VS_position);
        vec3 ddy_pos = dFdy(VS_position);
    #endif

    #ifdef HAS_VS_normal
        vec3 input_normal = normalize(VS_normal);
    #else
        vec3 input_normal = cross(ddx_pos, ddy_pos);
    #endif

    #ifdef HAS_NormalTexture
        #ifdef HAS_VS_tangent
            vec4 input_tangent = VS_tangent;
        #else
            vec3 ddx_uv = dFdx(vec3(NormalTexture_UV, 0.0));
            vec3 ddy_uv = dFdy(vec3(NormalTexture_UV, 0.0));
            vec4 input_tangent = vec4(
                (ddy_uv.t * ddx_pos - ddx_uv.t * ddy_pos) / (ddx_uv.s * ddy_uv.t - ddy_uv.s * ddx_uv.t),
                1.0
            );
            input_tangent.xyz = normalize(input_tangent.xyz - dot(input_tangent.xyz, input_normal) * input_normal);
        #endif
        vec3 normal = normalize(input_normal);
        vec3 tangent = normalize(input_tangent.xyz);
        vec3 binormal = cross(normal, tangent) * input_tangent.w;
        mat3 tbn = mat3(tangent, binormal, normal);

        vec3 normal_sample = texture(NormalTexture, NormalTexture_UV).xyz * 2.0 - 1.0;
        return normalize(tbn * normal_sample);
    #else
        return normalize(input_normal);
    #endif
}

float sample_occlusion() {
    #ifdef HAS_OcclusionTexture
        return texture(OcclusionTexture, OcclusionTexture_UV).r;
    #else
        return 1.0;
    #endif
}

vec3 sample_emissive() {
    #ifdef HAS_EmissiveTexture
        return texture(EmissiveTexture, EmissiveTexture_UV).rgb * emissive_rgb_unused.rgb;
    #else
        return emissive_rgb_unused.rgb;
    #endif
}

float specular_occlusion(float dot_nv, float occlusion, float roughness) {
    return clamp(pow(dot_nv + occlusion, roughness) - 1.0 + occlusion, 0.0, 1.0);
}

vec3 calculate_ibl(
    vec3 normal,
    vec3 view_direction,
    vec3 diffuse_color,
    vec3 specular_color,
    float metallic,
    float roughness,
    float occlusion
) {
    float dot_nv = clamp(dot(normal, view_direction), 0.0, 1.0);
    vec3 reflect_direction = normalize(reflect(-view_direction, normal));

    vec3 irradiance = texture(IemTexture, normal).rgb;
    vec3 radiance = textureLod(PmremTexture, reflect_direction, roughness * 10.0).rgb;
    vec2 brdf = texture(PrecomputedBrdf, vec2(dot_nv, roughness)).xy;
    float specular_occlusion = specular_occlusion(dot_nv, occlusion, roughness);

    vec3 diffuse_light = irradiance * diffuse_color * occlusion;
    vec3 specular_light = radiance * (specular_color * brdf.x + brdf.y) * specular_occlusion;

    return diffuse_light + specular_light;
}

layout (location = 0) out vec4 Target0;

void main() {
    vec4 base_color = sample_base_color();
    vec2 metallic_roughness = sample_metallic_roughness();
    vec3 normal = sample_normal();
    float occlusion = sample_occlusion();
    vec3 emissive = sample_emissive();

    float metallic = metallic_roughness.r;
    float roughness = metallic_roughness.g;

    vec3 view_direction = normalize(camera_position.xyz - VS_position);

    const vec3 F0 = vec3(0.04);
    vec3 diffuse_color = base_color.rgb * (vec3(1.0) - F0) * (1.0 - metallic);
    vec3 specular_color = mix(F0, base_color.rgb, metallic);

    vec3 ibl = calculate_ibl(
        normal,
        view_direction,
        diffuse_color,
        specular_color,
        metallic,
        roughness,
        occlusion
    );

    vec3 final_color = ibl + emissive;
    Target0 = vec4(final_color, 1.0);
}

#endif

#ifdef RAY_CLOSEST_HIT_STAGE
layout (set = 0, binding = 0) uniform sampler2D MaterialTextures[];

// layout (set = 1, binding = 0) uniform samplerCube IemTexture;
// layout (set = 1, binding = 1) uniform samplerCube PmremTexture;
// layout (set = 1, binding = 2) uniform sampler2D PrecomputedBrdf;

struct PrimaryRayPayload {
    vec4 color_and_distance;
    // vec4 normal_and_id;
};

layout (location = 0) rayPayloadNV PrimaryRayPayload PrimaryRay;
hitAttributeNV vec3 HitAttributes;

void main() {
    vec3 barycentrics = vec3(1.0 - HitAttributes.x - HitAttributes.y, HitAttributes.x, HitAttributes.y);
    PrimaryRay.color_and_distance = vec4(barycentrics * factor, gl_HitTNV);
    // PrimaryRay.normal_and_id = vec4(0.0, 0.0, 1.0, intBitsToFloat(0));
}

#endif
