// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

struct BoundingCone {
    vec4 cone_apex;
    vec4 cone_axis;
};

struct DrawIndexedIndirectCommand {
    uint index_count;
    uint instance_count;
    uint first_index;
    int vertex_offset;
    uint first_instance;
};

layout (std430, set = 0, binding = 0) restrict readonly buffer InputBoundingCones {
    BoundingCone input_cones[];
};

layout (std430, set = 0, binding = 1) restrict readonly buffer InputOccluderDrawCommands {
    DrawIndexedIndirectCommand input_occluder_draw_commands[];
};
layout (std430, set = 0, binding = 2) restrict readonly buffer InputDrawCommands {
    DrawIndexedIndirectCommand input_draw_commands[];
};

layout (std430, set = 0, binding = 3) restrict buffer DrawCommandsCount {
    uvec2 output_count;
};

layout (std430, set = 0, binding = 4) restrict writeonly buffer OutputOccluderDrawCommands {
    DrawIndexedIndirectCommand output_occluder_draw_commands[];
};
layout (std430, set = 0, binding = 5) restrict writeonly buffer OutputDrawCommands {
    DrawIndexedIndirectCommand output_draw_commands[];
};

layout (push_constant) uniform PC_ViewProjection {
    layout (offset = 0) vec4 CameraPosition;
};

bool cone_apex_test(vec3 apex, vec4 axis) {
    return dot(normalize(apex - CameraPosition.xyz), axis.xyz) < axis.w;
}

layout (local_size_x = 8, local_size_y = 1, local_size_z = 1) in;
void main() {
    if (gl_GlobalInvocationID.x < input_cones.length()) {
        if (gl_GlobalInvocationID.x == 0) {
            output_count = uvec2(0, 0);
        }

        barrier();

        BoundingCone input_cluster = input_cones[gl_GlobalInvocationID.x];

        // vec3 apex = input_cluster.cone_apex.xyz;
        // uint axis_bits = floatBitsToUint(input_cluster.cone_apex.w);
        // vec4 axis = vec4(
        //     float((axis_bits >>  0) & 0xFF) / 255.0,
        //     float((axis_bits >>  8) & 0xFF) / 255.0,
        //     float((axis_bits >> 16) & 0xFF) / 255.0,
        //     float((axis_bits >> 24) & 0xFF) / 255.0
        // );

        vec3 apex = input_cluster.cone_apex.xyz;
        vec4 axis = input_cluster.cone_axis;

        bool cull_result = axis.w >= 1.0 || cone_apex_test(apex, axis);
        if (cull_result) {
            uint command_index = atomicAdd(output_count.x, 1);
            output_occluder_draw_commands[command_index] = input_occluder_draw_commands[gl_GlobalInvocationID.x];
            output_draw_commands[command_index] = input_draw_commands[gl_GlobalInvocationID.x];
        }
    }
}
