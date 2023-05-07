use itertools::Itertools;
use std::fmt::Display;
use std::ops::{Deref, DerefMut};

#[derive(Debug, PartialEq, Eq)]
enum HexII {
    Ascii(char),
    Byte(u8),
    Null,
    Full,
    Eof,
}

impl From<&u8> for HexII {
    fn from(v: &u8) -> Self {
        match v {
            0x00 => Self::Null,
            0xFF => Self::Full,
            c if c.is_ascii_graphic() => Self::Ascii(*c as char),
            v => Self::Byte(*v),
        }
    }
}

impl Display for HexII {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HexII::Ascii(v) => write!(f, ".{}", v)?,
            HexII::Byte(v) => write!(f, "{:02x}", v)?,
            HexII::Null => write!(f, "  ")?,
            HexII::Full => write!(f, "##")?,
            HexII::Eof => write!(f, " ]")?,
        }
        Ok(())
    }
}

struct HexIILine(Vec<HexII>);

impl Display for HexIILine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, v) in self.0.iter().enumerate() {
            if i != 0 {
                write!(f, " ")?;
            }
            write!(f, "{}", v)?;
        }
        Ok(())
    }
}

impl From<&[u8]> for HexIILine {
    fn from(l: &[u8]) -> Self {
        Self(l.iter().map(HexII::from).collect())
    }
}

impl Deref for HexIILine {
    type Target = Vec<HexII>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for HexIILine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub fn hex_ii_dump<T: Iterator<Item = u8>>(data: T, base_offset: usize, total: usize) {
    const CHUNK_SIZE: usize = 0x10;
    let mut num_digits = (std::mem::size_of_val(&total) * 8) - (total.leading_zeros() as usize);
    if (num_digits % 8) != 0 {
        num_digits += 8 - (num_digits % 8)
    }
    num_digits >>= 2;
    for (mut offset, line) in data.chunks(CHUNK_SIZE).into_iter().enumerate() {
        offset += base_offset;
        let mut line = HexIILine::from(line.collect::<Vec<_>>().as_slice());
        if line.len() < CHUNK_SIZE {
            line.push(HexII::Eof);
        }
        while line.len() < CHUNK_SIZE {
            line.push(HexII::Null);
        }
        if line.iter().all(|v| v == &HexII::Null) {
            continue;
        }
        let offset = format!("{:digits$x}", offset * CHUNK_SIZE, digits = num_digits);
        println!("{} | {:<16} |", offset, line);
    }
}
