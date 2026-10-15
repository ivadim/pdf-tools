//! Builds a PDF from JPEG and PNG images, one image per page.
//!
//! JPEGs are embedded as they are (no re-encoding), so photos keep their
//! quality and the PDF stays close to the size of the images. The rotation a
//! camera records in EXIF is honoured.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use crate::image_data::{embed_image, embed_pixels};
use crate::Error;

/// Points per pixel when a page takes the image's own size: 96 pixels per
/// inch, the usual screen resolution.
pub const POINTS_PER_PIXEL: f32 = 0.75;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    #[default]
    A4,
    Letter,
    /// Each page is the size of its image (at 96 pixels per inch) plus the
    /// margins.
    FitImage,
}

impl PageSize {
    /// Portrait size in points, or `None` for [`PageSize::FitImage`].
    pub fn portrait(self) -> Option<(f32, f32)> {
        match self {
            PageSize::A4 => Some((595.28, 841.89)),
            PageSize::Letter => Some((612.0, 792.0)),
            PageSize::FitImage => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageOrientation {
    /// Landscape pages for wide images, portrait for the rest.
    #[default]
    Auto,
    Portrait,
    Landscape,
}

/// How images are placed on pages.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageLayout {
    pub page_size: PageSize,
    pub orientation: PageOrientation,
    /// Space around the image, in points.
    pub margin: f32,
}

/// Where an image goes: the page size and the image's box on it, in points
/// from the bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub page: (f32, f32),
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ImageLayout {
    /// Places an image of `size` pixels (as displayed, after rotation): as
    /// large as fits inside the margins, centred, keeping its proportions.
    pub fn place(&self, size: (f32, f32)) -> Placement {
        let (iw, ih) = (size.0.max(1.0), size.1.max(1.0));
        let margin = self.margin.max(0.0);
        let page = match self.page_size.portrait() {
            None => (iw * POINTS_PER_PIXEL + 2.0 * margin, ih * POINTS_PER_PIXEL + 2.0 * margin),
            Some((w, h)) => {
                let landscape = match self.orientation {
                    PageOrientation::Auto => iw > ih,
                    PageOrientation::Portrait => false,
                    PageOrientation::Landscape => true,
                };
                if landscape {
                    (h, w)
                } else {
                    (w, h)
                }
            }
        };
        let available = ((page.0 - 2.0 * margin).max(1.0), (page.1 - 2.0 * margin).max(1.0));
        let scale = (available.0 / iw).min(available.1 / ih);
        let (width, height) = (iw * scale, ih * scale);
        Placement { page, x: (page.0 - width) / 2.0, y: (page.1 - height) / 2.0, width, height }
    }
}

/// An image to put on a page, turned clockwise by `rotation` degrees (a
/// multiple of 90) on top of any rotation recorded in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePage {
    pub path: PathBuf,
    pub rotation: i32,
}

impl ImagePage {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), rotation: 0 }
    }

    pub fn rotated(self, rotation: i32) -> Self {
        Self { rotation, ..self }
    }
}

/// What the tools need to know about an image before converting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    /// Size in pixels as the image is meant to be seen (EXIF rotation applied).
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
}

/// Reads an image's size and orientation without decoding its pixels.
pub fn image_info(path: &Path) -> Result<ImageInfo, Error> {
    let read_error = |message: String| Error::ReadImage { path: path.to_path_buf(), message };
    let bytes = std::fs::read(path).map_err(|e| read_error(e.to_string()))?;
    let mut decoder = decoder(&bytes).map_err(read_error)?;
    let (width, height) = decoder.dimensions();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (width, height) = if quarter_turns(orientation) % 2 == 1 { (height, width) } else { (width, height) };
    Ok(ImageInfo { width, height, file_size: bytes.len() as u64 })
}

/// Decodes an image for display: EXIF rotation applied, then turned
/// clockwise by `rotation` degrees.
pub fn load_image(path: &Path, rotation: i32) -> Result<DynamicImage, Error> {
    let read_error = |message: String| Error::ReadImage { path: path.to_path_buf(), message };
    let bytes = std::fs::read(path).map_err(|e| read_error(e.to_string()))?;
    let mut decoder = decoder(&bytes).map_err(read_error)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| read_error(e.to_string()))?;
    image.apply_orientation(orientation);
    Ok(rotate(image, rotation))
}

/// Builds a document with one page per image, in order. `progress` is called
/// with the index of each image before it is read.
pub fn images_to_pdf(
    images: &[ImagePage],
    layout: &ImageLayout,
    mut progress: impl FnMut(usize),
) -> Result<Document, Error> {
    if images.is_empty() {
        return Err(Error::NoInput);
    }
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(images.len());
    for (index, page) in images.iter().enumerate() {
        progress(index);
        kids.push(Object::Reference(add_image_page(&mut doc, pages_id, page, layout)?));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => kids.len() as i64,
            "Kids" => kids,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    Ok(doc)
}

/// [`images_to_pdf`], saved to `output`. Returns the number of pages.
pub fn images_to_pdf_file(
    images: &[ImagePage],
    layout: &ImageLayout,
    output: &Path,
    progress: impl FnMut(usize),
) -> Result<usize, Error> {
    let mut doc = images_to_pdf(images, layout, progress)?;
    crate::save(&mut doc, output)?;
    Ok(images.len())
}

fn add_image_page(doc: &mut Document, parent: ObjectId, page: &ImagePage, layout: &ImageLayout) -> Result<ObjectId, Error> {
    if page.rotation % 90 != 0 {
        return Err(Error::InvalidRotation(page.rotation.into()));
    }
    let read_error = |message: String| Error::ReadImage { path: page.path.clone(), message };
    let bytes = std::fs::read(&page.path).map_err(|e| read_error(e.to_string()))?;
    let orientation = decoder(&bytes).map_err(read_error)?.orientation().unwrap_or(Orientation::NoTransforms);

    // Rotations are applied while drawing, so JPEGs are embedded untouched.
    // Mirrored orientations are rare: those images are decoded and flipped.
    let (image_id, width, height, turns) = if is_mirrored(orientation) {
        let mut decoded = DynamicImage::from_decoder(decoder(&bytes).map_err(read_error)?)
            .map_err(|e| read_error(e.to_string()))?;
        decoded.apply_orientation(orientation);
        let (id, w, h) = embed_pixels(doc, &decoded);
        (id, w, h, 0)
    } else {
        let (id, w, h) = embed_image(doc, &bytes).map_err(|e| read_error(e.to_string()))?;
        (id, w, h, quarter_turns(orientation))
    };
    let turns = (turns + page.rotation.div_euclid(90)).rem_euclid(4);
    let shown = if turns % 2 == 1 { (height, width) } else { (width, height) };
    let place = layout.place((shown.0 as f32, shown.1 as f32));

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", draw_matrix(turns, &place).into_iter().map(Object::Real).collect()),
            Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
    Ok(doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => parent,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(place.page.0), Object::Real(place.page.1)],
        "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image_id } },
        "Contents" => content_id,
    }))
}

/// Maps the image's unit square onto the placement box, turned clockwise
/// by `turns` quarter turns.
fn draw_matrix(turns: i32, place: &Placement) -> [f32; 6] {
    let Placement { x, y, width: w, height: h, .. } = *place;
    match turns {
        1 => [0.0, -h, w, 0.0, x, y + h],
        2 => [-w, 0.0, 0.0, -h, x + w, y + h],
        3 => [0.0, h, -w, 0.0, x + w, y],
        _ => [w, 0.0, 0.0, h, x, y],
    }
}

fn decoder(bytes: &[u8]) -> Result<impl ImageDecoder + '_, String> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    match reader.format() {
        Some(ImageFormat::Jpeg | ImageFormat::Png) => reader.into_decoder().map_err(|e| e.to_string()),
        Some(other) => Err(format!("{} images aren't supported; use JPEG or PNG", other.extensions_str()[0].to_uppercase())),
        None => Err("not a JPEG or PNG image".into()),
    }
}

/// Clockwise quarter turns of an orientation (ignoring any mirroring).
fn quarter_turns(orientation: Orientation) -> i32 {
    match orientation {
        Orientation::Rotate90 | Orientation::Rotate90FlipH => 1,
        Orientation::Rotate180 => 2,
        Orientation::Rotate270 | Orientation::Rotate270FlipH => 3,
        _ => 0,
    }
}

fn is_mirrored(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::FlipHorizontal | Orientation::FlipVertical | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    )
}

fn rotate(image: DynamicImage, degrees: i32) -> DynamicImage {
    match degrees.rem_euclid(360) {
        90 => image.rotate90(),
        180 => image.rotate180(),
        270 => image.rotate270(),
        _ => image,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    /// A 40×20 image, red on the left half and blue on the right.
    fn halves() -> RgbImage {
        RgbImage::from_fn(40, 20, |x, _| if x < 20 { Rgb([255, 0, 0]) } else { Rgb([0, 0, 255]) })
    }

    fn save(image: &DynamicImage, path: &Path, format: ImageFormat) {
        image.save_with_format(path, format).unwrap();
    }

    /// A JPEG with an EXIF orientation tag (APP1 segment after SOI).
    fn jpeg_with_orientation(image: &RgbImage, exif_orientation: u16) -> Vec<u8> {
        let mut plain = Vec::new();
        DynamicImage::ImageRgb8(image.clone()).write_to(&mut Cursor::new(&mut plain), ImageFormat::Jpeg).unwrap();
        // Big-endian TIFF header with one IFD entry: Orientation (0x0112), SHORT.
        let mut tiff = b"MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01".to_vec();
        tiff.extend_from_slice(&exif_orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);
        let mut out = plain[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&plain[2..]);
        out
    }

    fn page_matrix(doc: &Document, page: u32) -> Vec<f32> {
        let id = doc.get_pages()[&page];
        let content = Content::decode(&doc.get_page_content(id)).unwrap();
        let cm = content.operations.iter().find(|op| op.operator == "cm").unwrap();
        cm.operands.iter().map(|o| o.as_float().unwrap()).collect()
    }

    fn media_box(doc: &Document, page: u32) -> Vec<f32> {
        let id = doc.get_pages()[&page];
        let dict = doc.get_dictionary(id).unwrap();
        dict.get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect()
    }

    #[test]
    fn places_images_inside_the_margins() {
        let a4 = ImageLayout { margin: 20.0, ..Default::default() };
        // A wide image gets a landscape page, filled edge to edge in width.
        let wide = a4.place((400.0, 200.0));
        assert_eq!(wide.page, (841.89, 595.28));
        assert!((wide.width - (841.89 - 40.0)).abs() < 0.01);
        assert!((wide.width / wide.height - 2.0).abs() < 1e-4);
        assert!((wide.y - (595.28 - wide.height) / 2.0).abs() < 0.01, "centred");
        // Forced portrait: the same image is limited by the page width.
        let portrait = ImageLayout { orientation: PageOrientation::Portrait, ..a4 }.place((400.0, 200.0));
        assert_eq!(portrait.page, (595.28, 841.89));
        assert!((portrait.x - 20.0).abs() < 0.01);
        // Small images are scaled up too.
        let tall = ImageLayout { page_size: PageSize::Letter, ..Default::default() }.place((10.0, 20.0));
        assert_eq!((tall.page, tall.height), ((612.0, 792.0), 792.0));
        // Fit to image: 96 px per inch plus margins.
        let fit = ImageLayout { page_size: PageSize::FitImage, margin: 10.0, ..Default::default() }.place((400.0, 200.0));
        assert_eq!(fit.page, (320.0, 170.0));
        assert_eq!((fit.x, fit.y, fit.width, fit.height), (10.0, 10.0, 300.0, 150.0));
    }

    #[test]
    fn makes_one_page_per_image() {
        let dir = temp_dir("images-pages");
        let jpg = dir.join("photo.jpg");
        let png = dir.join("logo.png");
        save(&DynamicImage::ImageRgb8(halves()), &jpg, ImageFormat::Jpeg);
        let transparent = RgbaImage::from_fn(30, 30, |x, _| Rgba([0, 128, 0, if x < 15 { 255 } else { 0 }]));
        save(&DynamicImage::ImageRgba8(transparent), &png, ImageFormat::Png);

        let images = [ImagePage::new(&jpg), ImagePage::new(&png).rotated(90)];
        let mut seen = Vec::new();
        let output = dir.join("out.pdf");
        let pages = images_to_pdf_file(&images, &ImageLayout::default(), &output, |i| seen.push(i)).unwrap();
        assert_eq!((pages, seen), (2, vec![0, 1]));

        let doc = Document::load(&output).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
        assert_eq!(media_box(&doc, 1), [0.0, 0.0, 841.89, 595.28], "wide photo, landscape page");
        assert_eq!(media_box(&doc, 2), [0.0, 0.0, 595.28, 841.89]);
        // The JPEG is embedded as it is; the PNG keeps its transparency.
        let images: Vec<&lopdf::Stream> = doc
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
            .collect();
        let jpeg = images.iter().find(|s| s.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"DCTDecode")).unwrap();
        assert_eq!(jpeg.content, std::fs::read(&jpg).unwrap());
        assert!(images.iter().any(|s| s.dict.has(b"SMask")));
        // The rotated PNG is drawn turned clockwise.
        let m = page_matrix(&doc, 2);
        assert_eq!((m[0], m[3]), (0.0, 0.0));
        assert!(m[1] < 0.0 && m[2] > 0.0);
    }

    #[test]
    fn honours_exif_orientation() {
        let dir = temp_dir("images-exif");
        let rotated = dir.join("rotated.jpg");
        std::fs::write(&rotated, jpeg_with_orientation(&halves(), 6)).unwrap();
        // EXIF 6: shown turned 90° clockwise, so 20×40.
        let info = image_info(&rotated).unwrap();
        assert_eq!((info.width, info.height), (20, 40));
        let shown = load_image(&rotated, 0).unwrap().to_rgb8();
        assert_eq!((shown.width(), shown.height()), (20, 40));
        assert!(shown.get_pixel(10, 2)[0] > 200, "red half now on top");
        // An extra quarter turn by the user.
        let turned = load_image(&rotated, 90).unwrap();
        assert_eq!((turned.width(), turned.height()), (40, 20));

        let doc = images_to_pdf(&[ImagePage::new(&rotated)], &ImageLayout::default(), |_| {}).unwrap();
        assert_eq!(media_box(&doc, 1), [0.0, 0.0, 595.28, 841.89], "upright photo, portrait page");
        let m = page_matrix(&doc, 1);
        assert!(m[1] < 0.0 && m[2] > 0.0, "drawn a quarter turn clockwise: {m:?}");

        // A mirrored orientation is decoded and flipped instead.
        let mirrored = dir.join("mirrored.jpg");
        std::fs::write(&mirrored, jpeg_with_orientation(&halves(), 2)).unwrap();
        let flipped = load_image(&mirrored, 0).unwrap().to_rgb8();
        assert!(flipped.get_pixel(2, 10)[2] > 200, "blue half now on the left");
        let doc = images_to_pdf(&[ImagePage::new(&mirrored).rotated(180)], &ImageLayout::default(), |_| {}).unwrap();
        let m = page_matrix(&doc, 1);
        assert!(m[0] < 0.0 && m[3] < 0.0, "half turn: {m:?}");
    }

    #[test]
    fn reports_unreadable_images() {
        let dir = temp_dir("images-bad");
        let text = dir.join("notes.png");
        std::fs::write(&text, b"not an image").unwrap();
        let error = image_info(&text).unwrap_err().to_string();
        assert!(error.contains("notes.png"), "{error}");
        let gif = dir.join("anim.gif");
        std::fs::write(&gif, b"GIF89a\x01\0\x01\0\0\0\0;").unwrap();
        assert!(image_info(&gif).unwrap_err().to_string().contains("GIF images aren't supported"));
        assert!(image_info(&dir.join("missing.jpg")).is_err());
        assert!(matches!(images_to_pdf(&[], &ImageLayout::default(), |_| {}), Err(Error::NoInput)));
        let bad_turn = ImagePage::new(dir.join("x.png")).rotated(45);
        assert!(matches!(images_to_pdf(&[bad_turn], &ImageLayout::default(), |_| {}), Err(Error::InvalidRotation(45))));
    }
}
