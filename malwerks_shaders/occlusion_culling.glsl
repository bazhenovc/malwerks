// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

struct DrawIndexedIndirectCommand {
    uint index_count;
    uint instance_count;
    uint first_index;
    int vertex_offset;
    uint first_instance;
};

layout (std430, set = 0, binding = 0) restrict readonly buffer InputDrawCommands {
    DrawIndexedIndirectCommand input_draw_commands[];
};
layout (std430, set = 0, binding = 1) restrict buffer VisibilityBuffer {
    uvec4 visibility[][2];
};
layout (std430, set = 0, binding = 2) restrict buffer DrawCommandsCount {
    uvec2 output_count;
};
layout (std430, set = 0, binding = 3) restrict writeonly buffer OutputDrawCommands {
    DrawIndexedIndirectCommand output_draw_commands[];
};

layout (local_size_x = 8, local_size_y = 1, local_size_z = 1) in;
void main() {
    if (gl_GlobalInvocationID.x < visibility.length()) {
        uvec4 visible = visibility[gl_GlobalInvocationID.x][0];
        if (bool(visible.x)) {
            uint command_index = atomicAdd(output_count.y, 1);
            output_draw_commands[command_index] = input_draw_commands[gl_GlobalInvocationID.x];
        }

        barrier();

        visibility[gl_GlobalInvocationID.x][0] = uvec4(0, 0, 0, 0);
    }
}