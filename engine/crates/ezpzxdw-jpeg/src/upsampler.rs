use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use crate::error::{Error, Result, UnsupportedFeature};
use crate::parser::Component;

pub struct Upsampler {
    components: Vec<UpsamplerComponent>,
    line_buffer_size: usize
}

struct UpsamplerComponent {
    upsampler: Box<dyn Upsample + Sync>,
    width: usize,
    height: usize,
    row_stride: usize,
}

impl Upsampler {
    pub fn new(components: &[Component], output_width: u16, output_height: u16) -> Result<Upsampler> {
        let h_max = components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap();
        let v_max = components.iter().map(|c| c.vertical_sampling_factor).max().unwrap();
        let mut upsampler_components = Vec::with_capacity(components.len());

        for component in components {
            let upsampler = choose_upsampler((component.horizontal_sampling_factor,
                                              component.vertical_sampling_factor),
                                             (h_max, v_max),
                                             output_width,
                                             output_height)?;
            upsampler_components.push(UpsamplerComponent {
                upsampler,
                width: component.size.width as usize,
                height: component.size.height as usize,
                row_stride: component.block_size.width as usize * component.dct_scale,
            });
        }

        let buffer_size = components.iter().map(|c| c.size.width).max().unwrap() as usize * h_max as usize;

        Ok(Upsampler {
            components: upsampler_components,
            line_buffer_size: buffer_size
        })
    }

    pub fn upsample_and_interleave_row(&self, component_data: &[Vec<u8>], row: usize, output_width: usize, output: &mut [u8], color_convert: fn(&[Vec<u8>], &mut [u8])) {
        let component_count = component_data.len();
        let mut line_buffers = vec![vec![0u8; self.line_buffer_size]; component_count];

        debug_assert_eq!(component_count, self.components.len());

        for (i, component) in self.components.iter().enumerate() {
            component.upsampler.upsample_row(&component_data[i],
                                             component.width,
                                             component.height,
                                             component.row_stride,
                                             row,
                                             output_width,
                                             &mut line_buffers[i]);
        }
        color_convert(&line_buffers, output);
    }
}

struct UpsamplerH1V1;
struct UpsamplerH2V1;
struct UpsamplerH1V2;
struct UpsamplerH2V2;

struct UpsamplerGeneric {
    horizontal_scaling_factor: u8,
    vertical_scaling_factor: u8
}

// As libjpeg-turbo's jinit_upsampler (jdsample.c) with do_fancy_upsampling: decided from the
// sampling factors only (libjpeg has no shortcut for 1-pixel-wide or 1-pixel-high images).
// Whether a 2h component is wide enough for fancy upsampling (downsampled_width > 2) is decided
// in the upsamplers, which see the component's width.
fn choose_upsampler(sampling_factors: (u8, u8),
                    max_sampling_factors: (u8, u8),
                    _output_width: u16,
                    _output_height: u16) -> Result<Box<dyn Upsample + Sync>> {
    let h1 = sampling_factors.0 == max_sampling_factors.0;
    let v1 = sampling_factors.1 == max_sampling_factors.1;
    let h2 = sampling_factors.0 * 2 == max_sampling_factors.0;
    let v2 = sampling_factors.1 * 2 == max_sampling_factors.1;

    if h1 && v1 {
        Ok(Box::new(UpsamplerH1V1))
    } else if h2 && v1 {
        Ok(Box::new(UpsamplerH2V1))
    } else if h1 && v2 {
        Ok(Box::new(UpsamplerH1V2))
    } else if h2 && v2 {
        Ok(Box::new(UpsamplerH2V2))
    } else if max_sampling_factors.0 % sampling_factors.0 != 0
        || max_sampling_factors.1 % sampling_factors.1 != 0
    {
        Err(Error::Unsupported(
            UnsupportedFeature::NonIntegerSubsamplingRatio,
        ))
    } else {
        Ok(Box::new(UpsamplerGeneric {
            horizontal_scaling_factor: max_sampling_factors.0 / sampling_factors.0,
            vertical_scaling_factor: max_sampling_factors.1 / sampling_factors.1,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
trait Upsample {
    fn upsample_row(&self,
                    input: &[u8],
                    input_width: usize,
                    input_height: usize,
                    row_stride: usize,
                    row: usize,
                    output_width: usize,
                    output: &mut [u8]);
}

impl Upsample for UpsamplerH1V1 {
    fn upsample_row(&self,
                    input: &[u8],
                    _input_width: usize,
                    _input_height: usize,
                    row_stride: usize,
                    row: usize,
                    output_width: usize,
                    output: &mut [u8]) {
        let input = &input[row * row_stride ..];

        output[..output_width].copy_from_slice(&input[..output_width]);
    }
}

// libjpeg-turbo's h2v1_fancy_upsample (jdsample.c) when the component is more than 2 samples
// wide, otherwise h2v1_upsample (replication), as jinit_upsampler chooses.
impl Upsample for UpsamplerH2V1 {
    fn upsample_row(&self,
                    input: &[u8],
                    input_width: usize,
                    _input_height: usize,
                    row_stride: usize,
                    row: usize,
                    _output_width: usize,
                    output: &mut [u8]) {
        let input = &input[row * row_stride ..][..input_width];

        if input_width <= 2 {
            for (i, &value) in input.iter().enumerate() {
                output[i * 2] = value;
                output[i * 2 + 1] = value;
            }
            return;
        }

        let output = &mut output[..input_width * 2];

        // Special case for first column
        output[0] = input[0];
        output[1] = ((input[0] as u32 * 3 + input[1] as u32 + 2) >> 2) as u8;

        // General case: 3/4 * nearer pixel + 1/4 * further pixel
        for i in 1 .. input_width - 1 {
            let invalue = input[i] as u32 * 3;
            output[i * 2]     = ((invalue + input[i - 1] as u32 + 1) >> 2) as u8;
            output[i * 2 + 1] = ((invalue + input[i + 1] as u32 + 2) >> 2) as u8;
        }

        // Special case for last column
        let last = input_width - 1;
        output[last * 2] = ((input[last] as u32 * 3 + input[last - 1] as u32 + 1) >> 2) as u8;
        output[last * 2 + 1] = input[last];
    }
}

// Replication of each input row into two output rows, as libjpeg-turbo before 2.0 does for 1h2v
// (int_upsample); DocuWorks 9 draws 4:4:0 pictures this way, not with h1v2_fancy_upsample.
impl Upsample for UpsamplerH1V2 {
    fn upsample_row(&self,
                    input: &[u8],
                    _input_width: usize,
                    _input_height: usize,
                    row_stride: usize,
                    row: usize,
                    output_width: usize,
                    output: &mut [u8]) {
        let input = &input[(row / 2) * row_stride ..];

        output[..output_width].copy_from_slice(&input[..output_width]);
    }
}

// libjpeg-turbo's h2v2_fancy_upsample (jdsample.c) when the component is more than 2 samples
// wide, otherwise h2v2_upsample (replication in both directions), as jinit_upsampler chooses.
// The row above the first row and the row below the last row are the edge row itself, as
// libjpeg's context rows.
impl Upsample for UpsamplerH2V2 {
    fn upsample_row(&self,
                    input: &[u8],
                    input_width: usize,
                    input_height: usize,
                    row_stride: usize,
                    row: usize,
                    _output_width: usize,
                    output: &mut [u8]) {
        if input_width <= 2 {
            let input = &input[(row / 2) * row_stride ..][..input_width];
            for (i, &value) in input.iter().enumerate() {
                output[i * 2] = value;
                output[i * 2 + 1] = value;
            }
            return;
        }

        let row_near = row as f32 / 2.0;
        // If row_near's fractional is 0.0 we want row_far to be the previous row and if it's 0.5 we
        // want it to be the next row.
        let row_far = (row_near + row_near.fract() * 3.0 - 0.25).min((input_height - 1) as f32);

        let input_near = &input[row_near as usize * row_stride ..][..input_width];
        let input_far = &input[row_far as usize * row_stride ..][..input_width];
        let output = &mut output[..input_width * 2];

        let colsum = |i: usize| input_near[i] as u32 * 3 + input_far[i] as u32;

        // Special case for first column
        let mut thiscolsum = colsum(0);
        let mut nextcolsum = colsum(1);
        output[0] = ((thiscolsum * 4 + 8) >> 4) as u8;
        output[1] = ((thiscolsum * 3 + nextcolsum + 7) >> 4) as u8;
        let mut lastcolsum = thiscolsum;
        thiscolsum = nextcolsum;

        // General case: 3/4 * nearer pixel + 1/4 * further pixel in each dimension, thus 9/16,
        // 3/16, 3/16, 1/16 overall
        for i in 2 .. input_width {
            nextcolsum = colsum(i);
            output[(i - 1) * 2]     = ((thiscolsum * 3 + lastcolsum + 8) >> 4) as u8;
            output[(i - 1) * 2 + 1] = ((thiscolsum * 3 + nextcolsum + 7) >> 4) as u8;
            lastcolsum = thiscolsum;
            thiscolsum = nextcolsum;
        }

        // Special case for last column
        let last = input_width - 1;
        output[last * 2]     = ((thiscolsum * 3 + lastcolsum + 8) >> 4) as u8;
        output[last * 2 + 1] = ((thiscolsum * 4 + 7) >> 4) as u8;
    }
}

impl Upsample for UpsamplerGeneric {
    // Uses nearest neighbor sampling
    fn upsample_row(&self,
                    input: &[u8],
                    input_width: usize,
                    _input_height: usize,
                    row_stride: usize,
                    row: usize,
                    _output_width: usize,
                    output: &mut [u8]) {
        let mut index = 0;
        let start = (row / self.vertical_scaling_factor as usize) * row_stride;
        let input = &input[start..(start + input_width)];
        for val in input {
            for _ in 0..self.horizontal_scaling_factor {
                output[index] = *val;
                index += 1;
            }
        }
    }
}
