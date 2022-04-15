//! QOI Decoder implementation.

use crate::*;
use zerocopy::FromBytes;

/// Simple abstraction over a slice to help with reading
pub struct SliceReader<'a> {
    inner: &'a [u8],
    cursor: usize,
}

impl<'a> SliceReader<'a> {
    /// Initializes the reader, returning the QOI Header and a Reader struct if it's a valid QOI file.
    pub fn start(inner: &'a [u8]) -> Option<(Header, SliceReader<'a>)> {
        if inner[0..4] != tags::QOI_MAGIC {
            return None;
        };

        let header = Header::read_from(&inner[4..14])?;

        Some((header, SliceReader { cursor: 14, inner }))
    }

    /// Transforms reader into an image decoder.
    pub fn into_decoder(self) -> ImageDecoder<SliceReader<'a>> {
        ImageDecoder::new(self)
    }
    
    fn peek_n<const N: usize>(&self) -> Option<&'a [u8; N]> {
        if self.cursor + N > self.inner.len() {
            return None;
        }

        Some(array_ref!(self.inner, self.cursor, N))
    }

    fn read_u8(&mut self) -> Option<u8> {
        let old_cur = self.cursor;
        self.cursor += 1;
        if self.cursor > self.inner.len() {
            return None;
        }

        Some(self.inner[old_cur])
    }

    fn read_n<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
        let old_cur = self.cursor;
        self.cursor += N;
        if self.cursor > self.inner.len() {
            return None;
        }

        Some(array_ref!(self.inner, old_cur, N))
    }
}

impl<'a> Iterator for SliceReader<'a> {
    type Item = Chunk;

    fn next(&mut self) -> Option<Chunk> {
        let tag = self.read_u8()?;

        // check if it's one of RGB, RGBA, or 0
        match tag {
            tags::RGB => {
                let [r, g, b] = *self.read_n::<3>()?;

                return Some(Chunk::Rgb { r, g, b });
            }
            tags::RGBA => {
                let [r, g, b, a] = *self.read_n::<4>()?;

                return Some(Chunk::Rgba { r, g, b, a });
            }
            0 => {
                if self
                    .peek_n::<7>()
                    .filter(|b| b[..] == tags::BYTESTREAM_END[1..])
                    .is_some()
                {
                    return None;
                }
            }
            _ => (),
        };

        let masked_tag = tag & tags::MASK_2;
        Some(match masked_tag {
            tags::INDEX => Chunk::Index { idx: tag },
            tags::DIFF => Chunk::Diff {
                dr: ((tag >> 4) & tags::DIFF_MASK) as i8 - 2,
                dg: ((tag >> 2) & tags::DIFF_MASK) as i8 - 2,
                db: (tag & tags::DIFF_MASK) as i8 - 2,
            },
            tags::LUMA => {
                let second_byte = self.read_u8()?;
                Chunk::Luma {
                    dg: (tag & tags::INVERSE_MASK_2) as i8 - 32,
                    dr_dg: ((second_byte >> 4) & tags::LUMA_MASK) as i8 - 8,
                    db_dg: (second_byte & tags::LUMA_MASK) as i8 - 8,
                }
            }
            tags::RUN => Chunk::Run {
                length: (tag & tags::INVERSE_MASK_2) + 1,
            },
            _ => unreachable!(),
        })
    }
}

/// A QOI Decoder, built over an Iterator of QOI operation chunks.
pub struct ImageDecoder<T: Iterator<Item = Chunk>> {
    inner: T,
    previously_seen: [RgbaPixel; 64],
    previous: RgbaPixel,
    run: u8,
}

impl<T: Iterator<Item = Chunk>> ImageDecoder<T> {
    /// Creates a QOI Decoder over an iterator of QOI operation chunks.
    pub fn new(inner: T) -> ImageDecoder<T> {
        ImageDecoder {
            inner,
            previously_seen: [RgbaPixel {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }; 64],
            previous: RgbaPixel {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            run: 0,
        }
    }

    /// Turns decoder into an iterator of RGBA bytes.
    pub fn into_rgba_bytes(self) -> PixelsToRgbaBytes<ImageDecoder<T>> {
        PixelsToRgbaBytes {
            inner: self,
            buf: ArrayVec::new_const(),
        }
    }
}

impl<T: Iterator<Item = Chunk>> Iterator for ImageDecoder<T> {
    type Item = RgbaPixel;

    fn next(&mut self) -> Option<RgbaPixel> {
        if self.run > 0 {
            self.run -= 1;
            return Some(self.previous);
        }

        let next_pixel = match self.inner.next()? {
            Chunk::Rgb { r, g, b } => RgbaPixel {
                r,
                g,
                b,
                a: self.previous.a,
            },
            Chunk::Rgba { r, g, b, a } => RgbaPixel { r, g, b, a },
            Chunk::Index { idx } => self.previously_seen[idx as usize],
            Chunk::Luma { dg, dr_dg, db_dg } => RgbaPixel {
                r: ((self.previous.r as i16) + (dr_dg as i16 + dg as i16)) as u8,
                g: (self.previous.g as i16 + dg as i16) as u8,
                b: ((self.previous.b as i16) + (db_dg as i16 + dg as i16)) as u8,
                a: self.previous.a,
            },
            Chunk::Diff { dr, dg, db } => RgbaPixel {
                r: (self.previous.r as i16 + dr as i16) as u8,
                g: (self.previous.g as i16 + dg as i16) as u8,
                b: (self.previous.b as i16 + db as i16) as u8,
                a: self.previous.a,
            },
            Chunk::Run { length } => {
                self.run = length - 1;
                self.previous
            }
        };

        self.previous = next_pixel;
        self.previously_seen[next_pixel.index_position() as usize] = next_pixel;

        Some(next_pixel)
    }
}

/// Small adapter to flatten out RgbaPixel's into RGBA bytes.
pub struct PixelsToRgbaBytes<T: Iterator<Item = RgbaPixel>> {
    inner: T,
    buf: ArrayVec<u8, 4>,
}

impl<T: Iterator<Item = RgbaPixel>> Iterator for PixelsToRgbaBytes<T> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if self.buf.is_empty() {
            let next_pixel = self.inner.next()?;
            self.buf = ArrayVec::from([next_pixel.a, next_pixel.b, next_pixel.g, next_pixel.r]);
        }

        self.buf.pop()
    }
}
