// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[derive(Debug)]
pub enum PlyFormat {
    BinaryLittleEndian(f32),
}

#[derive(Debug, Copy, Clone)]
pub enum PlyPropertyType {
    Char,
    UnsignedChar,
    Short,
    UnsignedShort,
    Int,
    UnsignedInt,
    Float,
    Double,
}

impl PlyPropertyType {
    pub(crate) fn from_ply_str(s: &str) -> Self {
        match s {
            "char" => PlyPropertyType::Char,
            "uchar" => PlyPropertyType::UnsignedChar,
            "short" => PlyPropertyType::Short,
            "ushort" => PlyPropertyType::UnsignedShort,
            "int" => PlyPropertyType::Int,
            "uint" => PlyPropertyType::UnsignedInt,
            "float" => PlyPropertyType::Float,
            "double" => PlyPropertyType::Double,
            _ => panic!("unknown ply property type"),
        }
    }

    pub(crate) fn bytes_to_usize(self, bytes: &[u8]) -> usize {
        match self {
            PlyPropertyType::Char => *bytemuck::from_bytes::<i8>(&bytes[0..1]) as _,
            PlyPropertyType::UnsignedChar => *bytemuck::from_bytes::<u8>(&bytes[0..1]) as _,
            PlyPropertyType::Short => *bytemuck::from_bytes::<i16>(&bytes[0..2]) as _,
            PlyPropertyType::UnsignedShort => *bytemuck::from_bytes::<u16>(&bytes[0..2]) as _,
            PlyPropertyType::Int => *bytemuck::from_bytes::<i32>(&bytes[0..4]) as _,
            PlyPropertyType::UnsignedInt => *bytemuck::from_bytes::<u32>(&bytes[0..4]) as _,
            PlyPropertyType::Float => *bytemuck::from_bytes::<f32>(&bytes[0..4]) as _,
            PlyPropertyType::Double => *bytemuck::from_bytes::<f64>(&bytes[0..8]) as _,
        }
    }

    pub(crate) fn stride(self) -> usize {
        match self {
            PlyPropertyType::Char => 1,
            PlyPropertyType::UnsignedChar => 1,
            PlyPropertyType::Short => 2,
            PlyPropertyType::UnsignedShort => 2,
            PlyPropertyType::Int => 4,
            PlyPropertyType::UnsignedInt => 4,
            PlyPropertyType::Float => 4,
            PlyPropertyType::Double => 8,
        }
    }
}

#[derive(Debug)]
pub struct PlyPropertyHeader {
    pub property_name: String,
    pub property_type: PlyPropertyType,
    pub list_index_type: Option<PlyPropertyType>,
}

#[derive(Debug)]
pub struct PlyElementHeader {
    pub element_name: String,
    pub element_index: usize,
    pub element_count: usize,
    pub properties: Vec<PlyPropertyHeader>,
}

impl PlyElementHeader {
    pub(crate) fn compute_non_list_stride(&self) -> usize {
        let mut stride = 0;
        for property in self.properties.iter() {
            stride += property.property_type.stride();
        }
        stride
    }

    pub(crate) fn contains_lists(&self) -> bool {
        self.properties.iter().any(|e| e.list_index_type.is_some())
    }
}

#[derive(Debug)]
pub struct PlyHeader {
    pub ply_format: PlyFormat,
    pub ply_elements: Vec<PlyElementHeader>,
}

pub struct PlyLinearData {
    pub(crate) element_stride: usize,
    pub(crate) element_data: Vec<u8>,
}

pub struct PlyStructuredData {
    pub(crate) per_element_offsets: Vec<usize>,
    pub(crate) element_data: Vec<u8>,
}

pub struct PlyRleStructuredData {
    pub(crate) rle_element_offsets: rle_vec::RleVec<(usize, usize, usize)>,
    pub(crate) element_data: Vec<u8>,
}

pub enum PlyElementData {
    Linear(PlyLinearData),
    Structured(PlyStructuredData),
    RleStructured(PlyRleStructuredData),
}

pub trait PlyElementAccess: bytemuck::Pod {
    fn set_member_from_bytes(&mut self, member_index: usize, list_item_count: usize, slice: &[u8]);
}

pub struct PlyData {
    pub(crate) ply_elements_data: Vec<PlyElementData>,
}

impl PlyData {
    pub fn compute_used_memory(&self) -> usize {
        let mut num_bytes = 0;
        for element_data in self.ply_elements_data.iter() {
            num_bytes += match element_data {
                PlyElementData::Linear(linear_data) => {
                    std::mem::size_of_val(&linear_data.element_stride) + linear_data.element_data.len()
                }

                PlyElementData::Structured(structured_data) => {
                    std::mem::size_of_val(&structured_data.per_element_offsets[0])
                        * structured_data.per_element_offsets.len()
                }

                PlyElementData::RleStructured(rle_structured_data) => {
                    std::mem::size_of_val(&rle_structured_data.rle_element_offsets[0])
                        * rle_structured_data.rle_element_offsets.runs_len()
                        + rle_structured_data.element_data.len()
                }
            };
        }
        num_bytes
    }

    pub fn fetch_element<T>(&self, element_header: &PlyElementHeader, element_index: usize, out_element: &mut T)
    where
        T: PlyElementAccess,
    {
        let element = &self.ply_elements_data[element_header.element_index];
        match element {
            PlyElementData::Linear(linear_data) => {
                let data_start = element_index * linear_data.element_stride;
                let data_end = data_start + linear_data.element_stride;
                *out_element = *bytemuck::from_bytes(&linear_data.element_data[data_start..data_end]);
            }

            PlyElementData::Structured(structured_data) => {
                let data_start = structured_data.per_element_offsets[element_index];

                for (property_index, property) in element_header.properties.iter().enumerate() {
                    if let Some(list_index_type) = property.list_index_type {
                        let property_stride = property.property_type.stride();
                        let list_index_stride = list_index_type.stride();

                        let element_count = list_index_type
                            .bytes_to_usize(&structured_data.element_data[data_start..data_start + list_index_stride]);

                        out_element.set_member_from_bytes(
                            property_index,
                            element_count,
                            &structured_data.element_data[data_start..data_start + element_count * property_stride],
                        );
                    } else {
                        let element_stride = property.property_type.stride();
                        out_element.set_member_from_bytes(
                            property_index,
                            0,
                            &structured_data.element_data[data_start..data_start + element_stride],
                        );
                    }
                }
            }

            PlyElementData::RleStructured(rle_structured_data) => {
                let rle_offset = rle_structured_data.rle_element_offsets[element_index];
                let data_start = rle_offset.0 + (element_index - rle_offset.1) * rle_offset.2;

                for (property_index, property) in element_header.properties.iter().enumerate() {
                    if let Some(list_index_type) = property.list_index_type {
                        let property_stride = property.property_type.stride();
                        let list_index_stride = list_index_type.stride();

                        let element_count = list_index_type.bytes_to_usize(
                            &rle_structured_data.element_data[data_start..data_start + list_index_stride],
                        );

                        out_element.set_member_from_bytes(
                            property_index,
                            element_count,
                            &rle_structured_data.element_data[data_start..data_start + element_count * property_stride],
                        );
                    } else {
                        let element_stride = property.property_type.stride();
                        out_element.set_member_from_bytes(
                            property_index,
                            0,
                            &rle_structured_data.element_data[data_start..data_start + element_stride],
                        );
                    }
                }
            }
        }
    }
}

pub struct Ply {
    pub ply_header: PlyHeader,
    pub ply_data: PlyData,
}
