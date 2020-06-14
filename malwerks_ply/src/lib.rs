// Copyright (c) 2020 Kyrylo Bazhenov
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
    fn from_ply_str(s: &str) -> Self {
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

    fn bytes_to_usize(self, bytes: &[u8]) -> usize {
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

    fn stride(self) -> usize {
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
    property_name: String,
    property_type: PlyPropertyType,
    list_index_type: Option<PlyPropertyType>,
}

#[derive(Debug)]
pub struct PlyElementHeader {
    element_name: String,
    element_index: usize,
    element_count: usize,
    properties: Vec<PlyPropertyHeader>,
}

impl PlyElementHeader {
    fn compute_non_list_stride(&self) -> usize {
        let mut stride = 0;
        for property in self.properties.iter() {
            stride += property.property_type.stride();
        }
        stride
    }

    fn contains_lists(&self) -> bool {
        self.properties.iter().any(|e| e.list_index_type.is_some())
    }
}

#[derive(Debug)]
pub struct PlyHeader {
    ply_format: PlyFormat,
    ply_elements: Vec<PlyElementHeader>,
}

pub struct PlyLinearData {
    element_stride: usize,
    element_data: Vec<u8>,
}

pub struct PlyStructuredData {
    per_element_strides: Vec<(usize, usize)>, // stride + offset into element_data
    element_data: Vec<u8>,
}

pub enum PlyElementData {
    Linear(PlyLinearData),
    Structured(PlyStructuredData),
}

pub struct PlyData {
    ply_elements_data: Vec<PlyElementData>,
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
                    std::mem::size_of_val(&structured_data.per_element_strides[0])
                        * structured_data.per_element_strides.len()
                        + structured_data.element_data.len()
                }
            };
        }
        num_bytes
    }

    pub fn fetch_element<T>(&self, element_header: &PlyElementHeader, element_index: usize) -> T
    where
        T: bytemuck::Pod,
    {
        let element = &self.ply_elements_data[element_header.element_index];
        match element {
            PlyElementData::Linear(linear_data) => {
                let data_start = element_index * linear_data.element_stride;
                let data_end = data_start + linear_data.element_stride;
                *bytemuck::from_bytes(&linear_data.element_data[data_start..data_end])
            }

            PlyElementData::Structured(structured_data) => {
                // TODO: this probably wouldn't work because of variable-sized arrays
                let stride = structured_data.per_element_strides[element_index];
                let data_start = stride.1;
                let data_end = data_start + stride.0;
                *bytemuck::from_bytes(&structured_data.element_data[data_start..data_end])
            }
        }
    }
}

pub struct Ply {
    pub ply_header: PlyHeader,
    pub ply_data: PlyData,
}

#[logging_timer::time("info")]
pub fn parse_ply<R>(input: &mut R) -> Ply
where
    R: std::io::Read + std::io::Seek,
{
    // let mut buf_reader = std::io::BufReader::new(input);

    // TODO: remove this once "bufreader_seek_relative" is available
    let mut buf_reader = seek_bufread::BufReader::new(input);

    let ply_header = {
        use std::io::BufRead;

        let mut ply_format = PlyFormat::BinaryLittleEndian(1.0);
        let mut ply_elements = Vec::new();

        let mut temp_element_header: Option<PlyElementHeader> = None;
        let mut element_index = 0;
        let mut line = String::new();

        loop {
            match buf_reader.read_line(&mut line) {
                Ok(_) => {
                    line.pop().expect("failed to pop newline char");
                    if line.starts_with("ply") || line.starts_with("comment") {
                        // skip
                    } else if line.starts_with("format") {
                        // format <name:str> <version:f32>
                        let mut split = line.split_ascii_whitespace();

                        let _format_tag = split.next().expect("invalid format");
                        let format_name = split.next().expect("invalid format");
                        let format_version = split
                            .next()
                            .expect("invalid format")
                            .parse()
                            .expect("invalid format version");

                        assert_eq!(format_name, "binary_little_endian");
                        ply_format = PlyFormat::BinaryLittleEndian(format_version);
                    } else if line.starts_with("element") {
                        if let Some(mut element) = temp_element_header {
                            element.properties.shrink_to_fit();
                            ply_elements.push(element);
                        }

                        // element <name:str> <count:usize>
                        let mut split = line.split_ascii_whitespace();

                        let _element_tag = split.next().expect("invalid element");
                        let element_name = split.next().expect("invalid element").to_owned();
                        let element_count = split
                            .next()
                            .expect("invalid element")
                            .parse()
                            .expect("invalid element count");

                        temp_element_header = Some(PlyElementHeader {
                            element_name,
                            element_index,
                            element_count,
                            properties: Vec::new(),
                        });
                        element_index += 1;
                    } else if line.starts_with("property") {
                        // property [<type> | "list" <type>] <type> <name:str>
                        let mut split = line.split_ascii_whitespace();

                        let _property_tag = split.next().expect("invalid property");
                        let type_or_list = split.next().expect("invalid property");

                        let (property_type, list_index_type) = if type_or_list == "list" {
                            let list_index_type =
                                PlyPropertyType::from_ply_str(split.next().expect("invalid property list type"));
                            let property_type =
                                PlyPropertyType::from_ply_str(split.next().expect("invalid property type"));
                            (property_type, Some(list_index_type))
                        } else {
                            let property_type = PlyPropertyType::from_ply_str(type_or_list);
                            (property_type, None)
                        };
                        let property_name = split.next().expect("invalid property").to_owned();

                        if let Some(element) = temp_element_header.as_mut() {
                            element.properties.push(PlyPropertyHeader {
                                property_name,
                                property_type,
                                list_index_type,
                            });
                        }
                    } else if line == "end_header" {
                        if let Some(mut element) = temp_element_header {
                            element.properties.shrink_to_fit();
                            ply_elements.push(element);
                        }

                        break;
                    } else {
                        log::error!("unknown ply tag: {:?}", line);
                    }
                    line.clear();
                }

                Err(error) => panic!("ply reading error: {:?}", error),
            }
        }

        PlyHeader {
            ply_format,
            ply_elements,
        }
    };

    let ply_data = {
        use std::io::Read;
        use std::io::Seek;

        let mut ply_elements_data = Vec::with_capacity(ply_header.ply_elements.len());
        for element in ply_header.ply_elements.iter() {
            if element.contains_lists() {
                // Thank you ply for an awesome design of list properties.
                // Now I have to iterate over the entire data twice.
                // This is at least 5 times slower compared to regular properties.

                let mut per_element_strides = Vec::new();
                per_element_strides.resize(element.element_count * element.properties.len(), (0, 0));

                let mut temporary_bytes = [0u8; 8];
                let data_start = buf_reader
                    .seek(std::io::SeekFrom::Current(0))
                    .expect("failed to get current file position");

                let mut element_buffer_size = 0;
                for element_id in 0..element.element_count {
                    for (property_id, property) in element.properties.iter().enumerate() {
                        let stride_index = element_id + property_id;
                        if let Some(list_index_type) = property.list_index_type {
                            let property_stride = property.property_type.stride();
                            let list_index_stride = list_index_type.stride();

                            buf_reader
                                .read_exact(&mut temporary_bytes[0..list_index_stride])
                                .expect("failed to read list count");

                            let element_stride = list_index_type.bytes_to_usize(&temporary_bytes) * property_stride;
                            per_element_strides[stride_index].0 = element_stride;

                            buf_reader
                                .seek(std::io::SeekFrom::Current(element_stride as _))
                                .expect("failed to seek buffer");

                            element_buffer_size += element_stride;
                        } else {
                            let element_stride = property.property_type.stride();
                            per_element_strides[stride_index].0 = element_stride;

                            element_buffer_size += element_stride;
                            buf_reader
                                .seek(std::io::SeekFrom::Current(element_stride as _))
                                .expect_err("failed to seek buffer");
                        }
                    }
                }

                let mut element_data = Vec::new();
                element_data.resize(element_buffer_size, 0u8);

                buf_reader
                    .seek(std::io::SeekFrom::Start(data_start))
                    .expect("failed to seek buffer");

                let mut element_data_start = 0;
                for element_id in 0..element.element_count {
                    for (property_id, property) in element.properties.iter().enumerate() {
                        let stride_index = element_id + property_id;
                        per_element_strides[stride_index].1 = element_data_start;

                        if let Some(list_index_type) = property.list_index_type {
                            let list_index_stride = list_index_type.stride();
                            buf_reader
                                .read_exact(&mut temporary_bytes[0..list_index_stride])
                                .expect("failed to read list count for element");

                            let element_stride = per_element_strides[stride_index].0;
                            buf_reader
                                .read_exact(&mut element_data[element_data_start..element_data_start + element_stride])
                                .expect("failed to read structured element");

                            element_data_start += element_stride;
                        } else {
                            let element_stride = per_element_strides[stride_index].0;
                            buf_reader
                                .read_exact(&mut element_data[element_data_start..element_data_start + element_stride])
                                .expect("failed to read structured element");

                            element_data_start += element_stride;
                        }
                    }
                }

                ply_elements_data.push(PlyElementData::Structured(PlyStructuredData {
                    per_element_strides,
                    element_data,
                }));
            } else {
                let element_stride = element.compute_non_list_stride();
                let element_buffer_size = element.element_count * element_stride;

                let mut element_data = Vec::new();
                element_data.resize(element_buffer_size, 0u8);

                buf_reader
                    .read_exact(&mut element_data)
                    .expect("failed to read element data");
                ply_elements_data.push(PlyElementData::Linear(PlyLinearData {
                    element_stride,
                    element_data,
                }));
            }
        }

        buf_reader
            .read_exact(&mut [0u8; 1])
            .expect_err("end of file test failed");

        PlyData { ply_elements_data }
    };

    Ply { ply_header, ply_data }
}
