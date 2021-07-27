// The beginning of a TIFF parser, capable of finding the orientation

// Handy links:
// https://lars.ingebrigtsen.no/2019/09/22/parsing-exif-data/
// https://www.adobe.io/content/dam/udp/en/open/standards/tiff/TIFF6.pdf
// https://www.cipa.jp/std/documents/e/DC-X008-Translation-2019-E.pdf
// https://www.cipa.jp/std/documents/e/DC-008-2012_E.pdf


#[derive(Debug)]
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
            &take_bytes(ifd_bytes, &mut ifd_advance, 4),
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
}
