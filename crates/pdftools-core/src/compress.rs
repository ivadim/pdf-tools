//! Make files smaller: drop unused objects, compress streams and, at the
//! lossy levels, downscale and re-encode large images as JPEG.

use std::path::Path;

use lopdf::{Document, Object, ObjectId, Stream};

use crate::image_data::{decode_image, is_image, jpeg_bytes, mask_ids};
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Structural clean-up only; page content is untouched.
    Lossless,
    /// Images capped at 2000 px, JPEG quality 75.
    Balanced,
    /// Images capped at 1200 px, JPEG quality 50.
    Strong,
}

impl Level {
    /// Longest image side in pixels and JPEG quality.
    fn image_settings(self) -> Option<(u32, u8)> {
        match self {
            Level::Lossless => None,
            Level::Balanced => Some((2000, 75)),
            Level::Strong => Some((1200, 50)),
        }
    }
}

/// Images smaller than this (longest side, px) aren't worth recompressing.
const MIN_IMAGE_SIDE: u32 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressSummary {
    pub before: u64,
    pub after: u64,
    pub images_recompressed: usize,
}

/// Returns the number of images that were re-encoded. Images are recompressed
/// on several threads (see [`crate::set_max_threads`]).
pub fn compress_document(doc: &mut Document, level: Level) -> usize {
    // Embedded page thumbnails are redundant; viewers render their own.
    for page_id in doc.get_pages().into_values() {
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            page.remove(b"Thumb");
        }
    }

    let mut recompressed = 0;
    if let Some((max_side, quality)) = level.image_settings() {
        let masks = mask_ids(doc);
        let images: Vec<ObjectId> =
            doc.objects.iter().filter(|(id, object)| !masks.contains(id) && is_image(object)).map(|(id, _)| *id).collect();
        let shared = &*doc;
        let streams = crate::parallel::map(&images, |&id| recompress_image(shared, id, max_side, quality), |_| {});
        for (id, stream) in images.into_iter().zip(streams) {
            if let Some(stream) = stream {
                doc.objects.insert(id, Object::Stream(stream));
                recompressed += 1;
            }
        }
    }

    doc.prune_objects();
    crate::compress_streams(doc);
    recompressed
}

fn recompress_image(doc: &Document, id: ObjectId, max_side: u32, quality: u8) -> Option<Stream> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let decoded = decode_image(doc, stream)?;
    let image = decoded.image;
    if image.width().max(image.height()) < MIN_IMAGE_SIDE {
        return None;
    }
    // Averaging blocks of pixels (integer maths) is several times faster than
    // a CatmullRom resample, and looks the same at these sizes.
    let image = if image.width().max(image.height()) > max_side { image.thumbnail(max_side, max_side) } else { image };
    let bytes = jpeg_bytes(&image, quality)?;
    // Only keep the result if it is meaningfully smaller.
    if bytes.len() as f64 >= stream.content.len() as f64 * 0.9 {
        return None;
    }

    let mut dict = stream.dict.clone();
    dict.set("Width", image.width() as i64);
    dict.set("Height", image.height() as i64);
    dict.set("BitsPerComponent", 8);
    dict.set("Filter", "DCTDecode");
    dict.remove(b"DecodeParms");
    if decoded.converted_from_cmyk {
        dict.set("ColorSpace", "DeviceRGB");
    }
    Some(Stream::new(dict, bytes).with_compression(false))
}

pub fn compress_file(input: &Path, level: Level, output: &Path) -> Result<CompressSummary, Error> {
    let before = crate::file_size(input);
    let mut doc = crate::load(input)?;
    let images_recompressed = compress_document(&mut doc, level);
    crate::save(&mut doc, output)?;
    Ok(CompressSummary { before, after: crate::file_size(output), images_recompressed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{page_labels, round_trip, sample};
    use lopdf::dictionary;

    /// Adds a large, noisy (poorly compressible) RGB image to page 1.
    fn with_photo(mut doc: Document, size: u32) -> (Document, ObjectId) {
        let mut seed = 12345u32;
        let pixels: Vec<u8> = (0..size * size * 3)
            .map(|i| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                ((i / 3 % size) as u8).wrapping_add((seed >> 24) as u8 / 8)
            })
            .collect();
        let mut image = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => size as i64,
                "Height" => size as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            pixels,
        );
        image.compress().unwrap();
        let id = doc.add_object(image);
        (doc, id)
    }

    #[test]
    fn lossy_levels_shrink_large_images() {
        let (mut doc, id) = with_photo(sample("a", 1), 2400);
        let original = doc.get_object(id).unwrap().as_stream().unwrap().content.len();
        // Keep the image reachable so pruning doesn't simply delete it.
        doc.trailer.set("KeepImage", Object::Reference(id));

        assert_eq!(compress_document(&mut doc, Level::Strong), 1);
        let doc = round_trip(doc);
        let image = doc.get_object(id).unwrap().as_stream().unwrap();
        assert_eq!(image.dict.get(b"Filter").unwrap().as_name().unwrap(), b"DCTDecode");
        assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 1200);
        assert!(image.content.len() < original / 4, "{} vs {original}", image.content.len());
        assert_eq!(page_labels(&doc), ["a-1"]);
    }

    #[test]
    fn threads_give_the_same_result() {
        let photos = |count: usize| {
            let mut doc = sample("a", 1);
            let mut ids = Vec::new();
            for size in (0..count).map(|i| 1300 + 50 * i as u32) {
                let (with, id) = with_photo(doc, size);
                doc = with;
                doc.trailer.set(format!("Keep{}", id.0), Object::Reference(id));
                ids.push(id);
            }
            (doc, ids)
        };
        let (mut one, ids) = photos(5);
        let (mut four, _) = photos(5);
        crate::set_max_threads(1);
        assert_eq!(compress_document(&mut one, Level::Strong), 5);
        crate::set_max_threads(4);
        assert_eq!(compress_document(&mut four, Level::Strong), 5);
        for id in ids {
            let (a, b) = (one.get_object(id).unwrap().as_stream().unwrap(), four.get_object(id).unwrap().as_stream().unwrap());
            assert_eq!(a.content, b.content);
            // Downscaled to fit 1200 px, keeping the shape.
            assert_eq!(a.dict.get(b"Width").unwrap().as_i64().unwrap(), 1200);
        }
    }

    #[test]
    fn lossless_leaves_images_alone_and_drops_unused_objects() {
        let (mut doc, id) = with_photo(sample("a", 1), 300);
        assert_eq!(compress_document(&mut doc, Level::Lossless), 0);
        assert!(doc.get_object(id).is_err(), "unreferenced image should be pruned");
        assert_eq!(page_labels(&round_trip(doc)), ["a-1"]);
    }
}
