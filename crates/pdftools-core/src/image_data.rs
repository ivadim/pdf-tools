//! Converting between PDF image XObjects and decoded pixels.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use crate::Error;

/// Number of colour components of a colour space we know how to convert,
/// or `None` for anything else (Indexed, Lab, Separation, …).
pub(crate) fn color_components(doc: &Document, space: &Object) -> Option<u8> {
    let space = doc.dereference(space).ok()?.1;
    match space {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some(1),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(3),
            b"DeviceCMYK" | b"CMYK" => Some(4),
            _ => None,
        },
        Object::Array(items) => match items.first()?.as_name().ok()? {
            b"ICCBased" => {
                let profile = doc.dereference(items.get(1)?).ok()?.1.as_stream().ok()?;
                profile.dict.get(b"N").and_then(Object::as_i64).ok().map(|n| n as u8)
            }
            b"CalRGB" => Some(3),
            b"CalGray" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

/// The stream's filter names, in decoding order.
pub(crate) fn filters(stream: &Stream) -> Vec<Vec<u8>> {
    match stream.dict.get(b"Filter") {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(items)) => items.iter().filter_map(|o| o.as_name().ok().map(<[u8]>::to_vec)).collect(),
        _ => Vec::new(),
    }
}

/// The stream's data with all (supported) filters removed.
pub(crate) fn plain_content(stream: &Stream) -> Option<Vec<u8>> {
    if filters(stream).is_empty() {
        Some(stream.content.clone())
    } else {
        stream.decompressed_content().ok()
    }
}

pub(crate) struct DecodedImage {
    pub image: DynamicImage,
    pub was_jpeg: bool,
    /// The pixels are no longer in the stream's colour space (CMYK was
    /// converted to RGB), so `/ColorSpace` must be replaced.
    pub converted_from_cmyk: bool,
}

/// Decodes a plain 8-bit grey, RGB or CMYK image XObject. Masks, images with
/// `/Decode` arrays and unusual encodings are left alone (`None`).
pub(crate) fn decode_image(doc: &Document, stream: &Stream) -> Option<DecodedImage> {
    let dict = &stream.dict;
    if dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Image")
        || dict.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false)
        || dict.has(b"Decode")
        || dict.get(b"BitsPerComponent").and_then(Object::as_i64).ok() != Some(8)
    {
        return None;
    }
    let width = u32::try_from(dict.get(b"Width").and_then(Object::as_i64).ok()?).ok()?;
    let height = u32::try_from(dict.get(b"Height").and_then(Object::as_i64).ok()?).ok()?;
    let components = color_components(doc, dict.get(b"ColorSpace").ok()?)?;
    if width == 0 || height == 0 {
        return None;
    }

    let filters = filters(stream);
    if filters == [b"DCTDecode".to_vec()] {
        let image = image::load_from_memory_with_format(&stream.content, ImageFormat::Jpeg).ok()?;
        if image.width() != width || image.height() != height {
            return None;
        }
        let image = match components {
            1 => DynamicImage::ImageLuma8(image.to_luma8()),
            _ => DynamicImage::ImageRgb8(image.to_rgb8()),
        };
        // The decoder turns CMYK JPEGs into RGB.
        return Some(DecodedImage { image, was_jpeg: true, converted_from_cmyk: components == 4 });
    }
    if !(filters.is_empty() || filters == [b"FlateDecode".to_vec()]) {
        return None;
    }

    let data = plain_content(stream)?;
    let pixels = (width as usize) * (height as usize);
    let needed = pixels * components as usize;
    if data.len() < needed {
        return None;
    }
    let data = &data[..needed];
    let (image, converted_from_cmyk) = match components {
        1 => (DynamicImage::ImageLuma8(GrayImage::from_raw(width, height, data.to_vec())?), false),
        3 => (DynamicImage::ImageRgb8(RgbImage::from_raw(width, height, data.to_vec())?), false),
        4 => {
            let rgb = data.chunks_exact(4).flat_map(|p| cmyk_to_rgb(p[0], p[1], p[2], p[3])).collect();
            (DynamicImage::ImageRgb8(RgbImage::from_raw(width, height, rgb)?), true)
        }
        _ => return None,
    };
    Some(DecodedImage { image, was_jpeg: false, converted_from_cmyk })
}

pub(crate) fn cmyk_to_rgb(c: u8, m: u8, y: u8, k: u8) -> [u8; 3] {
    let channel = |v: u8| ((255 - v as u32) * (255 - k as u32) / 255) as u8;
    [channel(c), channel(m), channel(y)]
}

pub(crate) fn jpeg_bytes(image: &DynamicImage, quality: u8) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality).encode_image(image).ok()?;
    Some(bytes)
}

/// Embeds a PNG or JPEG as an image XObject (with a soft mask for
/// transparency) and returns its id and pixel size.
pub(crate) fn embed_image(doc: &mut Document, bytes: &[u8]) -> Result<(ObjectId, u32, u32), Error> {
    let format = image::guess_format(bytes).map_err(|e| Error::Image(e.to_string()))?;
    let decoded = image::load(Cursor::new(bytes), format).map_err(|e| Error::Image(e.to_string()))?;
    let (width, height) = (decoded.width(), decoded.height());

    // JPEGs without alpha can be embedded as they are.
    if format == ImageFormat::Jpeg && !decoded.color().has_alpha() {
        let space = match decoded.color().channel_count() {
            1 => "DeviceGray",
            3 => "DeviceRGB",
            _ => "",
        };
        if !space.is_empty() {
            let dict = dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width as i64,
                "Height" => height as i64,
                "ColorSpace" => space,
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            };
            let id = doc.add_object(Stream::new(dict, bytes.to_vec()).with_compression(false));
            return Ok((id, width, height));
        }
    }

    Ok(embed_pixels(doc, &decoded))
}

/// Embeds decoded pixels losslessly (Flate), with a soft mask when any pixel
/// is transparent, and returns the image's id and pixel size.
pub(crate) fn embed_pixels(doc: &mut Document, decoded: &DynamicImage) -> (ObjectId, u32, u32) {
    let (width, height) = (decoded.width(), decoded.height());
    let rgba = decoded.to_rgba8();
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    let mut alpha = Vec::with_capacity((width * height) as usize);
    for pixel in rgba.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel.0[3]);
    }

    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width as i64,
        "Height" => height as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
    };
    if alpha.iter().any(|&a| a < 255) {
        let mask = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        };
        let mut mask = Stream::new(mask, alpha);
        let _ = mask.compress();
        dict.set("SMask", Object::Reference(doc.add_object(mask)));
    }
    let mut stream = Stream::new(dict, rgb);
    let _ = stream.compress();
    (doc.add_object(stream), width, height)
}

/// Ids of images used as (soft) masks of other images; recompressing those
/// lossily would fray edges.
pub(crate) fn mask_ids(doc: &Document) -> std::collections::HashSet<ObjectId> {
    let mut ids = std::collections::HashSet::new();
    for object in doc.objects.values() {
        if let Ok(stream) = object.as_stream() {
            for key in [b"SMask".as_slice(), b"Mask"] {
                if let Ok(id) = stream.dict.get(key).and_then(Object::as_reference) {
                    ids.insert(id);
                }
            }
        }
    }
    ids
}

pub(crate) fn is_image(object: &Object) -> bool {
    object.as_stream().is_ok_and(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Rgb, RgbImage, Rgba, RgbaImage};

    fn encode(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    fn image_dict(width: i64, height: i64, space: Object) -> lopdf::Dictionary {
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => width, "Height" => height,
            "ColorSpace" => space, "BitsPerComponent" => 8,
        }
    }

    #[test]
    fn embeds_jpegs_as_they_are() {
        let mut doc = Document::with_version("1.5");
        for (image, space) in [
            (DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 4, Rgb([10, 200, 30]))), b"DeviceRGB".as_slice()),
            (DynamicImage::ImageLuma8(GrayImage::from_pixel(8, 4, image::Luma([90]))), b"DeviceGray".as_slice()),
        ] {
            let jpeg = encode(&image, ImageFormat::Jpeg);
            let (id, w, h) = embed_image(&mut doc, &jpeg).unwrap();
            assert_eq!((w, h), (8, 4));
            let stream = doc.get_object(id).unwrap().as_stream().unwrap();
            assert_eq!(stream.content, jpeg, "JPEG data must be embedded unchanged");
            assert_eq!(stream.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), space);
            // And it decodes back.
            let decoded = decode_image(&doc, stream).unwrap();
            assert!(decoded.was_jpeg && decoded.image.width() == 8);
        }
    }

    #[test]
    fn embeds_opaque_pngs_without_a_mask() {
        let mut doc = Document::with_version("1.5");
        let png = encode(&DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 3, Rgba([1, 2, 3, 255]))), ImageFormat::Png);
        let (id, _, _) = embed_image(&mut doc, &png).unwrap();
        assert!(!doc.get_object(id).unwrap().as_stream().unwrap().dict.has(b"SMask"));
        assert!(matches!(embed_image(&mut doc, b"not an image"), Err(Error::Image(_))));
    }

    #[test]
    fn decodes_raw_cmyk_and_gray_images() {
        let doc = Document::with_version("1.5");
        let cmyk = Stream::new(image_dict(2, 1, "DeviceCMYK".into()), vec![0, 0, 0, 0, 0, 255, 255, 0]);
        let decoded = decode_image(&doc, &cmyk).unwrap();
        assert!(decoded.converted_from_cmyk);
        assert_eq!(decoded.image.to_rgb8().into_raw(), vec![255, 255, 255, 255, 0, 0]);

        let gray = Stream::new(image_dict(2, 1, "DeviceGray".into()), vec![7, 9]);
        assert_eq!(decode_image(&doc, &gray).unwrap().image.to_luma8().into_raw(), vec![7, 9]);
    }

    #[test]
    fn leaves_unusual_images_alone() {
        let doc = Document::with_version("1.5");
        let rgb = || image_dict(1, 1, "DeviceRGB".into());
        let mut with_decode = rgb();
        with_decode.set("Decode", vec![1.into(), 0.into(), 1.into(), 0.into(), 1.into(), 0.into()]);
        let mut mask = rgb();
        mask.set("ImageMask", true);
        let mut lzw = rgb();
        lzw.set("Filter", "LZWDecode");
        let mut one_bit = rgb();
        one_bit.set("BitsPerComponent", 1);
        let cases = [
            Stream::new(with_decode, vec![0; 3]),
            Stream::new(mask, vec![0; 3]),
            Stream::new(lzw, vec![0; 3]),
            Stream::new(one_bit, vec![0; 3]),
            Stream::new(rgb(), vec![0; 2]), // too short
            Stream::new(image_dict(1, 1, "Pattern".into()), vec![0; 3]),
        ];
        for stream in cases {
            assert!(decode_image(&doc, &stream).is_none(), "{:?}", stream.dict);
        }
    }

    #[test]
    fn understands_icc_and_calibrated_spaces() {
        let mut doc = Document::with_version("1.5");
        let profile = doc.add_object(Stream::new(dictionary! { "N" => 3 }, vec![]));
        let icc = Object::Array(vec![Object::Name(b"ICCBased".to_vec()), Object::Reference(profile)]);
        assert_eq!(color_components(&doc, &icc), Some(3));
        let cal = Object::Array(vec![Object::Name(b"CalGray".to_vec()), dictionary! {}.into()]);
        assert_eq!(color_components(&doc, &cal), Some(1));
        let lab = Object::Array(vec![Object::Name(b"Lab".to_vec())]);
        assert_eq!(color_components(&doc, &lab), None);
        let named = doc.add_object(Object::Name(b"DeviceCMYK".to_vec()));
        assert_eq!(color_components(&doc, &Object::Reference(named)), Some(4));
    }

    #[test]
    fn reads_filter_lists() {
        let mut stream = Stream::new(dictionary! { "Filter" => vec![Object::Name(b"A".to_vec()), Object::Name(b"B".to_vec())] }, vec![]);
        assert_eq!(filters(&stream), vec![b"A".to_vec(), b"B".to_vec()]);
        stream.dict.remove(b"Filter");
        assert!(filters(&stream).is_empty());
        assert_eq!(plain_content(&Stream::new(dictionary! {}, b"raw".to_vec())).unwrap(), b"raw");
    }
}
