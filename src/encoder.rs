//! An encoder that turns RGBA bytes into a QOI file.

use crate::*;

/// A QOI encoder.
pub struct Encoder {
    previously_seen: [RgbaPixel; 64],
    previous: RgbaPixel,
    run: u8,
    index: u32,
    length: u32,
    pub header: Header,
}

impl Encoder {
    /// Builds an encoder from a header.
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

    /// Processes a pixel, emitting one to two chunks.
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

    /// Turns an iterator over RgbaPixels (or things that can be converted into RgbaPixels) into a Vec<u8> of QOI bytes.
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

    /// Writes out an iterator over RgbaPixels (or things that can be converted into RgbaPixels) as QOI bytes into a [std::io::Write]
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
