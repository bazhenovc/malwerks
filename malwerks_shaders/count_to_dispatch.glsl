// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#version 460 core

struct DispatchIndirectCommand {
    uint x;
    uint y;
    uint z;
};

layout (std430, set = 0, binding = 0) restrict readonly buffer DrawCommandsCount {
    uvec2 input_counts[];
};
layout (std430, set = 0, binding = 1) restrict writeonly buffer OutputDispatchCommands {
    DispatchIndirectCommand output_dispatch_commands[];
};

layout (local_size_x = 1, local_size_y = 1, local_size_z = 1) in;
void main() {
	output_dispatch_commands[gl_GlobalInvocationID.x] = DispatchIndirectCommand(
		(input_counts[gl_GlobalInvocationID.x].x + 8) / 8,
		1,
		1
	);
}
