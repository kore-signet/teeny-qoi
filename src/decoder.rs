use crate::*;
use zerocopy::FromBytes;

pub trait ChunkStream {
    type Error;

    fn take_chunk(&mut self) -> Result<Option<Chunk>, Self::Error>;
}

pub struct SliceDecoder<'a> {
    inner: &'a [u8],
    cursor: usize,
}

impl<'a> SliceDecoder<'a> {
    pub fn start(inner: &'a [u8]) -> Option<(Header, SliceDecoder<'a>)> {
        if inner[0..4] != tags::QOI_MAGIC {
            return None;
        };

        let header = Header::read_from(&inner[4..14])?;

        Some((header, SliceDecoder { cursor: 14, inner }))
    }

    pub fn peek(&self) -> Option<&u8> {
        self.inner.get(self.cursor)
    }

    pub fn peek_n<const N: usize>(&self) -> Option<&'a [u8; N]> {
        if self.cursor + N > self.inner.len() {
            return None;
        }

        Some(array_ref!(self.inner, self.cursor, N))
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        let old_cur = self.cursor;
        self.cursor += 1;
        if self.cursor > self.inner.len() {
            return None;
        }

        Some(self.inner[old_cur])
    }

    pub fn read_n<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
        let old_cur = self.cursor;
        self.cursor += N;
        if self.cursor > self.inner.len() {
            return None;
        }

        Some(array_ref!(self.inner, old_cur, N))
    }
}

impl<'a> ChunkStream for SliceDecoder<'a> {
    type Error = ();

    fn take_chunk(&mut self) -> Result<Option<Chunk>, Self::Error> {
        let tag = if let Some(b) = self.read_u8() {
            b
        } else {
            return Ok(None);
        };

        // check if it's one of RGB, RGBA, or 0
        match tag {
            tags::RGB => {
                let [r, g, b] = *self.read_n::<3>().ok_or(())?;

                return Ok(Some(Chunk::Rgb { r, g, b }));
            }
            tags::RGBA => {
                let [r, g, b, a] = *self.read_n::<4>().ok_or(())?;

                return Ok(Some(Chunk::Rgba { r, g, b, a }));
            }
            0 => {
                if self
                    .peek_n::<7>()
                    .filter(|b| b[..] == tags::BYTESTREAM_END[1..])
                    .is_some()
                {
                    return Ok(None);
                }
            }
            _ => (),
        };

        let masked_tag = tag & tags::MASK_2;
        Ok(Some(match masked_tag {
            tags::INDEX => Chunk::Index { idx: tag },
            tags::DIFF => Chunk::Diff {
                dr: ((tag >> 4) & tags::DIFF_MASK) as i8 - 2,
                dg: ((tag >> 2) & tags::DIFF_MASK) as i8 - 2,
                db: (tag & tags::DIFF_MASK) as i8 - 2,
            },
            tags::LUMA => {
                let second_byte = if let Some(b) = self.read_u8() {
                    b
                } else {
                    return Ok(None);
                };

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
        }))
    }
}

pub struct ImageDecoder<T: ChunkStream> {
    inner: T,
    previously_seen: [RgbaPixel; 64],
    previous: RgbaPixel,
}

impl<T: ChunkStream> ImageDecoder<T> {
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
        }
    }

    pub fn emit_pixels(&mut self) -> Result<Option<ArrayVec<RgbaPixel, 62>>, T::Error> {
        let chunk = if let Some(v) = self.inner.take_chunk()? {
            v
        } else {
            return Ok(None);
        };

        let mut out = ArrayVec::new();

        match chunk {
            Chunk::Rgb { r, g, b } => out.push(RgbaPixel {
                r,
                g,
                b,
                a: self.previous.a,
            }),
            Chunk::Rgba { r, g, b, a } => out.push(RgbaPixel { r, g, b, a }),
            Chunk::Index { idx } => out.push(self.previously_seen[idx as usize]),
            Chunk::Luma { dg, dr_dg, db_dg } => out.push(RgbaPixel {
                r: ((self.previous.r as i16) + (dr_dg as i16 + dg as i16)) as u8,
                g: (self.previous.g as i16 + dg as i16) as u8,
                b: ((self.previous.b as i16) + (db_dg as i16 + dg as i16)) as u8,
                a: self.previous.a,
            }),
            Chunk::Diff { dr, dg, db } => {
                out.push(RgbaPixel {
                    r: (self.previous.r as i16 + dr as i16) as u8,
                    g: (self.previous.g as i16 + dg as i16) as u8,
                    b: (self.previous.b as i16 + db as i16) as u8,
                    a: self.previous.a,
                });
            }
            Chunk::Run { length } => {
                for i in 0..length {
                    // i want to avoid doing unsafe here, but to avoid assertions be run everytime, it's kinda needed.
                    unsafe {
                        out.as_mut_ptr().add(i as usize).write(self.previous);
                    }
                }

                unsafe {
                    out.set_len(length as usize);
                }
            }
        };

        self.previous = out[0];
        self.previously_seen[self.previous.index_position() as usize] = out[0];

        Ok(Some(out))
    }
}
