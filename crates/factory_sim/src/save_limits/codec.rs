//! Recursive serde adapters keep collection checks independent of snapshot fields.
use super::{COLLECTION_LIMIT_ERROR, SaveLimits};
use serde::Serialize;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::ser::{
    self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use std::cell::Cell;
use std::io::{self, Read};
use std::rc::Rc;

const PAYLOAD_LIMIT_ERROR: &str = "save payload exceeds safety limit";

pub(crate) fn check_collections(
    value: &impl Serialize,
    limits: SaveLimits,
) -> Result<(), bincode::Error> {
    value.serialize(Check(limits))
}
#[derive(Clone, Copy)]
struct Check(SaveLimits);
impl Check {
    fn count(self, count: usize) -> Result<(), bincode::Error> {
        if count as u64 > self.0.max_collection_entries {
            return Err(Box::new(bincode::ErrorKind::Custom(
                COLLECTION_LIMIT_ERROR.into(),
            )));
        }
        Ok(())
    }
}
macro_rules! scalar_checks {
    ($($method:ident: $ty:ty),* $(,)?) => {
        $(fn $method(self, _: $ty) -> Result<(), Self::Error> { Ok(()) })*
    };
}

impl ser::Serializer for Check {
    type Ok = ();
    type Error = bincode::Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;
    scalar_checks! {
        serialize_bool: bool, serialize_i8: i8, serialize_i16: i16,
        serialize_i32: i32, serialize_i64: i64, serialize_i128: i128,
        serialize_u8: u8, serialize_u16: u16, serialize_u32: u32,
        serialize_u64: u64, serialize_u128: u128, serialize_f32: f32,
        serialize_f64: f64, serialize_char: char,
    }

    fn serialize_str(self, value: &str) -> Result<(), Self::Error> {
        self.count(value.len())
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<(), Self::Error> {
        self.count(value.len())
    }
    fn serialize_none(self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), Self::Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(self)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self, Self::Error> {
        if let Some(len) = len {
            self.count(len)?;
        }
        Ok(self)
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self, Self::Error> {
        self.serialize_seq(len)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}

impl SerializeSeq for Check {
    type Ok = ();
    type Error = bincode::Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl SerializeTuple for Check {
    type Ok = ();
    type Error = bincode::Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl SerializeTupleStruct for Check {
    type Ok = ();
    type Error = bincode::Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl SerializeTupleVariant for Check {
    type Ok = ();
    type Error = bincode::Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl SerializeStruct for Check {
    type Ok = ();
    type Error = bincode::Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl SerializeStructVariant for Check {
    type Ok = ();
    type Error = bincode::Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl SerializeMap for Check {
    type Ok = ();
    type Error = bincode::Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Wrap nested seeds as well as visitors so checks cover every collection.
struct Guard<T> {
    inner: T,
    limits: SaveLimits,
    collection: bool,
}
impl<T> Guard<T> {
    fn new(inner: T, limits: SaveLimits) -> Self {
        Self {
            inner,
            limits,
            collection: false,
        }
    }
    fn count<E: de::Error>(&self, count: Option<usize>) -> Result<(), E> {
        if count.is_some_and(|n| n as u64 > self.limits.max_collection_entries) {
            Err(E::custom(COLLECTION_LIMIT_ERROR))
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
pub(crate) fn deserialize<'de, T: serde::Deserialize<'de>>(
    bytes: &'de [u8],
    limits: SaveLimits,
) -> Result<T, bincode::Error> {
    use bincode::Options;
    let options = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(limits.payload_bytes());
    if bytes.len() as u64 > limits.payload_bytes() {
        return Err(Box::new(bincode::ErrorKind::SizeLimit));
    }
    options.deserialize_seed(Guard::new(std::marker::PhantomData::<T>, limits), bytes)
}

/// Decodes from a reader without first retaining the complete wire payload.
///
/// `bincode`'s ordinary I/O reader allocates string and byte buffers before a
/// serde visitor can inspect their declared length. This adapter checks those
/// lengths first, in addition to the recursive sequence/map checks in `Guard`.
pub(crate) fn deserialize_from<T: serde::de::DeserializeOwned>(
    reader: &mut impl Read,
    limits: SaveLimits,
) -> Result<(T, u64), bincode::Error> {
    use bincode::Options;

    let options = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(limits.payload_bytes());
    let consumed = Rc::new(Cell::new(0));
    let bounded = BoundedBincodeReader {
        inner: reader,
        remaining: limits.payload_bytes(),
        max_collection_entries: limits.max_collection_entries,
        consumed: Rc::clone(&consumed),
    };
    let value = options
        .deserialize_from_custom_seed(Guard::new(std::marker::PhantomData::<T>, limits), bounded)?;
    Ok((value, consumed.get()))
}

struct BoundedBincodeReader<R> {
    inner: R,
    remaining: u64,
    max_collection_entries: u64,
    consumed: Rc<Cell<u64>>,
}

impl<R: Read> BoundedBincodeReader<R> {
    fn check_allocation(&self, length: usize) -> Result<(), bincode::Error> {
        if length as u64 > self.max_collection_entries {
            Err(Box::new(bincode::ErrorKind::Custom(
                COLLECTION_LIMIT_ERROR.into(),
            )))
        } else if length as u64 > self.remaining {
            Err(Box::new(bincode::ErrorKind::SizeLimit))
        } else {
            Ok(())
        }
    }
}

impl<R: Read> Read for BoundedBincodeReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return Err(io::Error::other(PAYLOAD_LIMIT_ERROR));
        }
        let allowed = usize::try_from(self.remaining.min(buffer.len() as u64))
            .expect("allowed read length is bounded by the buffer length");
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.remaining -= read as u64;
        self.consumed.set(self.consumed.get() + read as u64);
        Ok(read)
    }
}

impl<'de, R: Read> bincode::BincodeRead<'de> for BoundedBincodeReader<R> {
    fn forward_read_str<V>(&mut self, length: usize, visitor: V) -> bincode::Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        self.check_allocation(length)?;
        let mut bytes = vec![0; length];
        self.read_exact(&mut bytes)?;
        let value = std::str::from_utf8(&bytes).map_err(bincode::ErrorKind::InvalidUtf8Encoding)?;
        visitor.visit_str(value)
    }

    fn get_byte_buffer(&mut self, length: usize) -> bincode::Result<Vec<u8>> {
        self.check_allocation(length)?;
        let mut bytes = vec![0; length];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn forward_read_bytes<V>(&mut self, length: usize, visitor: V) -> bincode::Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        self.check_allocation(length)?;
        let mut bytes = vec![0; length];
        self.read_exact(&mut bytes)?;
        visitor.visit_bytes(&bytes)
    }
}

macro_rules! forward_decode {
    ($($method:ident),* $(,)?) => {
        $(fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            self.inner.$method(Guard::new(visitor, self.limits))
        })*
    };
}

impl<'de, D: de::Deserializer<'de>> de::Deserializer<'de> for Guard<D> {
    type Error = D::Error;

    forward_decode! {
        deserialize_any, deserialize_bool, deserialize_i8, deserialize_i16,
        deserialize_i32, deserialize_i64, deserialize_i128, deserialize_u8,
        deserialize_u16, deserialize_u32, deserialize_u64, deserialize_u128,
        deserialize_f32, deserialize_f64, deserialize_char, deserialize_str,
        deserialize_bytes, deserialize_option, deserialize_unit,
        deserialize_identifier, deserialize_ignored_any,
    }
    // Borrow first, so a forged string/byte length cannot allocate before checks.
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.inner.deserialize_str(Guard::new(visitor, self.limits))
    }
    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.inner
            .deserialize_bytes(Guard::new(visitor, self.limits))
    }
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let mut guard = Guard::new(visitor, self.limits);
        guard.collection = true;
        self.inner.deserialize_seq(guard)
    }
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let mut guard = Guard::new(visitor, self.limits);
        guard.collection = true;
        self.inner.deserialize_map(guard)
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .deserialize_unit_struct(name, Guard::new(visitor, self.limits))
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .deserialize_newtype_struct(name, Guard::new(visitor, self.limits))
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .deserialize_tuple(len, Guard::new(visitor, self.limits))
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .deserialize_tuple_struct(name, len, Guard::new(visitor, self.limits))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .deserialize_struct(name, fields, Guard::new(visitor, self.limits))
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .deserialize_enum(name, variants, Guard::new(visitor, self.limits))
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}
macro_rules! forward_scalars {
    ($($method:ident: $ty:ty),* $(,)?) => {
        $(fn $method<E: de::Error>(self, value: $ty) -> Result<Self::Value, E> {
            self.inner.$method(value)
        })*
    };
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Guard<V> {
    type Value = V::Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.expecting(f)
    }

    forward_scalars! {
        visit_bool: bool, visit_i8: i8, visit_i16: i16, visit_i32: i32,
        visit_i64: i64, visit_i128: i128, visit_u8: u8, visit_u16: u16,
        visit_u32: u32, visit_u64: u64, visit_u128: u128,
        visit_f32: f32, visit_f64: f64, visit_char: char,
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.count(Some(value.len()))?;
        self.inner.visit_str(value)
    }

    fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
        self.count(Some(value.len()))?;
        self.inner.visit_borrowed_str(value)
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        self.count(Some(value.len()))?;
        self.inner.visit_string(value)
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        self.count(Some(value.len()))?;
        self.inner.visit_bytes(value)
    }

    fn visit_borrowed_bytes<E: de::Error>(self, value: &'de [u8]) -> Result<Self::Value, E> {
        self.count(Some(value.len()))?;
        self.inner.visit_borrowed_bytes(value)
    }

    fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        self.count(Some(value.len()))?;
        self.inner.visit_byte_buf(value)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }
    fn visit_some<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(Guard::new(d, self.limits))
    }
    fn visit_newtype_struct<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_newtype_struct(Guard::new(d, self.limits))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, a: A) -> Result<Self::Value, A::Error> {
        if self.collection {
            self.count(a.size_hint())?;
        }
        self.inner.visit_seq(Guard::new(a, self.limits))
    }
    fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<Self::Value, A::Error> {
        self.count(a.size_hint())?;
        self.inner.visit_map(Guard::new(a, self.limits))
    }
    fn visit_enum<A: EnumAccess<'de>>(self, a: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_enum(Guard::new(a, self.limits))
    }
}
impl<'de, T: DeserializeSeed<'de>> DeserializeSeed<'de> for Guard<T> {
    type Value = T::Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.inner.deserialize(Guard::new(d, self.limits))
    }
}
impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Guard<A> {
    type Error = A::Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        self.inner.next_element_seed(Guard::new(seed, self.limits))
    }
    // Suppress allocation from attacker-supplied hints; grow only as elements decode.
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
impl<'de, A: MapAccess<'de>> MapAccess<'de> for Guard<A> {
    type Error = A::Error;
    fn next_key_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        self.inner.next_key_seed(Guard::new(seed, self.limits))
    }
    fn next_value_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        self.inner.next_value_seed(Guard::new(seed, self.limits))
    }
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
impl<'de, A: EnumAccess<'de>> EnumAccess<'de> for Guard<A> {
    type Error = A::Error;
    type Variant = Guard<A::Variant>;
    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, Self::Variant), Self::Error> {
        let (value, variant) = self.inner.variant_seed(Guard::new(seed, self.limits))?;
        Ok((value, Guard::new(variant, self.limits)))
    }
}
impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Guard<A> {
    type Error = A::Error;
    fn unit_variant(self) -> Result<(), Self::Error> {
        self.inner.unit_variant()
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        self.inner
            .newtype_variant_seed(Guard::new(seed, self.limits))
    }
    fn tuple_variant<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .tuple_variant(len, Guard::new(visitor, self.limits))
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.inner
            .struct_variant(fields, Guard::new(visitor, self.limits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;
    use std::collections::BTreeMap;

    #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
    enum Nested {
        Unit,
        Newtype(Vec<u8>),
        Tuple(u8, Vec<u8>),
        Struct {
            values: Option<BTreeMap<String, Vec<u8>>>,
        },
    }

    #[test]
    fn recursive_collection_limits_match_on_write_and_read() {
        let limits = SaveLimits {
            max_collection_entries: 3,
            ..SaveLimits::default()
        };
        for count in [3, 4] {
            for value in [
                Nested::Newtype(vec![0; count]),
                Nested::Tuple(1, vec![0; count]),
                Nested::Struct {
                    values: Some(BTreeMap::from([("key".into(), vec![0; count])])),
                },
            ] {
                let bytes = bincode::DefaultOptions::new()
                    .with_fixint_encoding()
                    .serialize(&value)
                    .unwrap();
                assert_eq!(check_collections(&value, limits).is_ok(), count == 3);
                if count == 3 {
                    assert_eq!(deserialize::<Nested>(&bytes, limits).unwrap(), value);
                } else {
                    assert!(matches!(
                        crate::SaveLoadError::from(
                            deserialize::<Nested>(&bytes, limits).unwrap_err()
                        ),
                        crate::SaveLoadError::TooLarge
                    ));
                }
            }
        }
    }

    #[test]
    fn forged_lengths_are_rejected_before_elements_or_allocations() {
        let bytes = u64::MAX.to_le_bytes();
        let limits = SaveLimits::default();
        assert!(deserialize::<Vec<()>>(&bytes, limits).is_err());
        assert!(deserialize::<Vec<u64>>(&bytes, limits).is_err());
        assert!(deserialize::<BTreeMap<u64, u64>>(&bytes, limits).is_err());
        assert!(deserialize::<String>(&bytes, limits).is_err());
        let truncated = 100_u64.to_le_bytes();
        assert!(deserialize::<Vec<u64>>(&truncated, limits).is_err());
        assert!(deserialize::<String>(&truncated, limits).is_err());
        assert!(deserialize::<u8>(&[1, 2], limits).is_err());
    }

    #[test]
    fn reader_path_rejects_forged_and_oversized_collections() {
        let limits = SaveLimits {
            max_collection_entries: 3,
            ..SaveLimits::default()
        };
        let value = Nested::Newtype(vec![0; 4]);
        let bytes = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&value)
            .unwrap();
        let error =
            deserialize_from::<Nested>(&mut std::io::Cursor::new(bytes), limits).unwrap_err();
        assert!(matches!(
            crate::SaveLoadError::from(error),
            crate::SaveLoadError::TooLarge
        ));

        let forged = u64::MAX.to_le_bytes();
        let error =
            deserialize_from::<String>(&mut std::io::Cursor::new(forged), limits).unwrap_err();
        assert!(matches!(
            crate::SaveLoadError::from(error),
            crate::SaveLoadError::TooLarge
        ));

        let consumed = Rc::new(Cell::new(0));
        let mut reader = BoundedBincodeReader {
            inner: std::io::Cursor::new(Vec::<u8>::new()),
            remaining: 3,
            max_collection_entries: 100,
            consumed,
        };
        let error = bincode::BincodeRead::get_byte_buffer(&mut reader, 4).unwrap_err();
        assert!(matches!(*error, bincode::ErrorKind::SizeLimit));
    }
}
