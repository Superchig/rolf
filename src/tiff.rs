// The beginning of a TIFF parser, capable of finding the orientation

// Handy links:
// https://lars.ingebrigtsen.no/2019/09/22/parsing-exif-data/
// https://www.adobe.io/content/dam/udp/en/open/standards/tiff/TIFF6.pdf
// https://www.cipa.jp/std/documents/e/DC-X008-Translation-2019-E.pdf
// https://www.cipa.jp/std/documents/e/DC-008-2012_E.pdf

#[derive(Debug, PartialEq, Eq)]
pub struct IFDEntry {
    pub tag: EntryTag,
    pub field_type: EntryType,
    pub count: u32,
    pub value_offset: u32,
}

impl IFDEntry {
    pub fn from_slice(ifd_bytes: &[u8], byte_order: Endian) -> IFDEntry {
        let mut ifd_advance = 0;

        // Bytes 0-1
        let entry_tag = usizeify(take_bytes(ifd_bytes, &mut ifd_advance, 2), byte_order);

        assert_eq!(ifd_advance, 2);

        let field_type_hex = take_bytes(ifd_bytes, &mut ifd_advance, 2);
        let field_type = usizeify(field_type_hex, byte_order);
        let field_type_enum = EntryType::from_usize(field_type);

        // NOTE(Chris): Count is not the total number of bytes, but rather the number of values
        // (the length of which is specified by the field type)
        let count = usizeify(take_bytes(ifd_bytes, &mut ifd_advance, 4), byte_order);

        let byte_count = if let EntryType::Short = field_type_enum {
            count * field_type_enum.byte_count()
        } else {
            4
        };

        let value_offset = usizeify_n(
            take_bytes(ifd_bytes, &mut ifd_advance, 4),
            byte_order,
            byte_count,
        );

        IFDEntry {
            tag: EntryTag::from_usize(entry_tag),
            field_type: field_type_enum,
            count: count as u32,
            value_offset: value_offset as u32,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum EntryTag {
    Orientation = 274,
    Unimplemented,
}

impl EntryTag {
    fn from_usize(value: usize) -> EntryTag {
        match value {
            274 => EntryTag::Orientation,
            _ => EntryTag::Unimplemented,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EntryType {
    Short = 3,
    Unimplemented,
}

impl EntryType {
    fn from_usize(value: usize) -> EntryType {
        match value {
            3 => EntryType::Short,
            _ => EntryType::Unimplemented,
        }
    }

    fn byte_count(self) -> usize {
        match self {
            EntryType::Short => 2,
            _ => panic!("byte count not defined for {:?}", self),
        }
    }
}

fn take_bytes<'a>(bytes: &'a [u8], byte_advance: &mut usize, n: usize) -> &'a [u8] {
    let old_advance = *byte_advance;

    *byte_advance += n;

    &bytes[old_advance..old_advance + n]
}

#[derive(Debug, Copy, Clone)]
pub enum Endian {
    LittleEndian,
    BigEndian,
}

// Converts a slice of bytes into a usize, depending on the Endianness
// NOTE(Chris): It seems like we could probably do this faster by using an unsafe copy of memory
// from the slice into a usize value.
pub fn usizeify(bytes: &[u8], byte_order: Endian) -> usize {
    match byte_order {
        Endian::LittleEndian => bytes.iter().enumerate().fold(0usize, |sum, (index, byte)| {
            sum + ((*byte as usize) << (index * 8))
        }),
        Endian::BigEndian => bytes
            .iter()
            .rev()
            .enumerate()
            .fold(0usize, |sum, (index, byte)| {
                sum + ((*byte as usize) << (index * 8))
            }),
    }
}

fn usizeify_n(bytes: &[u8], byte_order: Endian, n: usize) -> usize {
    match byte_order {
        Endian::LittleEndian => bytes
            .iter()
            .take(n)
            .enumerate()
            .fold(0usize, |sum, (index, byte)| {
                sum + ((*byte as usize) << (index * 8))
            }),
        Endian::BigEndian => bytes
            .iter()
            .take(n)
            .rev()
            .enumerate()
            .fold(0usize, |sum, (index, byte)| {
                sum + ((*byte as usize) << (index * 8))
            }),
    }
}

// fn find_bytes_bool(haystack: &[u8], needle: &[u8]) -> bool {
//     match find_bytes(haystack, needle) {
//         Some(_) => true,
//         None => false,
//     }
// }

pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let mut count = 0;
    for (index, byte) in haystack.iter().enumerate() {
        if *byte == needle[count] {
            count += 1;
        } else {
            count = 0;
        }

        if count == needle.len() {
            // Add 1 because index is 0-based but needle.len() is not
            return Some(index - needle.len() + 1);
        }
    }

    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_find_bytes() {
        let haystack = b"blah blah blah \x01\x01\x24\x59\xab\xde\xad\xbe\xef wow this is something";
        let needle = b"Exif\x00\x00";

        assert_eq!(find_bytes(haystack, needle), None);

        let haystack2 = b"blah blah blah \x01\x01\x24\x59\xabExif\x00\x00\xde\xad\xbe\xef wow this
            is something";

        assert_eq!(find_bytes(haystack2, needle), Some(20));
        if let Some(index) = find_bytes(haystack2, needle) {
            assert_eq!(&haystack2[index..index + needle.len()], needle);
        }
    }

    #[test]
    fn test_usizeify() {
        assert_eq!(usizeify(b"\x12\x34\x56\x78", Endian::BigEndian), 305419896);
        assert_eq!(
            usizeify(b"\x78\x56\x34\x12", Endian::LittleEndian),
            305419896
        );

        assert_eq!(usizeify(b"\x00\x06", Endian::BigEndian), 6);
    }

    #[test]
    fn test_usizeify_n() {
        assert_eq!(usizeify_n(b"\x00\x06\x00\x00", Endian::BigEndian, 2), 6);
    }

    #[test]
    fn test_field_type_byte_count() {
        // NOTE(Chris): According to the TIFF 6.0 specification page 15, field type 3 is a short
        // (16-bit unsigned integer)
        let entry_field_type = EntryType::from_usize(3);

        assert_eq!(entry_field_type, EntryType::Short);

        assert_eq!(entry_field_type.byte_count(), 2);
    }

    #[test]
    fn test_from_slice_big_endian() {
        let bytes = [
            0x01, 0x12, 0x0, 0x3, 0x0, 0x0, 0x0, 0x1, 0xde, 0xad, 0xc0, 0xde,
        ];

        assert_eq!(usizeify(&bytes[0..=1], Endian::BigEndian), 274);
        assert_eq!(usizeify(&bytes[2..=3], Endian::BigEndian), 3);
        assert_eq!(usizeify(&bytes[4..=7], Endian::BigEndian), 1);
        assert_eq!(usizeify(&bytes[8..=11], Endian::BigEndian), 0xdeadc0de);

        let ifd_entry = IFDEntry::from_slice(&bytes, Endian::BigEndian);
        assert_eq!(
            ifd_entry,
            IFDEntry {
                tag: EntryTag::Orientation,
                field_type: EntryType::Short,
                count: 1,
                // NOTE(Chris): In this case, the first two bytes of the "value offset" will be used as
                // the value. On page 15 of the TIFF 6.0 specification, it says "the Value Offset
                // contains the Value instead of pointing to the Value if and only if the Value fits
                // into 4 bytes." Since we are storing 1 short (and a short is 2 bytes), our value
                // easily fits into 4 bytes.
                // NOTE(Chris): For the orientation tag, we would realistically want a value between 0
                // and 8, inclusive. We use this value instead for the sake of testing.
                value_offset: 0xdead,
            }
        );
    }

    #[test]
    fn test_from_slice_little_endian() {
        let bytes = [
            0x012, 0x1, 0x3, 0x0, 0x1, 0x0, 0x0, 0x0, 0xad, 0xde, 0x00, 0x00,
        ];

        assert_eq!(usizeify(&bytes[0..=1], Endian::LittleEndian), 274);
        assert_eq!(usizeify(&bytes[2..=3], Endian::LittleEndian), 3);
        assert_eq!(usizeify(&bytes[4..=7], Endian::LittleEndian), 1);
        // NOTE(Chris): 0xdead == 0x0000dead
        // NOTE(Chris): This is because we typically write numbers in big-endian.
        assert_eq!(usizeify_n(&bytes[8..=11], Endian::LittleEndian, 2), 0x0000dead);
        assert_eq!(usizeify_n(&bytes[8..=11], Endian::LittleEndian, 4), 0x0000dead);

        let ifd_entry = IFDEntry::from_slice(&bytes, Endian::LittleEndian);
        assert_eq!(
            ifd_entry,
            IFDEntry {
                tag: EntryTag::Orientation,
                field_type: EntryType::Short,
                count: 1,
                value_offset: 0xdead,
            }
        );
    }
}
