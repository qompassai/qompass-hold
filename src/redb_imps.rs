use std::{
    any::type_name,
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    io::{self, Cursor, Read, Write},
};

use redb::{TypeName, Value};

/// decode a variable length integer
fn decode_int<T: Read>(data: &mut T) -> io::Result<usize> {
    let mut byte_buf = [0];
    data.read_exact(&mut byte_buf)?;
    match byte_buf[0] {
        255 => {
            let mut int_buf = [0, 0, 0, 0];
            data.read_exact(&mut int_buf)?;
            Ok(u32::from_le_bytes(int_buf) as usize)
        }
        254 => {
            let mut short_buf = [0, 0];
            data.read_exact(&mut short_buf)?;
            Ok(u16::from_le_bytes(short_buf) as usize)
        }
        num => Ok(num as usize),
    }
}

#[test]
fn test_decode_int() {
    let four: [u8; 1] = [4];
    assert_eq!(decode_int(&mut four.as_ref()).unwrap(), 4);
    let short: [u8; 3] = [254, 55, 187];
    assert_eq!(decode_int(&mut short.as_ref()).unwrap(), 55 | (187 << 8));
    let int: [u8; 5] = [255, 123, 254, 2, 3];
    assert_eq!(
        decode_int(&mut int.as_ref()).unwrap(),
        123 | (254 << 8) | (2 << 16) | (3 << 24)
    );
}

/// encodes an int to the end of a buffer
/// Values above `u32::MAX` do not fit the format and are an error;
/// silently writing nothing (the previous behavior) would corrupt
/// the stream — a decoder would read the next field as this length.
fn encode_int<T: Write>(int: usize, buf: &mut T) -> io::Result<()> {
    if int < 253 {
        buf.write_all(&[int as u8])?;
    } else if int <= u16::MAX.into() {
        buf.write_all(&[254])?;
        buf.write_all(&(int as u16).to_le_bytes())?;
    } else if int <= u32::MAX as usize {
        buf.write_all(&[255])?;
        buf.write_all(&(int as u32).to_le_bytes())?;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "integer too large to encode (max u32::MAX)",
        ));
    }
    Ok(())
}

#[test]
fn test_encode_int_boundaries() {
    // every width class, at both edges, must round-trip exactly
    for value in [
        0usize,
        1,
        252, // last single-byte value
        253, // first two-byte (254-marker) value
        u16::MAX as usize - 1,
        u16::MAX as usize,     // last two-byte value
        u16::MAX as usize + 1, // first four-byte (255-marker) value
        u32::MAX as usize - 1,
        u32::MAX as usize, // largest encodable value
    ] {
        let mut buf = Vec::new();
        encode_int(value, &mut buf).unwrap();
        assert_eq!(
            decode_int(&mut buf.as_slice()).unwrap(),
            value,
            "round-trip of {value}"
        );
    }
}

#[test]
fn test_encode_int_too_large_is_an_error() {
    // adversarial: an out-of-format value must fail loudly and
    // write nothing, never a silent empty encoding
    let mut buf = Vec::new();
    let err = encode_int(u32::MAX as usize + 1, &mut buf).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    assert!(buf.is_empty(), "nothing may be written on error");
}

#[test]
fn test_decode_int_truncated_input_is_an_error() {
    // adversarial: truncated streams must error, not default to 0
    assert!(decode_int(&mut [].as_slice()).is_err());
    assert!(decode_int(&mut [254].as_slice()).is_err());
    assert!(decode_int(&mut [254, 1].as_slice()).is_err());
    assert!(decode_int(&mut [255, 1, 2].as_slice()).is_err());
}

#[test]
fn test_encode_int() {
    use std::io::Seek;

    let mut buf = Cursor::new(Vec::new());
    encode_int(124, &mut buf).unwrap();
    buf.rewind().unwrap();
    assert_eq!(decode_int(&mut buf).unwrap(), 124);
    buf.rewind().unwrap();

    encode_int(43123, &mut buf).unwrap();
    buf.rewind().unwrap();
    assert_eq!(decode_int(&mut buf).unwrap(), 43123);
    buf.rewind().unwrap();

    encode_int(3194105786, &mut buf).unwrap();
    buf.rewind().unwrap();
    assert_eq!(decode_int(&mut buf).unwrap(), 3194105786);
}

#[derive(Debug)]
pub struct RedbHashMap<K: Debug, V: Debug>(K, V);

impl<K, V> Value for RedbHashMap<K, V>
where
    K: Value,
    V: Value,
    for<'a> K::SelfType<'a>: Hash + Eq,
{
    type SelfType<'a>
        = HashMap<K::SelfType<'a>, V::SelfType<'a>>
    where
        Self: 'a;

    type AsBytes<'a>
        = Vec<u8>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        let mut buf = Cursor::new(data);
        let len = decode_int(&mut buf).unwrap();
        let mut map = HashMap::with_capacity(len);

        for _ in 0..len {
            let key_len = K::fixed_width().unwrap_or_else(|| decode_int(&mut buf).unwrap());
            // decode the key
            let p = buf.position() as usize;
            let key = K::from_bytes(&data[p..p + key_len]);
            buf.set_position((p + key_len) as u64);

            let val_len = V::fixed_width().unwrap_or_else(|| decode_int(&mut buf).unwrap());
            let p = buf.position() as usize;
            let val = V::from_bytes(&data[p..p + val_len]);
            buf.set_position((p + val_len) as u64);

            map.insert(key, val);
        }

        map
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let len = value.len();
        // guesstimation
        let mut buf = Vec::with_capacity(len * 2);
        encode_int(len, &mut buf).unwrap();

        for (k, v) in value.iter() {
            let key_bytes = K::as_bytes(k);
            let key_ref = key_bytes.as_ref();
            let val_bytes = V::as_bytes(v);
            let val_ref = val_bytes.as_ref();

            // we need to encode the length if it's not fixed
            if K::fixed_width().is_none() {
                encode_int(key_ref.len(), &mut buf).unwrap();
            }
            buf.extend_from_slice(key_ref);

            if V::fixed_width().is_none() {
                encode_int(val_ref.len(), &mut buf).unwrap();
            }
            buf.extend_from_slice(val_ref);
        }

        buf
    }

    fn type_name() -> TypeName {
        TypeName::new(&format!(
            "HashMap<{}, {}>",
            type_name::<K>(),
            type_name::<V>()
        ))
    }
}

#[test]
fn test_redb_hashmap() {
    let map = HashMap::from([("hello", 5), ("foo", 4), ("bar", 3)]);

    let serialized = RedbHashMap::<&str, u8>::as_bytes(&map);
    let deserialized = RedbHashMap::<&str, u8>::from_bytes(&serialized);
    assert_eq!(map, deserialized);
}

#[test]
fn test_redb_hashmap_empty() {
    // an empty map is a real stored value and must round-trip too
    let map: HashMap<&str, u8> = HashMap::new();
    let serialized = RedbHashMap::<&str, u8>::as_bytes(&map);
    let deserialized = RedbHashMap::<&str, u8>::from_bytes(&serialized);
    assert_eq!(map, deserialized);
}
