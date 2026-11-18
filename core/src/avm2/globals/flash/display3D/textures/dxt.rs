//! Software decoders for the DXT1 (BC1) and DXT5 (BC3) block formats.
//!
//! Stage3D accepts compressed textures on every renderer. Decoding them to
//! RGBA8 here keeps the upload path identical on native and WebGL, and lets us
//! upload arbitrary mip levels (including levels smaller than one DXT block).

pub fn decode_dxt1(input: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut output = vec![0; width as usize * height as usize * 4];
    let blocks = width.div_ceil(4) as usize * height.div_ceil(4) as usize;
    if input.len() < blocks.saturating_mul(8) {
        tracing::warn!(
            "Truncated DXT1 data: {} bytes for {width}x{height}",
            input.len()
        );
        return output;
    }
    let mut offset = 0;
    for block_y in (0..height).step_by(4) {
        for block_x in (0..width).step_by(4) {
            let block = &input[offset..offset + 8];
            offset += 8;
            let color0 = u16::from_le_bytes([block[0], block[1]]);
            let color1 = u16::from_le_bytes([block[2], block[3]]);
            let colors = decode_dxt_colors(color0, color1, false);
            let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
            write_color_block(
                &mut output,
                width,
                height,
                block_x,
                block_y,
                &colors,
                indices,
            );
        }
    }
    output
}

pub fn decode_dxt5(input: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut output = vec![0; width as usize * height as usize * 4];
    let blocks = width.div_ceil(4) as usize * height.div_ceil(4) as usize;
    if input.len() < blocks.saturating_mul(16) {
        tracing::warn!(
            "Truncated DXT5 data: {} bytes for {width}x{height}",
            input.len()
        );
        return output;
    }
    let mut offset = 0;
    for block_y in (0..height).step_by(4) {
        for block_x in (0..width).step_by(4) {
            let block = &input[offset..offset + 16];
            offset += 16;
            let alpha0 = block[0];
            let alpha1 = block[1];
            let alpha_indices = (block[2] as u64)
                | ((block[3] as u64) << 8)
                | ((block[4] as u64) << 16)
                | ((block[5] as u64) << 24)
                | ((block[6] as u64) << 32)
                | ((block[7] as u64) << 40);
            let mut alphas = [0; 8];
            alphas[0] = alpha0;
            alphas[1] = alpha1;
            if alpha0 > alpha1 {
                for i in 1..7 {
                    alphas[i + 1] =
                        (((7 - i) as u16 * alpha0 as u16 + i as u16 * alpha1 as u16) / 7) as u8;
                }
            } else {
                for i in 1..5 {
                    alphas[i + 1] =
                        (((5 - i) as u16 * alpha0 as u16 + i as u16 * alpha1 as u16) / 5) as u8;
                }
                alphas[6] = 0;
                alphas[7] = 255;
            }

            let color0 = u16::from_le_bytes([block[8], block[9]]);
            let color1 = u16::from_le_bytes([block[10], block[11]]);
            let colors = decode_dxt_colors(color0, color1, true);
            let indices = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
            for y in 0..4 {
                for x in 0..4 {
                    let px = block_x + x;
                    let py = block_y + y;
                    if px >= width || py >= height {
                        continue;
                    }
                    let index = (y * 4 + x) as u64;
                    let color_index = ((indices >> (2 * index)) & 0x3) as usize;
                    let alpha_index = ((alpha_indices >> (3 * index)) & 0x7) as usize;
                    let mut color = colors[color_index];
                    color[3] = alphas[alpha_index];
                    let dest = ((py * width + px) * 4) as usize;
                    output[dest..dest + 4].copy_from_slice(&color);
                }
            }
        }
    }
    output
}

fn write_color_block(
    output: &mut [u8],
    width: u32,
    height: u32,
    block_x: u32,
    block_y: u32,
    colors: &[[u8; 4]; 4],
    indices: u32,
) {
    for y in 0..4 {
        for x in 0..4 {
            let px = block_x + x;
            let py = block_y + y;
            if px >= width || py >= height {
                continue;
            }
            let index = ((indices >> (2 * (y * 4 + x))) & 0x3) as usize;
            let dest = ((py * width + px) * 4) as usize;
            output[dest..dest + 4].copy_from_slice(&colors[index]);
        }
    }
}

fn decode_dxt_colors(color0: u16, color1: u16, four_color_mode: bool) -> [[u8; 4]; 4] {
    let c0 = rgb565(color0);
    let c1 = rgb565(color1);
    let mut colors = [[0; 4]; 4];
    colors[0] = [c0[0], c0[1], c0[2], 255];
    colors[1] = [c1[0], c1[1], c1[2], 255];
    if four_color_mode || color0 > color1 {
        colors[2] = [
            ((2 * c0[0] as u16 + c1[0] as u16) / 3) as u8,
            ((2 * c0[1] as u16 + c1[1] as u16) / 3) as u8,
            ((2 * c0[2] as u16 + c1[2] as u16) / 3) as u8,
            255,
        ];
        colors[3] = [
            ((c0[0] as u16 + 2 * c1[0] as u16) / 3) as u8,
            ((c0[1] as u16 + 2 * c1[1] as u16) / 3) as u8,
            ((c0[2] as u16 + 2 * c1[2] as u16) / 3) as u8,
            255,
        ];
    } else {
        colors[2] = [
            ((c0[0] as u16 + c1[0] as u16) / 2) as u8,
            ((c0[1] as u16 + c1[1] as u16) / 2) as u8,
            ((c0[2] as u16 + c1[2] as u16) / 2) as u8,
            255,
        ];
        colors[3] = [0, 0, 0, 0];
    }
    colors
}

fn rgb565(value: u16) -> [u8; 3] {
    [
        expand_5bits(((value >> 11) & 0x1F) as u8),
        expand_6bits(((value >> 5) & 0x3F) as u8),
        expand_5bits((value & 0x1F) as u8),
    ]
}

fn expand_5bits(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn expand_6bits(value: u8) -> u8 {
    (value << 2) | (value >> 4)
}
