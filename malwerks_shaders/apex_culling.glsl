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

layout (std430, set = 0, binding = 0) restrict readonly buffer InputClusters {
    BoundingCone input_cones[];
};
layout (std430, set = 0, binding = 1) restrict writeonly buffer OutputDrawCommands {
    DrawIndexedIndirectCommand output_draw_commands[];
};

layout (push_constant) uniform PC_ViewProjection {
    layout (offset = 0) vec4 CameraPosition;
    layout (offset = 16) uvec4 DebugParameters;
};

bool cone_apex_test(BoundingCone cluster) {
    return dot(normalize(cluster.cone_apex.xyz - CameraPosition.xyz), cluster.cone_axis.xyz) < cluster.cone_axis.w;
}

layout (local_size_x = 8, local_size_y = 1, local_size_z = 1) in;
void main() {
    BoundingCone input_cluster = input_cones[gl_GlobalInvocationID.x];

    bool cull_result = bool(DebugParameters.x) || input_cluster.cone_axis.w >= 1.0 || cone_apex_test(input_cluster);
    output_draw_commands[gl_GlobalInvocationID.x].instance_count = uint(cull_result);
}
