use super::dxt::decode_dxt5;
use crate::avm2::Value;
use crate::avm2::activation::Activation;
use crate::avm2::bytearray::ByteArrayStorage;
use crate::avm2::error::{Error, make_error_1034, make_error_3675, make_error_3679};
use crate::avm2::object::{Object, TextureObject};
use ruffle_render::atf::ATFTexture;
use ruffle_render::atf::ATFTextureData;
use std::io::Cursor;

use jpegxr::PixelFormat;
use std::io::Read;
use std::io::Seek;

pub fn do_compressed_upload<'gc>(
    activation: &mut Activation<'_, 'gc>,
    texture: TextureObject<'gc>,
    data: Object<'gc>,
    byte_array_offset: usize,
    is_cube: bool,
) -> Result<(), Error<'gc>> {
    let Some(bytes) = data.as_bytearray() else {
        let bytearray_class = activation.avm2().classes().bytearray;
        return Err(make_error_1034(
            activation,
            Value::Object(data),
            bytearray_class.inner_class_definition(),
        ));
    };
    let Some(raw_atf) = ByteArrayStorage::bytes(&bytes).get(byte_array_offset..) else {
        return Err(make_error_3679(activation));
    };

    // Some 2016-era MU assets use an ATF variant this reader does not
    // understand (an unexpected EOF). Flash only skips the bad upload, so do
    // the same instead of aborting the caller's enterFrame handler.
    let atf_texture = match ATFTexture::from_bytes(raw_atf) {
        Ok(texture) => texture,
        Err(e) => {
            tracing::warn!(
                "Skipping ATF texture ({} bytes at offset {}): {e}",
                raw_atf.len(),
                byte_array_offset
            );
            return Ok(());
        }
    };

    if atf_texture.width != texture.handle().width()
        || atf_texture.height != texture.handle().height()
    {
        return Err(make_error_3679(activation));
    }

    if is_cube != atf_texture.cubemap {
        return Err(make_error_3675(activation));
    }

    for (face, mip_levels) in atf_texture.face_mip_data.iter().enumerate() {
        for (mip_level, data) in mip_levels.iter().enumerate() {
            let width = (atf_texture.width >> mip_level).max(1);
            let height = (atf_texture.height >> mip_level).max(1);
            let format = &atf_texture.format;
            let pixels = match data {
                ATFTextureData::JpegXR(bytes) => {
                    let image = match jpegxr_to_tiff(width, height, &mut Cursor::new(bytes)) {
                        Ok((image, _)) => image,
                        Err(e) => {
                            tracing::warn!("Skipping JPEG-XR mip level: {e}");
                            continue;
                        }
                    };
                    image.to_rgba8().pixels().flat_map(|p| p.0).collect()
                }
                ATFTextureData::CompressedAlpha {
                    dxt1_alpha_compressed,
                    jpegxr_alpha: orig_jpegxr_alpha,
                    dxt5_rgb_compressed,
                    jpegxr_bgr: orig_jpegxr_bgr,
                } => {
                    let result = (|| -> Result<Vec<u8>, String> {
                        if width < 4 || height < 4 {
                            return Err(format!(
                                "COMPRESSED_ALPHA mip {width}x{height} is smaller than one DXT block"
                            ));
                        }
                        // See https://github.com/adobe/dds2atf/issues/5
                        // The ATF format uses a weird version of LZMA that doesn't store the uncompressed length.
                        // Compute it ourselves, and insert it where it should be
                        let mut dxt1_alpha_compressed = dxt1_alpha_compressed.clone();
                        let mut dxt5_rgb_compressed = dxt5_rgb_compressed.clone();

                        let dxt1_uncompressed_length = u64::MAX;
                        let dxt5_uncompressed_length = u64::MAX;

                        if dxt1_alpha_compressed.len() < 5 || dxt5_rgb_compressed.len() < 5 {
                            return Err("Truncated COMPRESSED_ALPHA LZMA header".to_string());
                        }
                        dxt1_alpha_compressed.splice(5..5, dxt1_uncompressed_length.to_le_bytes());
                        dxt5_rgb_compressed.splice(5..5, dxt5_uncompressed_length.to_le_bytes());

                        let mut dxt1_alpha = Vec::with_capacity(dxt1_alpha_compressed.len());
                        lzma_rs::lzma_decompress(
                            &mut dxt1_alpha_compressed.as_slice(),
                            &mut dxt1_alpha,
                        )
                        .map_err(|e| format!("Failed to decompress DXT1 alpha: {e}"))?;

                        let mut dxt5_rgb = Vec::with_capacity(dxt5_rgb_compressed.len());
                        lzma_rs::lzma_decompress(
                            &mut dxt5_rgb_compressed.as_slice(),
                            &mut dxt5_rgb,
                        )
                        .map_err(|e| format!("Failed to decompress DXT5 RGB: {e}"))?;

                        // 'COMPRESSED_ALPHA' images are encoded in a very strange way. The LZMA-compressed DXT1/DXT5
                        // sections just hold the DXT lookup table block. The associated values for each block
                        // are stored in JPEG-XR images, which are optionally compressed
                        let (jpegxr_alpha, pixel_format) = jpegxr_to_tiff(
                            width / 4,
                            height / 2,
                            &mut Cursor::new(orig_jpegxr_alpha),
                        )?;

                        if pixel_format != PixelFormat::PixelFormat8bppGray {
                            return Err(format!(
                                "Unexpected JPEG-XR alpha format {pixel_format:?}"
                            ));
                        }
                        let jpegxr_alpha = jpegxr_alpha.to_rgba8();

                        let jpegxr_bgr = jpegxr_to_raw_pixels(
                            width / 4,
                            height / 2,
                            &mut Cursor::new(orig_jpegxr_bgr),
                        )?;

                        if dxt1_alpha.len() % 6 != 0 || dxt5_rgb.len() % 4 != 0 {
                            return Err("Bad DXT data length".to_string());
                        }
                        let block_count = dxt1_alpha.len() / 6;
                        if block_count != dxt5_rgb.len() / 4
                            || jpegxr_alpha.as_raw().len() < block_count * 8
                            || jpegxr_bgr.len() < block_count * 4
                        {
                            return Err("COMPRESSED_ALPHA block data does not match".to_string());
                        }

                        let second_half = ((width / 4) * (height / 4)) as usize;
                        if second_half != block_count {
                            return Err(format!(
                                "COMPRESSED_ALPHA expected {second_half} blocks, got {block_count}"
                            ));
                        }

                        let mut reconstructed_dxt = Vec::with_capacity(block_count * 16);
                        // The two values for each pixels are split across the upper and lower halves of the image.
                        // See https://github.com/adobe/dds2atf/blob/cbc479be2e77daa273306161af571f8255aec78d/pvr2atfcore.cpp#L884
                        for i in 0..block_count {
                            let alpha_lookup_table = &dxt1_alpha[i * 6..(i + 1) * 6];
                            let jpegxr_alpha_first = &jpegxr_alpha.as_raw()[i * 4..(i + 1) * 4];
                            let jpegxr_alpha_second = &jpegxr_alpha.as_raw()
                                [(second_half + (i * 4))..(second_half + ((i + 1) * 4))];

                            let rgb_lookup_table = &dxt5_rgb[i * 4..(i + 1) * 4];
                            let jpegxr_bgr_first = &jpegxr_bgr[(i * 2)..((i + 1) * 2)];
                            // Each pixel is 2 bytes, so multiply second_half by 2 to get the correct
                            // start of the second half of the image
                            let jpegxr_bgr_second = &jpegxr_bgr
                                [(second_half * 2 + (i * 2))..(second_half * 2 + ((i + 1) * 2))];

                            // We decoded as grayscale, so just use the red values (all of the rgb values
                            // should be the same)
                            reconstructed_dxt.push(jpegxr_alpha_first[0]);
                            reconstructed_dxt.push(jpegxr_alpha_second[0]);
                            // Copy the corresponding lookup table
                            reconstructed_dxt.extend(alpha_lookup_table);

                            // Copy the two 16-bit values
                            reconstructed_dxt.extend(jpegxr_bgr_first);
                            reconstructed_dxt.extend(jpegxr_bgr_second);

                            // Copy the lookup table
                            reconstructed_dxt.extend(rgb_lookup_table);
                        }

                        Ok(decode_dxt5(&reconstructed_dxt, width, height))
                    })();

                    match result {
                        Ok(pixels) => pixels,
                        Err(e) => {
                            tracing::warn!("Skipping COMPRESSED_ALPHA mip level: {e}");
                            continue;
                        }
                    }
                }
                ATFTextureData::CompressedRawAlpha {
                    dxt5,
                    pvrtc: _,
                    etc1: _,
                    etc2: _,
                } => {
                    if dxt5.is_empty() {
                        tracing::warn!(
                            "ATF CompressedRawAlpha has no DXT5 data; skipping this mip level"
                        );
                        Vec::new()
                    } else {
                        decode_dxt5(dxt5, width, height)
                    }
                }
                ATFTextureData::Unknown(_) => {
                    return Err(format!("Unsupported ATF format: {:?}", format).into());
                }
            };

            if pixels.is_empty() {
                continue;
            }
            texture.context3d().copy_pixels_to_texture(
                pixels,
                texture.handle(),
                face as u32,
                mip_level as u32,
            );
        }
    }

    Ok(())
}

fn jpegxr_to_raw_pixels<R: Read + Seek>(
    atf_width: u32,
    atf_height: u32,
    bytes: R,
) -> Result<Vec<u8>, String> {
    let mut decoder = jpegxr::ImageDecode::with_reader(bytes)
        .map_err(|e| format!("Failed to decode JPEG-XR image: {e}"))?;

    let pixel_format = decoder
        .get_pixel_format()
        .map_err(|e| format!("Failed to get pixel format: {e}"))?;
    let (jpeg_width, jpeg_height) = decoder
        .get_size()
        .map_err(|e| format!("Failed to get JPEG-XR size: {e}"))?;
    let jpeg_width = jpeg_width as u32;
    let jpeg_height = jpeg_height as u32;

    if jpeg_width != atf_width || jpeg_height != atf_height {
        return Err(format!(
            "Mismatched JPEG-XR size {jpeg_width}x{jpeg_height}, expected {atf_width}x{atf_height}"
        ));
    }

    let info = jpegxr::PixelInfo::from_format(pixel_format);
    let stride = jpeg_width as usize * info.bits_per_pixel() / 8;
    let size = stride * jpeg_height as usize;

    let mut output = vec![0; size];

    decoder
        .copy_all(&mut output, stride)
        .map_err(|e| format!("Failed to decode JPEG-XR pixels: {e}"))?;
    Ok(output)
}

fn jpegxr_to_tiff<R: Read + Seek>(
    atf_width: u32,
    atf_height: u32,
    bytes: R,
) -> Result<(image::DynamicImage, jpegxr::PixelFormat), String> {
    let mut decoder = jpegxr::ImageDecode::with_reader(bytes)
        .map_err(|e| format!("Failed to decode JPEG-XR image: {e}"))?;

    let pixel_format = decoder
        .get_pixel_format()
        .map_err(|e| format!("Failed to get pixel format: {e}"))?;

    let (jpeg_width, jpeg_height) = decoder
        .get_size()
        .map_err(|e| format!("Failed to get JPEG-XR size: {e}"))?;
    let jpeg_width = jpeg_width as u32;
    let jpeg_height = jpeg_height as u32;

    if jpeg_width != atf_width || jpeg_height != atf_height {
        return Err(format!(
            "Mismatched JPEG-XR size {jpeg_width}x{jpeg_height}, expected {atf_width}x{atf_height}"
        ));
    }

    let info = jpegxr::PixelInfo::from_format(pixel_format);
    let stride = jpeg_width as usize * info.bits_per_pixel() / 8;
    let size = stride * jpeg_height as usize;

    // We convert the result to a TIFF - this makes the jpegxr library handle
    // all of the weird JPEG-XR alpha formats for us. We can then use the normal
    // `image` crate to decode the TIFF to an rgba array.
    let mut bmp_buffer = vec![0; size];
    decoder
        .convert_to_tiff(&mut Cursor::new(&mut bmp_buffer))
        .map_err(|e| format!("Failed to convert JPEG-XR to TIFF: {e}"))?;

    let image_reader =
        image::ImageReader::with_format(Cursor::new(bmp_buffer), image::ImageFormat::Tiff);
    let image = image_reader
        .decode()
        .map_err(|e| format!("Failed to decode JPEG-XR TIFF: {e}"))?;
    Ok((image, pixel_format))
}
