// SPDX-License-Identifier: MPL-2.0
use typedio::message::{decode, encode, TransferError, Wire};
use typedio::{BoundedVec, Message};

#[derive(Debug, PartialEq, Message)]
struct Nested<T: Message> {
    tag: u8,
    body: Option<T>,
    result: Result<[u16; 3], char>,
}
#[derive(Debug, PartialEq, Message)]
enum Event {
    Unit,
    Tuple(u32, bool),
    Record { value: Nested<u64> },
}
#[repr(C)]
#[derive(Message)]
struct Padded {
    a: u8,
    b: u64,
}
#[derive(Message)]
struct SameName {
    a: u32,
    b: u32,
}
#[derive(Message)]
struct DifferentName {
    a: u32,
    b: u32,
}

#[test]
fn nested_variants_roundtrip_and_padding_is_omitted() {
    let value = Event::Record {
        value: Nested {
            tag: 9,
            body: Some(77),
            result: Err('中'),
        },
    };
    let mut buffer = [0xa5; 128];
    let n = encode(&value, &mut buffer).unwrap();
    assert_eq!(decode::<Event>(Event::SCHEMA, &buffer[..n]).unwrap(), value);
    assert!(buffer[n..].iter().all(|x| *x == 0xa5));
    let n = encode(&Padded { a: 1, b: 2 }, &mut buffer).unwrap();
    assert_eq!(n, 9);
    assert_eq!(&buffer[..n], &[1, 2, 0, 0, 0, 0, 0, 0, 0]);
}
#[test]
fn invalid_bytes_cannot_construct_invalid_rust_values() {
    assert_eq!(
        decode::<bool>(bool::SCHEMA, &[2]),
        Err(TransferError::InvalidValue)
    );
    assert_eq!(
        decode::<char>(char::SCHEMA, &0xd800u32.to_le_bytes()),
        Err(TransferError::InvalidValue)
    );
    assert!(matches!(
        decode::<Event>(Event::SCHEMA, &99u32.to_le_bytes()),
        Err(TransferError::InvalidValue)
    ));
    assert!(matches!(
        decode::<Event>(Event::SCHEMA, &[0, 0]),
        Err(TransferError::Truncated)
    ));
    assert!(matches!(
        decode::<Event>(Event::SCHEMA, &[0, 0, 0, 0, 99]),
        Err(TransferError::InvalidValue)
    ));
}
#[test]
fn schema_covers_types_names_widths_and_bounds() {
    assert_ne!(u32::SCHEMA, i32::SCHEMA);
    assert_ne!(SameName::SCHEMA, DifferentName::SCHEMA);
    assert_ne!(BoundedVec::<u8, 4>::SCHEMA, BoundedVec::<u8, 5>::SCHEMA);
    assert_ne!(Nested::<u64>::SCHEMA, Nested::<u32>::SCHEMA);
    assert_eq!(
        decode::<u32>(u64::SCHEMA, &[0; 4]),
        Err(TransferError::SchemaMismatch)
    );
    assert_eq!(<Padded as Wire>::MAX_SIZE, 9);
}
#[test]
fn bounded_vec_transfers_only_live_elements() {
    let value: BoundedVec<u16, 4> = [1, 2].try_into().unwrap();
    let mut out = [0xcc; 32];
    let n = encode(&value, &mut out).unwrap();
    assert_eq!(n, 12);
    assert_eq!(
        decode::<BoundedVec<u16, 4>>(BoundedVec::<u16, 4>::SCHEMA, &out[..n]).unwrap(),
        value
    );
    assert_eq!(
        decode::<BoundedVec<u16, 4>>(BoundedVec::<u16, 4>::SCHEMA, &5u64.to_le_bytes()),
        Err(TransferError::LengthLimit)
    );
}
#[test]
fn pointer_is_an_opaque_address_even_for_non_message_pointee() {
    struct NotExchangeable {
        _owned: String,
    }
    let pointer = 0x1234usize as *const NotExchangeable;
    let mut out = [0; 8];
    let n = encode(&pointer, &mut out).unwrap();
    assert_eq!(
        decode::<*const NotExchangeable>(<*const NotExchangeable>::SCHEMA, &out[..n]).unwrap(),
        pointer
    );
}

#[repr(C, packed)]
#[derive(Message)]
struct Packed {
    tag: u8,
    values: BoundedVec<u32, 4>,
}

#[test]
fn packed_noncopy_fields_use_aligned_snapshots_without_native_padding() {
    let value = Packed {
        tag: 9,
        values: [7, 11].try_into().unwrap(),
    };
    let mut bytes = [0; 64];
    let len = encode(&value, &mut bytes).unwrap();
    assert_eq!(len, 1 + 8 + 2 * 4);
    let restored = decode::<Packed>(Packed::SCHEMA, &bytes[..len]).unwrap();
    let values = restored.values;
    assert_eq!(values, [7, 11].try_into().unwrap());
    // Encoding must leave original non-Copy ownership intact.
    let original_values = value.values;
    assert_eq!(original_values, values);
}
