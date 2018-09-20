use std::borrow::Borrow;
use std::fmt::{self, Debug, Display};
use std::mem;
use std::ops::Deref;

use serde::ser::{Serialize, Serializer, SerializeStruct};
use serde::de::{self, Deserialize, Deserializer, DeserializeSeed, IntoDeserializer, MapAccess, Unexpected, Visitor};
use serde::de::value::BorrowedStrDeserializer;

use error::Error;

/// Reference to a range of bytes encompassing a single valid JSON value in the
/// input data.
///
/// A `RawSlice` can be used to defer parsing parts of a payload until later,
/// or to avoid parsing it at all in the case that part of the payload just
/// needs to be transferred verbatim into a different output object.
///
/// When serializing, a value of this type will retain its original formatting
/// and will not be minified or pretty-printed.
///
/// # Example
///
/// ```
/// #[macro_use]
/// extern crate serde_derive;
/// extern crate serde_json;
///
/// use serde_json::{Result, value::RawSlice};
///
/// #[derive(Deserialize)]
/// struct Input<'a> {
///     code: u32,
///     #[serde(borrow)]
///     payload: &'a RawSlice,
/// }
///
/// #[derive(Serialize)]
/// struct Output<'a> {
///     info: (u32, &'a RawSlice),
/// }
///
/// // Efficiently rearrange JSON input containing separate "code" and "payload"
/// // keys into a single "info" key holding an array of code and payload.
/// //
/// // This could be done equivalently using serde_json::Value as the type for
/// // payload, but &RawSlice will perform netter because it does not require
/// // memory allocation. The correct range of bytes is borrowed from the input
/// // data and pasted verbatim into the output.
/// fn rearrange(input: &str) -> Result<String> {
///     let input: Input = serde_json::from_str(input)?;
///
///     let output = Output {
///         info: (input.code, input.payload),
///     };
///
///     serde_json::to_string(&output)
/// }
///
/// fn main() -> Result<()> {
///     let out = rearrange(r#" {"code": 200, "payload": {}} "#)?;
///
///     assert_eq!(out, r#"{"info":[200,{}]}"#);
///
///     Ok(())
/// }
/// ```
///
/// # Note
///
/// `RawSlice` is only available if serde\_json is built with the `"raw_value"`
/// feature.
///
/// ```toml
/// [dependencies]
/// serde_json = { version = "1.0", features = ["raw_value"] }
/// ```
#[repr(C)]
pub struct RawSlice {
    borrowed: str,
}

///
pub struct RawValue {
    owned: Box<RawSlice>,
}

impl RawSlice {
    fn from_inner(borrowed: &str) -> &Self {
        unsafe { mem::transmute::<&str, &RawSlice>(borrowed) }
    }
}

impl RawValue {
    fn from_inner(owned: Box<str>) -> Self {
        RawValue {
            owned: unsafe { mem::transmute::<Box<str>, Box<RawSlice>>(owned) },
        }
    }
}

impl Clone for RawValue {
    fn clone(&self) -> Self {
        self.owned.to_owned()
    }
}

impl Deref for RawValue {
    type Target = RawSlice;

    fn deref(&self) -> &Self::Target {
        &self.owned
    }
}

impl Borrow<RawSlice> for RawValue {
    fn borrow(&self) -> &RawSlice {
        &self.owned
    }
}

impl ToOwned for RawSlice {
    type Owned = RawValue;

    fn to_owned(&self) -> Self::Owned {
        RawValue::from_inner(self.borrowed.to_owned().into_boxed_str())
    }
}

impl Debug for RawSlice {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter
            .debug_tuple("RawSlice")
            .field(&format_args!("{}", &self.borrowed))
            .finish()
    }
}

impl Debug for RawValue {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter
            .debug_tuple("RawValue")
            .field(&format_args!("{}", &self.owned.borrowed))
            .finish()
    }
}

impl RawSlice {
    /// Access the JSON text underlying a raw value.
    ///
    /// # Example
    ///
    /// ```
    /// #[macro_use]
    /// extern crate serde_derive;
    /// extern crate serde_json;
    ///
    /// use serde_json::{Result, value::RawSlice};
    ///
    /// #[derive(Deserialize)]
    /// struct Response<'a> {
    ///     code: u32,
    ///     #[serde(borrow)]
    ///     payload: &'a RawSlice,
    /// }
    ///
    /// fn process(input: &str) -> Result<()> {
    ///     let response: Response = serde_json::from_str(input)?;
    ///
    ///     let payload = response.payload.as_ref();
    ///     if payload.starts_with('{') {
    ///         // handle a payload which is a JSON map
    ///     } else {
    ///         // handle any other type
    ///     }
    ///
    ///     Ok(())
    /// }
    ///
    /// fn main() -> Result<()> {
    ///     process(r#" {"code": 200, "payload": {}} "#)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn as_ref(&self) -> &str {
        &self.borrowed
    }
}

impl Display for RawSlice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.borrowed)
    }
}

impl Display for RawValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&**self, f)
    }
}

pub const TOKEN: &'static str = "$serde_json::private::RawValue";

impl Serialize for RawSlice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct(TOKEN, 1)?;
        s.serialize_field(TOKEN, &self.borrowed)?;
        s.end()
    }
}

impl Serialize for RawValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (**self).serialize(serializer)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for &'a RawSlice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RawSliceVisitor;

        impl<'de> Visitor<'de> for RawSliceVisitor {
            type Value = &'de RawSlice;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "any valid JSON value")
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let value = visitor.next_key::<RawKey>()?;
                if value.is_none() {
                    return Err(de::Error::invalid_type(Unexpected::Map, &self));
                }
                visitor.next_value_seed(RawSliceFromString)
            }
        }

        deserializer.deserialize_newtype_struct(TOKEN, RawSliceVisitor)
    }
}

impl<'de> Deserialize<'de> for RawValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RawValueVisitor;

        impl<'de> Visitor<'de> for RawValueVisitor {
            type Value = RawValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "any valid JSON value")
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let value = visitor.next_key::<RawKey>()?;
                if value.is_none() {
                    return Err(de::Error::invalid_type(Unexpected::Map, &self));
                }
                visitor.next_value_seed(RawValueFromString)
            }
        }

        deserializer.deserialize_newtype_struct(TOKEN, RawValueVisitor)
    }
}

struct RawKey;

impl<'de> Deserialize<'de> for RawKey {
    fn deserialize<D>(deserializer: D) -> Result<RawKey, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FieldVisitor;

        impl<'de> Visitor<'de> for FieldVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("raw value")
            }

            fn visit_str<E>(self, s: &str) -> Result<(), E>
            where
                E: de::Error,
            {
                if s == TOKEN {
                    Ok(())
                } else {
                    Err(de::Error::custom("unexpected raw value"))
                }
            }
        }

        deserializer.deserialize_identifier(FieldVisitor)?;
        Ok(RawKey)
    }
}

pub struct RawSliceFromString;

impl<'de> DeserializeSeed<'de> for RawSliceFromString {
    type Value = &'de RawSlice;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for RawSliceFromString {
    type Value = &'de RawSlice;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("raw value")
    }

    fn visit_borrowed_str<E>(self, s: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(RawSlice::from_inner(s))
    }
}

pub struct RawValueFromString;

impl<'de> DeserializeSeed<'de> for RawValueFromString {
    type Value = RawValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for RawValueFromString {
    type Value = RawValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("raw value")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(s.to_owned())
    }

    fn visit_string<E>(self, s: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(RawValue::from_inner(s.into_boxed_str()))
    }
}

struct RawKeyDeserializer;

impl<'de> Deserializer<'de> for RawKeyDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_borrowed_str(TOKEN)
    }

    forward_to_deserialize_any! {
        bool u8 u16 u32 u64 u128 i8 i16 i32 i64 i128 f32 f64 char str string seq
        bytes byte_buf map struct option unit newtype_struct ignored_any
        unit_struct tuple_struct tuple enum identifier
    }
}

pub struct OwnedRawDeserializer {
    pub raw_value: Option<String>,
}

impl<'de> MapAccess<'de> for OwnedRawDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        if self.raw_value.is_none() {
            return Ok(None);
        }
        seed.deserialize(RawKeyDeserializer).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.raw_value.take().unwrap().into_deserializer())
    }
}

pub struct BorrowedRawDeserializer<'de> {
    pub raw_value: Option<&'de str>,
}

impl<'de> MapAccess<'de> for BorrowedRawDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        if self.raw_value.is_none() {
            return Ok(None);
        }
        seed.deserialize(RawKeyDeserializer).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(BorrowedStrDeserializer::new(self.raw_value.take().unwrap()))
    }
}
