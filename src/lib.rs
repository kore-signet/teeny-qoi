#![cfg_attr(not(feature = "std"), no_std)]

pub use arrayvec::ArrayVec;
use core::mem;
use zerocopy::{AsBytes, BigEndian, FromBytes, U32};

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::Vec;

mod decoder;
mod encoder;
mod helpers;
pub use decoder::*;
pub use encoder::*;
pub use helpers::*;

#[derive(AsBytes, FromBytes, Debug)]
#[repr(C)]
pub struct Header {
    pub width: U32<BigEndian>,
    pub height: U32<BigEndian>,
    pub channels: u8,
    pub colorspace: u8,
}

impl Header {
    pub fn rgb(width: u32, height: u32) -> Header {
        Header {
            width: width.into(),
            height: height.into(),
            channels: 3,
            colorspace: 0,
        }
    }

    pub fn rgba(width: u32, height: u32) -> Header {
        Header {
            width: width.into(),
            height: height.into(),
            channels: 4,
            colorspace: 0,
        }
    }
}

pub mod tags {
    pub const INDEX: u8 = 0x00; /* 00xxxxxx */
    pub const DIFF: u8 = 0x40; /* 01xxxxxx */
    pub const LUMA: u8 = 0x80; /* 10xxxxxx */
    pub const RUN: u8 = 0xc0; /* 11xxxxxx */
    pub const RGB: u8 = 0xfe; /* 11111110 */
    pub const RGBA: u8 = 0xff; /* 11111111 */

    pub const MASK_2: u8 = 0xc0; /* 11000000 */
    pub const DIFF_MASK: u8 = 0x03; /* 00000011 */
    pub const INVERSE_MASK_2: u8 = 0x3f; /* 00111111 */
    pub const LUMA_MASK: u8 = 0x0f; /* 00001111 */

    pub const QOI_MAGIC: [u8; 4] = [b'q', b'o', b'i', b'f'];
    pub const BYTESTREAM_END: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
}

#[inline(always)]
const fn in_diff_range(dr: i8, dg: i8, db: i8) -> bool {
    (dr > -3 && dr < 2) && (dg > -3 && dg < 2) && (db > -3 && db < 2)
}

#[inline(always)]
const fn in_luma_range(dr_dg: i8, dr_db: i8, dg: i8) -> bool {
    (dr_dg > -9 && dr_dg < 8) && (dg > -33 && dg < 32) && (dr_db > -9 && dr_db < 8)
}

#[derive(Debug)]
pub enum Chunk {
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
    Rgba {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    Index {
        idx: u8, // bounded into 0..63
    },
    Diff {
        dr: i8, // bounded into -2..1
        dg: i8, // bounded into  -2..1
        db: i8, // bounded into -2..1
    },
    Luma {
        dg: i8,    // bounded into -32..31
        dr_dg: i8, // bounded into -8..7
        db_dg: i8, // same as dr_db
    },
    Run {
        length: u8, // bounded into 1..62
    },
}

// this macro implements as bytes for Chunk, calling the specified method on the byte slices produced
macro_rules! impl_chunk_as_bytes {
    ($self:ident, $out:ident, $out_method:ident) => {
        match $self {
            Chunk::Rgb { r, g, b } => $out.$out_method(&[tags::RGB, r, g, b]),
            Chunk::Rgba { r, g, b, a } => $out.$out_method(&[tags::RGBA, r, g, b, a]),
            Chunk::Index { idx } => $out.$out_method(&[tags::INDEX | idx]),
            Chunk::Diff { dr, dg, db } => $out.$out_method(&[tags::DIFF
                | ((dr + 2) as u8) << 4
                | ((dg + 2) as u8) << 2
                | (db + 2) as u8]),
            Chunk::Luma { dg, dr_dg, db_dg } => $out.$out_method(&[
                tags::LUMA | ((dg + 32) as u8),
                ((dr_dg + 8) as u8) << 4 | (db_dg + 8) as u8,
            ]),
            Chunk::Run { length } => $out.$out_method(&[tags::RUN | (length - 1)]),
        }
    };
}

impl Chunk {
    // returns None on failure
    #[inline(always)]
    pub fn write_to_arrayvec<const CAP: usize>(self, out: &mut ArrayVec<u8, CAP>) -> Option<()> {
        impl_chunk_as_bytes!(self, out, try_extend_from_slice).ok()
    }

    #[cfg(any(feature = "alloc", feature = "std"))]
    #[inline(always)]
    pub fn write_to_vec(self, out: &mut Vec<u8>) {
        impl_chunk_as_bytes!(self, out, extend_from_slice);
    }

    #[cfg(any(feature = "alloc", feature = "std"))]
    #[inline(always)]
    pub fn to_vec(self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::with_capacity(5);
        self.write_to_vec(&mut out);
        out
    }

    #[cfg(feature = "std")]
    #[inline(always)]
    pub fn write_into(self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        impl_chunk_as_bytes!(self, w, write_all)
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, PartialEq, Debug)]
#[repr(C)]
pub struct RgbaPixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaPixel {
    #[inline(always)]
    pub fn index_position(&self) -> u8 {
        use core::num::Wrapping;
        let (r, g, b, a) = (
            Wrapping(self.r),
            Wrapping(self.g),
            Wrapping(self.b),
            Wrapping(self.a),
        );
        (r * Wrapping(3u8) + g * Wrapping(5u8) + b * Wrapping(7u8) + a * Wrapping(11u8)).0 % 64
    }
}

impl From<[u8; 4]> for RgbaPixel {
    fn from([r, g, b, a]: [u8; 4]) -> Self {
        RgbaPixel { r, g, b, a }
    }
}

impl From<[u8; 3]> for RgbaPixel {
    fn from([r, g, b]: [u8; 3]) -> Self {
        RgbaPixel { r, g, b, a: 255 }
    }
}

impl From<(u8, u8, u8, u8)> for RgbaPixel {
    fn from((r, g, b, a): (u8, u8, u8, u8)) -> Self {
        RgbaPixel { r, g, b, a }
    }
}

impl From<(u8, u8, u8)> for RgbaPixel {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        RgbaPixel { r, g, b, a: 255 }
    }
}
