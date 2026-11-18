#[allow(unused_imports)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_pixels(
    reader: &mut BitReader<'_>,
    width: usize,
    height: usize,
    groups: &Groups,
    huffman_image: Option<&MetaImage>,
    prefix_bits: u32,
    cache: &mut Option<ColorCache>,
    limits: &PngLimits,
) -> Result<Vec<u32>, ImageError> {
    let pixels = check_dimensions(width, height, limits)?;
    let mut data: Vec<u32> = Vec::with_capacity(pixels);
    while data.len() < pixels {
        let group_index = match huffman_image {
            Some(image) => {
                let x = data.len() % width;
                let y = data.len() / width;
                image.group(x, y, prefix_bits, groups.codes.len())?
            }
            None => 0,
        };
        let group = groups.get(group_index)?;
        let green_symbol = group[0].decode(reader)?;
        if green_symbol < NUM_LITERAL_CODES {
            let red = group[1].decode(reader)? as u8;
            let blue = group[2].decode(reader)? as u8;
            let alpha = group[3].decode(reader)? as u8;
            let argb = (u32::from(alpha) << 24)
                | (u32::from(red) << 16)
                | (u32::from(green_symbol as u8) << 8)
                | u32::from(blue);
            data.push(argb);
            if let Some(cache) = cache.as_mut() {
                cache.insert(argb);
            }
        } else if green_symbol < NUM_LITERAL_CODES + NUM_LENGTH_CODES {
            let length = get_copy_value(green_symbol - NUM_LITERAL_CODES, reader)? as usize;
            if length == 0 {
                return Err(ImageError::InvalidData {
                    reason: "VP8L backward reference with zero length",
                });
            }
            let distance_symbol = group[4].decode(reader)?;
            let distance_code = get_copy_value(distance_symbol, reader)?;
            let distance = plane_code_to_distance(width, distance_code);
            if distance > data.len() {
                return Err(ImageError::InvalidData {
                    reason: "VP8L backward reference before the start of the image",
                });
            }
            if length > pixels - data.len() {
                return Err(ImageError::InvalidData {
                    reason: "VP8L backward reference exceeds the image",
                });
            }
            for _ in 0..length {
                let value = data[data.len() - distance];
                data.push(value);
                if let Some(cache) = cache.as_mut() {
                    cache.insert(value);
                }
            }
        } else {
            let index = usize::from(green_symbol - NUM_LITERAL_CODES - NUM_LENGTH_CODES);
            let value = match cache.as_ref() {
                Some(cache) => cache.lookup(index)?,
                None => {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L colour-cache code without a cache",
                    });
                }
            };
            data.push(value);
            if let Some(cache) = cache.as_mut() {
                cache.insert(value);
            }
        }
    }
    Ok(data)
}

/// Meta Huffman image: groups a `VP8L` block to its prefix-code group.
pub(crate) struct MetaImage {
    pub(crate) width: usize,
    pub(crate) groups: Vec<u16>,
}

impl MetaImage {
    pub(crate) fn group(
        &self,
        x: usize,
        y: usize,
        prefix_bits: u32,
        count: usize,
    ) -> Result<usize, ImageError> {
        let index = (y >> prefix_bits) * self.width + (x >> prefix_bits);
        let value = self
            .groups
            .get(index)
            .copied()
            .ok_or(ImageError::InvalidData {
                reason: "VP8L meta Huffman image index out of range",
            })?;
        let value = usize::from(value);
        if value >= count {
            return Err(ImageError::InvalidData {
                reason: "VP8L meta Huffman code group out of range",
            });
        }
        Ok(value)
    }
}

pub(crate) fn decode_main_pixels(
    reader: &mut BitReader<'_>,
    width: usize,
    height: usize,
    limits: &PngLimits,
) -> Result<Vec<u32>, ImageError> {
    let mut cache = read_color_cache(reader)?;
    let has_meta = reader.read_bit()? == 1;
    let mut huffman_image = None;
    let mut prefix_bits = 0u32;
    let group_count;
    if has_meta {
        prefix_bits = reader.read_bits(3)? + 2;
        let meta_width = width.div_ceil(1usize << prefix_bits);
        let meta_height = height.div_ceil(1usize << prefix_bits);
        let meta_pixels = decode_entropy_image(reader, meta_width, meta_height, limits)?;
        let mut max_group = 0usize;
        let mut groups = Vec::with_capacity(meta_pixels.len());
        for pixel in &meta_pixels {
            let group = ((pixel >> 8) & 0xffff) as usize;
            max_group = max_group.max(group);
            groups.push(group as u16);
        }
        group_count = max_group + 1;
        huffman_image = Some(MetaImage {
            width: meta_width,
            groups,
        });
    } else {
        group_count = 1;
    }
    let groups = Groups::read(reader, cache_size_bits(&cache), group_count)?;
    decode_pixels(
        reader,
        width,
        height,
        &groups,
        huffman_image.as_ref(),
        prefix_bits,
        &mut cache,
        limits,
    )
}

// ---------------------------------------------------------------------------
// Transforms (inverse application)
// ---------------------------------------------------------------------------
