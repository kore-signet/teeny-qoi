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

// adaptation of https://github.com/droundy/arrayref; license:
/*
Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu>
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the
   distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
*/

mod macros {
    macro_rules! array_ref {
        ($arr:expr, $offset:expr, $len:ident) => {{
            {
                #[inline]
                unsafe fn as_array<const CAP: usize, T>(slice: &[T]) -> &[T; CAP] {
                    &*(slice.as_ptr() as *const [_; CAP])
                }
                let offset = $offset;
                let slice = &$arr[offset..offset + $len];
                #[allow(unused_unsafe)]
                unsafe {
                    as_array::<$len, _>(slice)
                }
            }
        }};
    }

    pub(crate) use array_ref;
}

pub(crate) use macros::*;
