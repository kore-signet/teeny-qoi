use image::RgbaImage;
use std::env;
use teeny_qoi::decoder::SliceReader;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        panic!("usage: qoi2png input.qoi output.png");
    }

    let input = std::fs::read(&args[0]).expect("couldn't read input");
    let (header, reader) = SliceReader::start(&input[..]).expect("invalid qoi file");
    let image = RgbaImage::from_vec(
        header.width.get(),
        header.height.get(),
        reader.into_decoder().into_rgba_bytes().collect::<Vec<u8>>(),
    )
    .expect("couldn't create output image - wrong size?");
    img.save(&args[1]).expect("couldn't write image");
}
