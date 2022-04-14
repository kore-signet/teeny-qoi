use crate::RgbaPixel;
use core::slice::ChunksExact;

pub struct RgbaBytesAdapater<'a> {
    inner: ChunksExact<'a, u8>,
}

impl<'a> Iterator for RgbaBytesAdapater<'a> {
    type Item = RgbaPixel;

    fn next(&mut self) -> Option<RgbaPixel> {
        let chunk = self.inner.next()?;
        Some(RgbaPixel {
            r: chunk[0],
            g: chunk[1],
            b: chunk[2],
            a: chunk[3],
        })
    }
}

impl<'a> From<&'a [u8]> for RgbaBytesAdapater<'a> {
    fn from(slice: &'a [u8]) -> RgbaBytesAdapater {
        RgbaBytesAdapater {
            inner: slice.chunks_exact(4),
        }
    }
}
