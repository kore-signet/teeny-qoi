#![cfg_attr(not(feature = "std"), no_std)]

pub use arrayvec::ArrayVec;
use core::mem;
use zerocopy::{AsBytes, BigEndian, FromBytes, U32};

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::Vec;

mod helpers;
pub use helpers::*;

#[derive(AsBytes, FromBytes)]
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
    pub const INDEX: u8 = 0x00;
    pub const DIFF: u8 = 0x40;
    pub const LUMA: u8 = 0x80;
    pub const RUN: u8 = 0xc0;
    pub const RGB: u8 = 0xfe;
    pub const RGBA: u8 = 0xff;

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

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct RgbaPixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaPixel {
    #[inline(always)]
    pub fn index_position(&self) -> u8 {
        use std::num::Wrapping;
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

pub struct Encoder {
    previously_seen: [RgbaPixel; 64],
    previous: RgbaPixel,
    run: u8,
    index: u32,
    length: u32,
    pub header: Header,
}

impl Encoder {
    pub fn new(header: Header) -> Encoder {
        Encoder {
            previously_seen: [RgbaPixel {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }; 64],
            previous: RgbaPixel {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            run: 0,
            index: 0,
            length: header.width.get() * header.height.get(),
            header,
        }
    }

    pub fn process_pixel(&mut self, pixel: RgbaPixel) -> ArrayVec<Chunk, 2> {
        let mut output = ArrayVec::new_const();
        self.index += 1;

        // if pixel is the same as the last one, possibly emit a Run operation and return
        if pixel == self.previous {
            self.run += 1;

            // if the run is 62, we've reached the maximum len QOI allows
            // else, if we're at the end of the image, we need to emit a last chunk containing the current run
            if self.run == 62 || self.index == self.length {
                output.push(Chunk::Run {
                    length: mem::take(&mut self.run),
                });
            }

            self.previous = pixel;

            return output;
        }

        // if pixel is different:

        // first, reset the run if one exists
        if self.run > 0 {
            output.push(Chunk::Run {
                length: mem::take(&mut self.run),
            });
            self.run = 0;
        }

        // if pixel is in the previously seen array, return an Index operation and return
        let index_pos = pixel.index_position();
        if self.previously_seen[index_pos as usize] == pixel {
            output.push(Chunk::Index { idx: index_pos });
            self.previous = pixel;
            return output;
        }

        // if it isn't, add it to it!
        self.previously_seen[index_pos as usize] = pixel;

        // if the alpha channel matches the previous pixel:
        if pixel.a == self.previous.a {
            // pixel val diffs
            let dr: i8 = pixel.r.wrapping_sub(self.previous.r) as i8;
            let dg: i8 = pixel.g.wrapping_sub(self.previous.g) as i8;
            let db: i8 = pixel.b.wrapping_sub(self.previous.b) as i8;

            // diffs between the red and blue diffs and the green diff
            let dr_dg = dr.wrapping_sub(dg) as i8;
            let db_dg = db.wrapping_sub(dg) as i8;

            output.push(if in_diff_range(dr, dg, db) {
                Chunk::Diff { dr, dg, db }
            } else if in_luma_range(dr_dg, db_dg, dg) {
                Chunk::Luma { dg, dr_dg, db_dg }
            } else {
                Chunk::Rgb {
                    r: pixel.r,
                    g: pixel.g,
                    b: pixel.b,
                }
            });
        // if we have a new alpha value:
        } else {
            output.push(Chunk::Rgba {
                r: pixel.r,
                g: pixel.g,
                b: pixel.b,
                a: pixel.a,
            });
        }

        self.previous = pixel;

        output
    }

    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn image_to_vec<T, I>(mut self, image: I) -> Vec<u8>
    where
        T: Into<RgbaPixel>,
        I: IntoIterator<Item = T>,
    {
        // width * height * channels+1 + header size + bytestream end size
        let mut out = Vec::with_capacity(
            (self.header.width.get() * self.header.height.get() * (self.header.channels + 1) as u32
                + 14
                + 8) as usize,
        );

        out.extend_from_slice(&tags::QOI_MAGIC);
        out.extend_from_slice(self.header.as_bytes());

        for pixel in image {
            for chunk in self.process_pixel(pixel.into()) {
                chunk.write_to_vec(&mut out);
            }
        }

        out.extend_from_slice(&tags::BYTESTREAM_END);

        out
    }

    #[cfg(feature = "std")]
    pub fn write_image<T, I, W>(mut self, image: I, out: &mut W) -> std::io::Result<()>
    where
        T: Into<RgbaPixel>,
        I: IntoIterator<Item = T>,
        W: std::io::Write,
    {
        out.write_all(&tags::QOI_MAGIC)?;
        out.write_all(self.header.as_bytes())?;

        for pixel in image {
            for chunk in self.process_pixel(pixel.into()) {
                chunk.write_into(out)?;
            }
        }

        out.write_all(&tags::BYTESTREAM_END)?;

        Ok(())
    }
}
