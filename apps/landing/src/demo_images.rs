use telar::ImageData;

pub fn make_gradient(width: u32, height: u32) -> ImageData {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let tx = x as f32 / (width - 1) as f32;
            let ty = y as f32 / (height - 1) as f32;
            let r = (tx * 235.0 + 20.0) as u8;
            let g = (ty * 120.0 + 40.0) as u8;
            let b = ((1.0 - tx) * 235.0 + 20.0) as u8;
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    ImageData::new(pixels, width, height)
}

pub fn make_checker(width: u32, height: u32, cell: u32) -> ImageData {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let on = ((x / cell) + (y / cell)) % 2 == 0;
            if on {
                pixels.extend_from_slice(&[238, 240, 250, 255]);
            } else {
                pixels.extend_from_slice(&[67, 97, 238, 255]);
            }
        }
    }
    ImageData::new(pixels, width, height)
}

pub fn make_radial_alpha(width: u32, height: u32) -> ImageData {
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let radius = cx.min(cy) - 2.0;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let t = ((radius - dist) / radius).clamp(0.0, 1.0);
            let r = (240.0 - t * 60.0) as u8;
            let g = (90.0 + t * 90.0) as u8;
            let b = (230.0) as u8;
            pixels.extend_from_slice(&[r, g, b, (t * 255.0) as u8]);
        }
    }
    ImageData::new(pixels, width, height)
}
