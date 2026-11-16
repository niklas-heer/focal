//! How many image pixels make one screen point: 2 for a Retina screenshot,
//! which macOS saves at 144 pixels per inch, or for a file named
//! `name@2x.png`; otherwise 1. Images are drawn at their size in points, as
//! Preview's "Actual Size" shows them.

/// The pixel density of the image named `name` whose file starts with
/// `header` (its first bytes are enough).
pub fn density(name: &str, header: &[u8]) -> f32 {
    if let Some(scale) = scale_suffix(name) {
        return scale;
    }
    png_density(header).unwrap_or(1.)
}

/// `2` for `chart@2x.png`, `3` for `chart@3x.png`.
fn scale_suffix(name: &str) -> Option<f32> {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let (_, scale) = stem.rsplit_once('@')?;
    match scale {
        "2x" => Some(2.),
        "3x" => Some(3.),
        _ => None,
    }
}

/// The density a PNG's `pHYs` chunk gives, from its pixels per metre, as a
/// multiple of 72 per inch: only Retina's whole multiples count, so an
/// odd 300 ppi scan keeps its pixels.
fn png_density(bytes: &[u8]) -> Option<f32> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    let mut rest = bytes.strip_prefix(SIGNATURE)?;
    while rest.len() >= 12 {
        let length = u32::from_be_bytes(rest[0..4].try_into().ok()?) as usize;
        let kind = &rest[4..8];
        if kind == b"IDAT" || kind == b"IEND" {
            return None;
        }
        if kind == b"pHYs" && length == 9 && rest.len() >= 17 {
            let x = u32::from_be_bytes(rest[8..12].try_into().ok()?);
            let y = u32::from_be_bytes(rest[12..16].try_into().ok()?);
            let metres = rest[16] == 1;
            if !metres || x != y {
                return None;
            }
            let per_inch = f64::from(x) * 0.0254;
            let multiple = (per_inch / 72.).round();
            let whole = (per_inch / 72. - multiple).abs() < 0.05;
            #[allow(clippy::cast_possible_truncation)]
            return (whole && (2. ..=3.).contains(&multiple)).then_some(multiple as f32);
        }
        rest = rest.get(12 + length..)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_with_phys(per_metre: u32) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(13u32.to_be_bytes());
        png.extend(b"IHDR");
        png.extend([0; 13]);
        png.extend([0; 4]);
        png.extend(9u32.to_be_bytes());
        png.extend(b"pHYs");
        png.extend(per_metre.to_be_bytes());
        png.extend(per_metre.to_be_bytes());
        png.push(1);
        png.extend([0; 4]);
        png
    }

    #[test]
    fn a_retina_screenshot_is_two_pixels_a_point() {
        assert!((density("shot.png", &png_with_phys(5669)) - 2.).abs() < f32::EPSILON);
    }

    #[test]
    fn ordinary_images_are_one_pixel_a_point() {
        assert!((density("photo.png", &png_with_phys(2835)) - 1.).abs() < f32::EPSILON);
        // A 300 ppi scan keeps its pixels.
        assert!((density("scan.png", &png_with_phys(11811)) - 1.).abs() < f32::EPSILON);
        assert!((density("photo.jpg", b"\xff\xd8") - 1.).abs() < f32::EPSILON);
    }

    #[test]
    fn a_scale_in_the_name_counts() {
        assert!((density("icon@2x.png", b"") - 2.).abs() < f32::EPSILON);
        assert!((density("icon@3x.jpeg", b"") - 3.).abs() < f32::EPSILON);
        assert!((density("me@home.png", b"") - 1.).abs() < f32::EPSILON);
    }
}
