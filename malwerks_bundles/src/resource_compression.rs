// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub(crate) trait CompressibleStorage {
    fn compress(&self) -> Vec<u8>;
    fn decompress(bytes: &[u8]) -> Self;
}

pub(crate) fn serialize<T, S>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: serde::Serialize + CompressibleStorage,
    S: serde::Serializer,
{
    serializer.serialize_bytes(&bytes.compress())
}

pub(crate) fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: serde::Deserialize<'de> + CompressibleStorage,
    D: serde::Deserializer<'de>,
{
    let bytes = deserializer.deserialize_bytes(CowVisitor)?;
    Ok(CompressibleStorage::decompress(&bytes))
}

struct CowVisitor;

impl<'de> serde::de::Visitor<'de> for CowVisitor {
    type Value = std::borrow::Cow<'de, [u8]>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a byte array")
    }

    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(std::borrow::Cow::Borrowed(v))
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(std::borrow::Cow::Borrowed(v.as_bytes()))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(std::borrow::Cow::Owned(v.to_vec()))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(std::borrow::Cow::Owned(v.as_bytes().to_vec()))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(std::borrow::Cow::Owned(v))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(std::borrow::Cow::Owned(v.into_bytes()))
    }

    fn visit_seq<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
    where
        V: serde::de::SeqAccess<'de>,
    {
        let len = core::cmp::min(visitor.size_hint().unwrap_or(0), 4096);
        let mut bytes = Vec::with_capacity(len);

        while let Some(b) = visitor.next_element()? {
            bytes.push(b);
        }

        Ok(std::borrow::Cow::Owned(bytes))
    }
}

impl CompressibleStorage for Vec<u8> {
    fn compress(&self) -> Vec<u8> {
        use std::io::Write;

        let mut encoder = lz4::EncoderBuilder::new()
            .level(9)
            .build(Vec::with_capacity(self.capacity()))
            .expect("failed to create lz4 encoder");
        let _ = encoder.write(self.as_slice()).expect("failed to write lz4 stream");
        let (output, result) = encoder.finish();
        result.expect("failed to compress lz4 data");
        output
    }

    fn decompress(bytes: &[u8]) -> Self {
        use std::io::Read;

        let mut target = Vec::with_capacity(bytes.len());

        let mut decoder = lz4::Decoder::new(bytes).expect("failed to create lz4 decoder");
        decoder.read_to_end(&mut target).expect("failed to read lz4 data");
        let (_, result) = decoder.finish();
        result.expect("failed to decompress lz4 data");
        target
    }
}
