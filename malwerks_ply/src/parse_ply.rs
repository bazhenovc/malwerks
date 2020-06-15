// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::ply_structs::*;

#[derive(Debug)]
pub enum PlyError {
    InvalidHeader,
    InvalidData,
}

pub fn parse_ply<R>(input: &mut R) -> Result<Ply, PlyError>
where
    R: std::io::Read + std::io::Seek,
{
    // let mut buf_reader = std::io::BufReader::new(input);

    // TODO: remove this once "bufreader_seek_relative" is available
    let mut buf_reader = seek_bufread::BufReader::new(input);

    let ply_header = match parse_header(&mut buf_reader) {
        Some(header) => header,
        None => return Err(PlyError::InvalidHeader),
    };

    let ply_data = match parse_data(&ply_header, &mut buf_reader) {
        Ok(data) => data,
        Err(_) => return Err(PlyError::InvalidData),
    };

    Ok(Ply { ply_header, ply_data })
}

fn parse_header<R>(buf_reader: &mut seek_bufread::BufReader<R>) -> Option<PlyHeader>
where
    R: std::io::Read + std::io::Seek,
{
    use std::io::BufRead;

    let mut ply_format = PlyFormat::BinaryLittleEndian(1.0);
    let mut ply_elements = Vec::new();

    let mut temp_element_header: Option<PlyElementHeader> = None;
    let mut element_index = 0;
    let mut line = String::new();

    loop {
        match buf_reader.read_line(&mut line) {
            Ok(_) => {
                line.pop()?;
                if line.starts_with("ply") || line.starts_with("comment") {
                    // skip
                } else if line.starts_with("format") {
                    // format <name:str> <version:f32>
                    let mut split = line.split_ascii_whitespace();

                    let _format_tag = split.next()?;
                    let format_name = split.next()?;
                    let format_version = match split.next()?.parse() {
                        Ok(v) => v,
                        Err(_) => return None,
                    };

                    if format_name != "binary_little_endian" {
                        return None;
                    }

                    ply_format = PlyFormat::BinaryLittleEndian(format_version);
                } else if line.starts_with("element") {
                    if let Some(mut element) = temp_element_header {
                        element.properties.shrink_to_fit();
                        ply_elements.push(element);
                    }

                    // element <name:str> <count:usize>
                    let mut split = line.split_ascii_whitespace();

                    let _element_tag = split.next()?;
                    let element_name = split.next()?.to_owned();
                    let element_count = match split.next()?.parse() {
                        Ok(c) => c,
                        Err(_) => return None,
                    };

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

                    let _property_tag = split.next()?;
                    let type_or_list = split.next()?;

                    let (property_type, list_index_type) = if type_or_list == "list" {
                        let list_index_type = PlyPropertyType::from_ply_str(split.next()?);
                        let property_type = PlyPropertyType::from_ply_str(split.next()?);
                        (property_type, Some(list_index_type))
                    } else {
                        let property_type = PlyPropertyType::from_ply_str(type_or_list);
                        (property_type, None)
                    };
                    let property_name = split.next()?.to_owned();

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

            Err(_) => return None,
        }
    }

    Some(PlyHeader {
        ply_format,
        ply_elements,
    })
}

fn parse_data<R>(ply_header: &PlyHeader, buf_reader: &mut seek_bufread::BufReader<R>) -> Result<PlyData, std::io::Error>
where
    R: std::io::Read + std::io::Seek,
{
    use std::io::Read;
    use std::io::Seek;

    let mut ply_elements_data = Vec::with_capacity(ply_header.ply_elements.len());
    for element in ply_header.ply_elements.iter() {
        if element.contains_lists() {
            // Thank you ply for an awesome design of list properties.
            // Now I have to iterate over the entire data twice.
            // This is at least 5 times slower compared to regular properties.

            let mut per_element_offsets = Vec::new();
            per_element_offsets.resize(element.element_count, 0);

            let mut temporary_bytes = [0u8; 8];
            let data_start = buf_reader.seek(std::io::SeekFrom::Current(0))?;

            let mut element_buffer_size = 0;
            for _element_id in 0..element.element_count {
                for property in element.properties.iter() {
                    if let Some(list_index_type) = property.list_index_type {
                        let property_stride = property.property_type.stride();
                        let list_index_stride = list_index_type.stride();

                        buf_reader.read_exact(&mut temporary_bytes[0..list_index_stride])?;

                        let element_stride = list_index_type.bytes_to_usize(&temporary_bytes) * property_stride;
                        buf_reader.seek(std::io::SeekFrom::Current(element_stride as _))?;

                        element_buffer_size += element_stride + list_index_stride;
                    } else {
                        let element_stride = property.property_type.stride();
                        element_buffer_size += element_stride;
                        buf_reader.seek(std::io::SeekFrom::Current(element_stride as _))?;
                    }
                }
            }

            let mut element_data = Vec::new();
            element_data.resize(element_buffer_size, 0u8);

            buf_reader.seek(std::io::SeekFrom::Start(data_start))?;

            let mut element_data_start = 0;
            for element_id in 0..element.element_count {
                per_element_offsets[element_id] = element_data_start;
                for property in element.properties.iter() {
                    if let Some(list_index_type) = property.list_index_type {
                        let property_stride = property.property_type.stride();
                        let list_index_stride = list_index_type.stride();

                        let list_index_slice =
                            &mut element_data[element_data_start..element_data_start + list_index_stride];
                        buf_reader.read_exact(list_index_slice)?;

                        let element_stride = list_index_type.bytes_to_usize(&list_index_slice) * property_stride;
                        element_data_start += list_index_stride;
                        buf_reader
                            .read_exact(&mut element_data[element_data_start..element_data_start + element_stride])?;

                        element_data_start += element_stride;
                    } else {
                        let element_stride = property.property_type.stride();
                        buf_reader
                            .read_exact(&mut element_data[element_data_start..element_data_start + element_stride])?;

                        element_data_start += element_stride;
                    }
                }
            }

            ply_elements_data.push(PlyElementData::Structured(PlyStructuredData {
                per_element_offsets,
                element_data,
            }));
        } else {
            let element_stride = element.compute_non_list_stride();
            let element_buffer_size = element.element_count * element_stride;

            let mut element_data = Vec::new();
            element_data.resize(element_buffer_size, 0u8);

            buf_reader.read_exact(&mut element_data)?;
            ply_elements_data.push(PlyElementData::Linear(PlyLinearData {
                element_stride,
                element_data,
            }));
        }
    }

    //buf_reader
    //    .read_exact(&mut [0u8; 1])
    //    .expect_err("end of file test failed");

    Ok(PlyData { ply_elements_data })
}
