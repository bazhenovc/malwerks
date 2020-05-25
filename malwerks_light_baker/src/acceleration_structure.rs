// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;

use ash::vk;

pub struct AccelerationStructure {
    top_level_acceleration_structure: InternalAccelerationStructure,
    bottom_level_acceleration_structure: InternalAccelerationStructure,

    world_instances: Vec<vk::AccelerationStructureInstanceNV>,
    _world_geometries: Vec<vk::GeometryNV>,
}

impl AccelerationStructure {
    pub fn new(render_world: &RenderWorld, factory: &mut DeviceFactory) -> Self {
        let top_level_acceleration_structure_info = vk::AccelerationStructureInfoNV::builder()
            .ty(vk::AccelerationStructureTypeNV::TOP_LEVEL_NV)
            .flags(vk::BuildAccelerationStructureFlagsNV::PREFER_FAST_TRACE)
            .instance_count(render_world.get_instance_count() as _)
            .build();
        let top_level_acceleration_structure = factory.create_acceleration_structure_nv(
            &vk::AccelerationStructureCreateInfoNV::builder()
                .info(top_level_acceleration_structure_info)
                .build(),
        );

        let world_geometries = render_world.create_geometries_nv();
        let bottom_level_acceleration_structure_info = vk::AccelerationStructureInfoNV::builder()
            .ty(vk::AccelerationStructureTypeNV::BOTTOM_LEVEL_NV)
            .flags(vk::BuildAccelerationStructureFlagsNV::PREFER_FAST_TRACE)
            .geometries(&world_geometries)
            .build();
        let bottom_level_acceleration_structure = factory.create_acceleration_structure_nv(
            &vk::AccelerationStructureCreateInfoNV::builder()
                .info(bottom_level_acceleration_structure_info)
                .build(),
        );

        let memory_types = [
            vk::AccelerationStructureMemoryRequirementsTypeNV::OBJECT_NV,
            vk::AccelerationStructureMemoryRequirementsTypeNV::BUILD_SCRATCH_NV,
            vk::AccelerationStructureMemoryRequirementsTypeNV::UPDATE_SCRATCH_NV,
        ];

        let memory_requirements: Vec<_> = memory_types
            .iter()
            .map(|ty| {
                let tlas_memory_requirements = factory.get_acceleration_structure_memory_requirements_nv(
                    &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                        .ty(*ty)
                        .acceleration_structure(top_level_acceleration_structure)
                        .build(),
                );
                let blas_memory_requirements = factory.get_acceleration_structure_memory_requirements_nv(
                    &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                        .ty(*ty)
                        .acceleration_structure(bottom_level_acceleration_structure)
                        .build(),
                );

                log::info!("TLAS {:?} {:?}", ty, tlas_memory_requirements.memory_requirements);
                log::info!("BLAS {:?} {:?}", ty, blas_memory_requirements.memory_requirements);

                (tlas_memory_requirements, blas_memory_requirements)
            })
            .collect();

        let top_level_object_memory = factory.allocate_heap_memory(
            &memory_requirements[0].0.memory_requirements,
            &vk_mem::AllocationCreateInfo::default(),
        );
        let bottom_level_object_memory = factory.allocate_heap_memory(
            &memory_requirements[0].1.memory_requirements,
            &vk_mem::AllocationCreateInfo::default(),
        );

        factory.bind_acceleration_structure_memory_nv(&[
            vk::BindAccelerationStructureMemoryInfoNV::builder()
                .acceleration_structure(top_level_acceleration_structure)
                .memory(top_level_object_memory.0.get_device_memory())
                .memory_offset(top_level_object_memory.0.get_offset() as _)
                .build(),
            vk::BindAccelerationStructureMemoryInfoNV::builder()
                .acceleration_structure(bottom_level_acceleration_structure)
                .memory(bottom_level_object_memory.0.get_device_memory())
                .memory_offset(bottom_level_object_memory.0.get_offset() as _)
                .build(),
        ]);
        let top_level_acceleration_structure_handle =
            factory.get_acceleration_structure_handle_nv(top_level_acceleration_structure);
        let bottom_level_acceleration_structure_handle =
            factory.get_acceleration_structure_handle_nv(bottom_level_acceleration_structure);

        let world_instances = render_world.create_instances_nv(
            0,
            0,
            vk::GeometryInstanceFlagsNV::default(),
            top_level_acceleration_structure_handle,
        );

        Self {
            top_level_acceleration_structure: InternalAccelerationStructure {
                acceleration_structure: top_level_acceleration_structure,
                acceleration_structure_info: top_level_acceleration_structure_info,
                _acceleration_structure_handle: top_level_acceleration_structure_handle,
                object_memory: top_level_object_memory,
                build_memory_requirements: memory_requirements[1].0.memory_requirements,
                _update_memory_requirements: memory_requirements[2].0.memory_requirements,
            },
            bottom_level_acceleration_structure: InternalAccelerationStructure {
                acceleration_structure: bottom_level_acceleration_structure,
                acceleration_structure_info: bottom_level_acceleration_structure_info,
                _acceleration_structure_handle: bottom_level_acceleration_structure_handle,
                object_memory: bottom_level_object_memory,
                build_memory_requirements: memory_requirements[1].1.memory_requirements,
                _update_memory_requirements: memory_requirements[2].1.memory_requirements,
            },
            world_instances,
            _world_geometries: world_geometries,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.top_level_acceleration_structure.destroy(factory);
        self.bottom_level_acceleration_structure.destroy(factory);
    }

    pub fn build(&mut self, command_buffer: &mut CommandBuffer, factory: &mut DeviceFactory, queue: &mut DeviceQueue) {
        let top_level_scratch_buffer = factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size(self.top_level_acceleration_structure.build_memory_requirements.size)
                .usage(vk::BufferUsageFlags::RAY_TRACING_NV)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                memory_type_bits: self
                    .top_level_acceleration_structure
                    .build_memory_requirements
                    .memory_type_bits,
                ..Default::default()
            },
        );
        let bottom_level_scratch_buffer = factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size(self.bottom_level_acceleration_structure.build_memory_requirements.size)
                .usage(vk::BufferUsageFlags::RAY_TRACING_NV)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                memory_type_bits: self
                    .bottom_level_acceleration_structure
                    .build_memory_requirements
                    .memory_type_bits,
                ..Default::default()
            },
        );

        let geometry_instance_buffer = factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size((self.world_instances.len() * std::mem::size_of::<vk::AccelerationStructureInstanceNV>()) as _)
                .usage(vk::BufferUsageFlags::RAY_TRACING_NV | vk::BufferUsageFlags::TRANSFER_DST)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            },
        );
        {
            let geometry_slice = unsafe {
                std::slice::from_raw_parts(
                    self.world_instances.as_ptr() as *const u8,
                    self.world_instances.len() * std::mem::size_of::<vk::AccelerationStructureInstanceNV>(),
                )
            };

            let mut upload_batch = UploadBatch::new(command_buffer);
            upload_batch.upload_buffer_memory(
                vk::PipelineStageFlags::RAY_TRACING_SHADER_NV,
                &geometry_instance_buffer,
                &geometry_slice,
                0,
                factory,
            );
            upload_batch.flush(factory, queue);
        }

        command_buffer.reset();
        command_buffer.begin(
            &vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                .build(),
        );
        command_buffer.build_acceleration_structure_nv(
            &self.top_level_acceleration_structure.acceleration_structure_info,
            geometry_instance_buffer.0,
            0,
            false,
            self.top_level_acceleration_structure.acceleration_structure,
            vk::AccelerationStructureNV::null(),
            top_level_scratch_buffer.0,
            0,
        );
        command_buffer.build_acceleration_structure_nv(
            &self.bottom_level_acceleration_structure.acceleration_structure_info,
            vk::Buffer::null(),
            0,
            false,
            self.bottom_level_acceleration_structure.acceleration_structure,
            vk::AccelerationStructureNV::null(),
            bottom_level_scratch_buffer.0,
            0,
        );
        command_buffer.end();
        queue.submit(
            &[vk::SubmitInfo::builder()
                .command_buffers(&[command_buffer.clone().into()])
                .build()],
            vk::Fence::null(),
        );
        queue.wait_idle();

        factory.deallocate_buffer(&top_level_scratch_buffer);
        factory.deallocate_buffer(&bottom_level_scratch_buffer);
        factory.deallocate_buffer(&geometry_instance_buffer);
    }

    pub fn get_top_level_acceleration_structure(&self) -> vk::AccelerationStructureNV {
        self.top_level_acceleration_structure.acceleration_structure
    }
}

struct InternalAccelerationStructure {
    acceleration_structure: vk::AccelerationStructureNV,
    acceleration_structure_info: vk::AccelerationStructureInfoNV,
    _acceleration_structure_handle: u64,
    object_memory: HeapAllocatedMemory,
    build_memory_requirements: vk::MemoryRequirements,
    _update_memory_requirements: vk::MemoryRequirements,
}

impl InternalAccelerationStructure {
    fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_acceleration_structure_nv(self.acceleration_structure);
        factory.deallocate_heap_memory(&self.object_memory);
    }
}
