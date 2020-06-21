// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;

use ash::vk;

pub struct AccelerationStructure {
    bottom_level_acceleration_structure: Vec<InternalAccelerationStructure>,
    top_level_acceleration_structure: InternalAccelerationStructure,

    world_instances: Vec<vk::AccelerationStructureInstanceNV>,
    _world_geometries: Vec<vk::GeometryNV>,
}

impl AccelerationStructure {
    pub fn new(render_world: &RenderWorld, factory: &mut DeviceFactory) -> Self {
        let world_geometries = render_world.create_geometries_nv();

        let mut bottom_level_acceleration_structure = Vec::with_capacity(world_geometries.len());
        let mut blas_references = Vec::with_capacity(world_geometries.len());
        for (id, _geometry) in world_geometries.iter().enumerate() {
            let acceleration_structure_info = vk::AccelerationStructureInfoNV::builder()
                .ty(vk::AccelerationStructureTypeNV::BOTTOM_LEVEL_NV)
                .flags(vk::BuildAccelerationStructureFlagsNV::PREFER_FAST_TRACE)
                .geometries(&world_geometries[id..id + 1])
                .build();
            let acceleration_structure = factory.create_acceleration_structure_nv(
                &vk::AccelerationStructureCreateInfoNV::builder()
                    .info(acceleration_structure_info)
                    .build(),
            );

            let blas_memory_requirements_object = factory.get_acceleration_structure_memory_requirements_nv(
                &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                    .ty(vk::AccelerationStructureMemoryRequirementsTypeNV::OBJECT_NV)
                    .acceleration_structure(acceleration_structure)
                    .build(),
            );
            let blas_memory_requirements_build = factory.get_acceleration_structure_memory_requirements_nv(
                &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                    .ty(vk::AccelerationStructureMemoryRequirementsTypeNV::BUILD_SCRATCH_NV)
                    .acceleration_structure(acceleration_structure)
                    .build(),
            );
            let blas_memory_requirements_update = factory.get_acceleration_structure_memory_requirements_nv(
                &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                    .ty(vk::AccelerationStructureMemoryRequirementsTypeNV::UPDATE_SCRATCH_NV)
                    .acceleration_structure(acceleration_structure)
                    .build(),
            );

            log::info!(
                "BLAS memory requirements: OBJECT: {:?} BUILD {:?} UPDATE {:?}",
                &blas_memory_requirements_object.memory_requirements,
                &blas_memory_requirements_build.memory_requirements,
                &blas_memory_requirements_update.memory_requirements
            );

            let blas_memory = factory.allocate_heap_memory(
                &blas_memory_requirements_object.memory_requirements,
                &vk_mem::AllocationCreateInfo::default(),
            );
            factory.bind_acceleration_structure_memory_nv(&[vk::BindAccelerationStructureMemoryInfoNV::builder()
                .acceleration_structure(acceleration_structure)
                .memory(blas_memory.0.get_device_memory())
                .memory_offset(blas_memory.0.get_offset() as _)
                .build()]);
            let acceleration_structure_handle = factory.get_acceleration_structure_handle_nv(acceleration_structure);

            blas_references.push(acceleration_structure_handle);
            bottom_level_acceleration_structure.push(InternalAccelerationStructure {
                acceleration_structure,
                acceleration_structure_info,
                _acceleration_structure_handle: acceleration_structure_handle,
                object_memory: blas_memory,
                build_memory_requirements: blas_memory_requirements_build.memory_requirements,
                _update_memory_requirements: blas_memory_requirements_update.memory_requirements,
            });
        }

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

        let tlas_memory_requirements_object = factory.get_acceleration_structure_memory_requirements_nv(
            &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                .ty(vk::AccelerationStructureMemoryRequirementsTypeNV::OBJECT_NV)
                .acceleration_structure(top_level_acceleration_structure)
                .build(),
        );
        let tlas_memory_requirements_build = factory.get_acceleration_structure_memory_requirements_nv(
            &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                .ty(vk::AccelerationStructureMemoryRequirementsTypeNV::BUILD_SCRATCH_NV)
                .acceleration_structure(top_level_acceleration_structure)
                .build(),
        );
        let tlas_memory_requirements_update = factory.get_acceleration_structure_memory_requirements_nv(
            &vk::AccelerationStructureMemoryRequirementsInfoNV::builder()
                .ty(vk::AccelerationStructureMemoryRequirementsTypeNV::UPDATE_SCRATCH_NV)
                .acceleration_structure(top_level_acceleration_structure)
                .build(),
        );

        log::info!(
            "TLAS memory requirements: OBJECT: {:?} BUILD {:?} UPDATE {:?}",
            &tlas_memory_requirements_object.memory_requirements,
            &tlas_memory_requirements_build.memory_requirements,
            &tlas_memory_requirements_update.memory_requirements
        );

        let top_level_object_memory = factory.allocate_heap_memory(
            &tlas_memory_requirements_object.memory_requirements,
            &vk_mem::AllocationCreateInfo::default(),
        );

        factory.bind_acceleration_structure_memory_nv(&[vk::BindAccelerationStructureMemoryInfoNV::builder()
            .acceleration_structure(top_level_acceleration_structure)
            .memory(top_level_object_memory.0.get_device_memory())
            .memory_offset(top_level_object_memory.0.get_offset() as _)
            .build()]);
        let top_level_acceleration_structure_handle =
            factory.get_acceleration_structure_handle_nv(top_level_acceleration_structure);

        let world_instances = render_world.create_instances_nv(
            0,
            0,
            vk::GeometryInstanceFlagsNV::TRIANGLE_CULL_DISABLE_NV,
            &blas_references,
        );

        Self {
            bottom_level_acceleration_structure,
            top_level_acceleration_structure: InternalAccelerationStructure {
                acceleration_structure: top_level_acceleration_structure,
                acceleration_structure_info: top_level_acceleration_structure_info,
                _acceleration_structure_handle: top_level_acceleration_structure_handle,
                object_memory: top_level_object_memory,
                build_memory_requirements: tlas_memory_requirements_build.memory_requirements,
                _update_memory_requirements: tlas_memory_requirements_update.memory_requirements,
            },
            world_instances,
            _world_geometries: world_geometries,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.top_level_acceleration_structure.destroy(factory);
        for blas in &mut self.bottom_level_acceleration_structure {
            blas.destroy(factory);
        }
    }

    pub fn build(&mut self, command_buffer: &mut CommandBuffer, factory: &mut DeviceFactory, queue: &mut DeviceQueue) {
        let mut blas_scratch_size = 0;
        for blas in &self.bottom_level_acceleration_structure {
            blas_scratch_size = blas_scratch_size.max(blas.build_memory_requirements.size);
        }
        let bottom_level_scratch_buffer = factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size(blas_scratch_size)
                .usage(vk::BufferUsageFlags::RAY_TRACING_NV)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                memory_type_bits: self.bottom_level_acceleration_structure[0]
                    .build_memory_requirements
                    .memory_type_bits,
                ..Default::default()
            },
        );

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
        for blas in &self.bottom_level_acceleration_structure {
            command_buffer.build_acceleration_structure_nv(
                &blas.acceleration_structure_info,
                vk::Buffer::null(),
                0,
                false,
                blas.acceleration_structure,
                vk::AccelerationStructureNV::null(),
                bottom_level_scratch_buffer.0,
                0,
            );
            command_buffer.pipeline_barrier(
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_NV,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_NV,
                None,
                &[vk::MemoryBarrier::builder()
                    .src_access_mask(
                        vk::AccessFlags::ACCELERATION_STRUCTURE_READ_NV
                            | vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_NV,
                    )
                    .dst_access_mask(
                        vk::AccessFlags::ACCELERATION_STRUCTURE_READ_NV
                            | vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_NV,
                    )
                    .build()],
                &[],
                &[],
            );
        }
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
        command_buffer.pipeline_barrier(
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_NV,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_NV,
            None,
            &[vk::MemoryBarrier::builder()
                .src_access_mask(
                    vk::AccessFlags::ACCELERATION_STRUCTURE_READ_NV | vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_NV,
                )
                .dst_access_mask(
                    vk::AccessFlags::ACCELERATION_STRUCTURE_READ_NV | vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_NV,
                )
                .build()],
            &[],
            &[],
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
