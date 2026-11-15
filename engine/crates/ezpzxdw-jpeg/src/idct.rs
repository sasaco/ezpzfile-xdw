// Malicious JPEG files can cause operations in the idct to overflow.
// One example is tests/crashtest/images/imagetestsuite/b0b8914cc5f7a6eff409f16d8cc236c5.jpg
// That's why wrapping operators are needed.

// Note: we have many values that are straight from a reference.
// Do not warn on them or try to automatically change them.
#![allow(clippy::excessive_precision)]
// Note: consistency for unrolled, scaled offset loops
#![allow(clippy::erasing_op)]
#![allow(clippy::identity_op)]
use crate::parser::Dimensions;
use core::num::Wrapping;

pub(crate) fn choose_idct_size(full_size: Dimensions, requested_size: Dimensions) -> usize {
    fn scaled(len: u16, scale: usize) -> u16 {
        ((len as u32 * scale as u32 - 1) / 8 + 1) as u16
    }

    for &scale in &[1, 2, 4] {
        if scaled(full_size.width, scale) >= requested_size.width
            || scaled(full_size.height, scale) >= requested_size.height
        {
            return scale;
        }
    }

    8
}

#[test]
fn test_choose_idct_size() {
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 200,
                height: 200
            }
        ),
        1
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 500,
                height: 500
            }
        ),
        1
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 684,
                height: 456
            }
        ),
        1
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 999,
                height: 456
            }
        ),
        1
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 684,
                height: 999
            }
        ),
        1
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 500,
                height: 333
            },
            Dimensions {
                width: 63,
                height: 42
            }
        ),
        1
    );

    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 685,
                height: 999
            }
        ),
        2
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 1000,
                height: 1000
            }
        ),
        2
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 1400,
                height: 1400
            }
        ),
        4
    );

    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 5472,
                height: 3648
            }
        ),
        8
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 16384,
                height: 16384
            }
        ),
        8
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 1,
                height: 1
            },
            Dimensions {
                width: 65535,
                height: 65535
            }
        ),
        8
    );
    assert_eq!(
        choose_idct_size(
            Dimensions {
                width: 5472,
                height: 3648
            },
            Dimensions {
                width: 16384,
                height: 16384
            }
        ),
        8
    );
}

pub(crate) fn dequantize_and_idct_block(
    scale: usize,
    coefficients: &[i16; 64],
    quantization_table: &[u16; 64],
    output_linestride: usize,
    output: &mut [u8],
) {
    match scale {
        8 => dequantize_and_idct_block_8x8(
            coefficients,
            quantization_table,
            output_linestride,
            output,
        ),
        4 => dequantize_and_idct_block_4x4(
            coefficients,
            quantization_table,
            output_linestride,
            output,
        ),
        2 => dequantize_and_idct_block_2x2(
            coefficients,
            quantization_table,
            output_linestride,
            output,
        ),
        1 => dequantize_and_idct_block_1x1(
            coefficients,
            quantization_table,
            output_linestride,
            output,
        ),
        _ => panic!("Unsupported IDCT scale {scale}/8"),
    }
}

pub fn dequantize_and_idct_block_8x8(
    coefficients: &[i16; 64],
    quantization_table: &[u16; 64],
    output_linestride: usize,
    output: &mut [u8],
) {
    #[cfg(not(feature = "platform_independent"))]
    if let Some(idct) = crate::arch::get_dequantize_and_idct_block_8x8() {
        #[allow(unsafe_code)]
        unsafe {
            return idct(coefficients, quantization_table, output_linestride, output);
        }
    }

    let output = output.chunks_mut(output_linestride);
    dequantize_and_idct_block_8x8_inner(coefficients, quantization_table, output)
}

// libjpeg-turbo 3.0.4's fast integer IDCT for 8-bit samples (`jpeg_idct_ifast` in jidctfst.c,
// the AA&N method), with the dequantisation multipliers of jddctmgr.c (`JDCT_IFAST`), translated
// to Rust. Arithmetic wraps (as in the C code with malicious coefficients) instead of panicking.
fn dequantize_and_idct_block_8x8_inner<'a, I>(
    coefficients: &[i16; 64],
    quantization_table: &[u16; 64],
    output: I,
) where
    I: IntoIterator<Item = &'a mut [u8]>,
    I::IntoIter: ExactSizeIterator<Item = &'a mut [u8]>,
{
    let output = output.into_iter();
    debug_assert!(
        output.len() >= 8,
        "Output iterator has the wrong length: {}",
        output.len()
    );

    let multipliers = ifast_multipliers(quantization_table);
    let dequantize = |i: usize| Wrapping(i32::from(coefficients[i])) * multipliers[i];

    let mut workspace = [Wrapping(0i32); 64];

    // Pass 1: process columns from input, store into work array.
    for i in 0..8 {
        if coefficients[i + 8] == 0
            && coefficients[i + 16] == 0
            && coefficients[i + 24] == 0
            && coefficients[i + 32] == 0
            && coefficients[i + 40] == 0
            && coefficients[i + 48] == 0
            && coefficients[i + 56] == 0
        {
            // AC terms all zero
            let dcval = dequantize(i);
            for row in 0..8 {
                workspace[i + 8 * row] = dcval;
            }
            continue;
        }

        let out = ifast_1d([
            dequantize(i),
            dequantize(i + 8),
            dequantize(i + 16),
            dequantize(i + 24),
            dequantize(i + 32),
            dequantize(i + 40),
            dequantize(i + 48),
            dequantize(i + 56),
        ]);
        for row in 0..8 {
            workspace[i + 8 * row] = out[row];
        }
    }

    // Pass 2: process rows from work array, store into output array.
    // The results are descaled by 8 and by the PASS1_BITS scaling, then range-limited.
    for (chunk, output_chunk) in workspace.chunks_exact(8).zip(output) {
        let chunk = <&[_; 8]>::try_from(chunk).unwrap();

        // eliminate downstream bounds checks
        let output_chunk = &mut output_chunk[..8];

        let (s0, rest) = chunk.split_first().unwrap();
        if *rest == [Wrapping(0); 7] {
            // AC terms all zero
            let dcval = ifast_output(*s0);
            output_chunk.fill(dcval);
            continue;
        }

        let out = ifast_1d(*chunk);
        for (o, v) in output_chunk.iter_mut().zip(out) {
            *o = ifast_output(v);
        }
    }
}

const IFAST_CONST_BITS: usize = 8;
const IFAST_PASS1_BITS: usize = 2;

const FIX_1_082392200: Wrapping<i32> = Wrapping(277); // FIX(1.082392200)
const FIX_1_414213562: Wrapping<i32> = Wrapping(362); // FIX(1.414213562)
const FIX_1_847759065: Wrapping<i32> = Wrapping(473); // FIX(1.847759065)
const FIX_2_613125930: Wrapping<i32> = Wrapping(669); // FIX(2.613125930)

// jddctmgr.c: AA&N scale factors (scalefactor[row] * scalefactor[col]) scaled up by 14 bits.
const AAN_CONST_BITS: usize = 14;
// jdct.h: fractional bits in the IFAST multipliers (8-bit samples).
const IFAST_SCALE_BITS: usize = 2;
#[rustfmt::skip]
const AANSCALES: [i32; 64] = [
    16384, 22725, 21407, 19266, 16384, 12873,  8867,  4520,
    22725, 31521, 29692, 26722, 22725, 17855, 12299,  6270,
    21407, 29692, 27969, 25172, 21407, 16819, 11585,  5906,
    19266, 26722, 25172, 22654, 19266, 15137, 10426,  5315,
    16384, 22725, 21407, 19266, 16384, 12873,  8867,  4520,
    12873, 17855, 16819, 15137, 12873, 10114,  6967,  3552,
     8867, 12299, 11585, 10426,  8867,  6967,  4799,  2446,
     4520,  6270,  5906,  5315,  4520,  3552,  2446,  1247,
];

// jddctmgr.c, JDCT_IFAST: DESCALE(quantval * aanscales, CONST_BITS - IFAST_SCALE_BITS), stored
// as IFAST_MULT_TYPE, which is `short` in libjpeg-turbo built with SIMD.
fn ifast_multipliers(quantization_table: &[u16; 64]) -> [Wrapping<i32>; 64] {
    const SHIFT: usize = AAN_CONST_BITS - IFAST_SCALE_BITS;
    let mut multipliers = [Wrapping(0i32); 64];
    for ((m, &q), &aan) in multipliers
        .iter_mut()
        .zip(quantization_table.iter())
        .zip(AANSCALES.iter())
    {
        let x = (Wrapping(i32::from(q)) * Wrapping(aan) + Wrapping(1 << (SHIFT - 1))) >> SHIFT;
        *m = Wrapping(i32::from(x.0 as i16));
    }
    multipliers
}

// MULTIPLY(var, const): DESCALE((var) * (const), CONST_BITS) without rounding
// (USE_ACCURATE_ROUNDING is not defined).
#[inline(always)]
fn ifast_multiply(var: Wrapping<i32>, constant: Wrapping<i32>) -> Wrapping<i32> {
    (var * constant) >> IFAST_CONST_BITS
}

// One 1-D pass of jpeg_idct_ifast. `s` is in natural order (s[0] = DC); the result is in output
// order (out[0] .. out[7]).
#[inline(always)]
fn ifast_1d(s: [Wrapping<i32>; 8]) -> [Wrapping<i32>; 8] {
    // Even part
    let tmp0 = s[0];
    let tmp1 = s[2];
    let tmp2 = s[4];
    let tmp3 = s[6];

    let tmp10 = tmp0 + tmp2; // phase 3
    let tmp11 = tmp0 - tmp2;

    let tmp13 = tmp1 + tmp3; // phases 5-3
    let tmp12 = ifast_multiply(tmp1 - tmp3, FIX_1_414213562) - tmp13; // 2*c4

    let tmp0 = tmp10 + tmp13; // phase 2
    let tmp3 = tmp10 - tmp13;
    let tmp1 = tmp11 + tmp12;
    let tmp2 = tmp11 - tmp12;

    // Odd part
    let tmp4 = s[1];
    let tmp5 = s[3];
    let tmp6 = s[5];
    let tmp7 = s[7];

    let z13 = tmp6 + tmp5; // phase 6
    let z10 = tmp6 - tmp5;
    let z11 = tmp4 + tmp7;
    let z12 = tmp4 - tmp7;

    let tmp7 = z11 + z13; // phase 5
    let tmp11 = ifast_multiply(z11 - z13, FIX_1_414213562); // 2*c4

    let z5 = ifast_multiply(z10 + z12, FIX_1_847759065); // 2*c2
    let tmp10 = ifast_multiply(z12, FIX_1_082392200) - z5; // 2*(c2-c6)
    let tmp12 = ifast_multiply(z10, -FIX_2_613125930) + z5; // -2*(c2+c6)

    let tmp6 = tmp12 - tmp7; // phase 2
    let tmp5 = tmp11 - tmp6;
    let tmp4 = tmp10 + tmp5;

    [
        tmp0 + tmp7,
        tmp1 + tmp6,
        tmp2 + tmp5,
        tmp3 - tmp4,
        tmp3 + tmp4,
        tmp2 - tmp5,
        tmp1 - tmp6,
        tmp0 - tmp7,
    ]
}

// Final output stage: IDESCALE(x, PASS1_BITS + 3) (a plain arithmetic shift) and range-limit.
// libjpeg indexes range_limit[x & RANGE_MASK]; this saturates instead, which is equal for every
// value a valid stream produces and is what libjpeg-turbo's SIMD ifast IDCT does.
#[inline(always)]
fn ifast_output(x: Wrapping<i32>) -> u8 {
    ((x >> (IFAST_PASS1_BITS + 3)).0 + 128).clamp(0, 255) as u8
}

// 4x4 and 2x2 IDCT based on Rakesh Dugad and Narendra Ahuja: "A Fast Scheme for Image Size Change in the Compressed Domain" (2001).
// http://sylvana.net/jpegcrop/jidctred/
fn dequantize_and_idct_block_4x4(
    coefficients: &[i16; 64],
    quantization_table: &[u16; 64],
    output_linestride: usize,
    output: &mut [u8],
) {
    debug_assert_eq!(coefficients.len(), 64);
    let mut temp = [Wrapping(0i32); 4 * 4];

    const CONST_BITS: usize = 12;
    const PASS1_BITS: usize = 2;
    const FINAL_BITS: usize = CONST_BITS + PASS1_BITS + 3;

    // columns
    for i in 0..4 {
        let s0 = Wrapping(coefficients[i + 8 * 0] as i32 * quantization_table[i + 8 * 0] as i32);
        let s1 = Wrapping(coefficients[i + 8 * 1] as i32 * quantization_table[i + 8 * 1] as i32);
        let s2 = Wrapping(coefficients[i + 8 * 2] as i32 * quantization_table[i + 8 * 2] as i32);
        let s3 = Wrapping(coefficients[i + 8 * 3] as i32 * quantization_table[i + 8 * 3] as i32);

        let x0 = (s0 + s2) << PASS1_BITS;
        let x2 = (s0 - s2) << PASS1_BITS;

        let p1 = (s1 + s3) * stbi_f2f(0.541196100);
        let t0 = (p1 + s3 * stbi_f2f(-1.847759065) + Wrapping(512)) >> (CONST_BITS - PASS1_BITS);
        let t2 = (p1 + s1 * stbi_f2f(0.765366865) + Wrapping(512)) >> (CONST_BITS - PASS1_BITS);

        temp[i + 4 * 0] = x0 + t2;
        temp[i + 4 * 3] = x0 - t2;
        temp[i + 4 * 1] = x2 + t0;
        temp[i + 4 * 2] = x2 - t0;
    }

    for i in 0..4 {
        let s0 = temp[i * 4 + 0];
        let s1 = temp[i * 4 + 1];
        let s2 = temp[i * 4 + 2];
        let s3 = temp[i * 4 + 3];

        let x0 = (s0 + s2) << CONST_BITS;
        let x2 = (s0 - s2) << CONST_BITS;

        let p1 = (s1 + s3) * stbi_f2f(0.541196100);
        let t0 = p1 + s3 * stbi_f2f(-1.847759065);
        let t2 = p1 + s1 * stbi_f2f(0.765366865);

        // constants scaled things up by 1<<12, plus we had 1<<2 from first
        // loop, plus horizontal and vertical each scale by sqrt(8) so together
        // we've got an extra 1<<3, so 1<<17 total we need to remove.
        // so we want to round that, which means adding 0.5 * 1<<17,
        // aka 65536. Also, we'll end up with -128 to 127 that we want
        // to encode as 0..255 by adding 128, so we'll add that before the shift
        let x0 = x0 + Wrapping(1 << (FINAL_BITS - 1)) + Wrapping(128 << FINAL_BITS);
        let x2 = x2 + Wrapping(1 << (FINAL_BITS - 1)) + Wrapping(128 << FINAL_BITS);

        let output = &mut output[i * output_linestride..][..4];
        output[0] = stbi_clamp((x0 + t2) >> FINAL_BITS);
        output[3] = stbi_clamp((x0 - t2) >> FINAL_BITS);
        output[1] = stbi_clamp((x2 + t0) >> FINAL_BITS);
        output[2] = stbi_clamp((x2 - t0) >> FINAL_BITS);
    }
}

fn dequantize_and_idct_block_2x2(
    coefficients: &[i16; 64],
    quantization_table: &[u16; 64],
    output_linestride: usize,
    output: &mut [u8],
) {
    debug_assert_eq!(coefficients.len(), 64);

    const SCALE_BITS: usize = 3;

    // Column 0
    let s00 = Wrapping(coefficients[8 * 0] as i32 * quantization_table[8 * 0] as i32);
    let s10 = Wrapping(coefficients[8 * 1] as i32 * quantization_table[8 * 1] as i32);

    let x0 = s00 + s10;
    let x2 = s00 - s10;

    // Column 1
    let s01 = Wrapping(coefficients[8 * 0 + 1] as i32 * quantization_table[8 * 0 + 1] as i32);
    let s11 = Wrapping(coefficients[8 * 1 + 1] as i32 * quantization_table[8 * 1 + 1] as i32);

    let x1 = s01 + s11;
    let x3 = s01 - s11;

    let x0 = x0 + Wrapping(1 << (SCALE_BITS - 1)) + Wrapping(128 << SCALE_BITS);
    let x2 = x2 + Wrapping(1 << (SCALE_BITS - 1)) + Wrapping(128 << SCALE_BITS);

    // Row 0
    output[0] = stbi_clamp((x0 + x1) >> SCALE_BITS);
    output[1] = stbi_clamp((x0 - x1) >> SCALE_BITS);

    // Row 1
    output[output_linestride + 0] = stbi_clamp((x2 + x3) >> SCALE_BITS);
    output[output_linestride + 1] = stbi_clamp((x2 - x3) >> SCALE_BITS);
}

fn dequantize_and_idct_block_1x1(
    coefficients: &[i16; 64],
    quantization_table: &[u16; 64],
    _output_linestride: usize,
    output: &mut [u8],
) {
    debug_assert_eq!(coefficients.len(), 64);

    let s0 = (Wrapping(coefficients[0] as i32 * quantization_table[0] as i32) + Wrapping(128 * 8)) / Wrapping(8);
    output[0] = stbi_clamp(s0);
}

// take a -128..127 value and stbi__clamp it and convert to 0..255
fn stbi_clamp(x: Wrapping<i32>) -> u8 {
    x.0.max(0).min(255) as u8
}

fn stbi_f2f(x: f32) -> Wrapping<i32> {
    Wrapping((x * 4096.0 + 0.5) as i32)
}

#[test]
fn test_dequantize_and_idct_block_8x8() {
    #[rustfmt::skip]
    let coefficients: [i16; 8 * 8] = [
        -14, -39, 58, -2, 3, 3, 0, 1,
        11, 27, 4, -3, 3, 0, 1, 0,
        -6, -13, -9, -1, -2, -1, 0, 0,
        -4, 0, -1, -2, 0, 0, 0, 0,
        3, 0, 0, 0, 0, 0, 0, 0,
        -3, -2, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0
    ];

    #[rustfmt::skip]
    let quantization_table: [u16; 8 * 8] = [
        8, 6, 5, 8, 12, 20, 26, 31,
        6, 6, 7, 10, 13, 29, 30, 28,
        7, 7, 8, 12, 20, 29, 35, 28,
        7, 9, 11, 15, 26, 44, 40, 31,
        9, 11, 19, 28, 34, 55, 52, 39,
        12, 18, 28, 32, 41, 52, 57, 46,
        25, 32, 39, 44, 52, 61, 60, 51,
        36, 46, 48, 49, 56, 50, 52, 50
    ];
    let output_linestride: usize = 8;
    let mut output = [0u8; 8 * 8];
    dequantize_and_idct_block_8x8(
        &coefficients,
        &quantization_table,
        output_linestride,
        &mut output,
    );
    #[rustfmt::skip]
    let expected_output = [
        118, 92, 110, 83, 77, 93, 144, 198,
        172, 116, 114, 87, 78, 93, 146, 191,
        194, 107, 91, 76, 71, 93, 160, 198,
        196, 100, 80, 74, 67, 92, 174, 209,
        182, 104, 88, 81, 68, 89, 178, 206,
        105, 64, 59, 59, 63, 94, 183, 201,
        35, 27, 28, 37, 72, 121, 203, 204,
        37, 45, 41, 47, 98, 154, 223, 208
    ];
    for i in 0..64 {
        assert!((output[i] as i16 - expected_output[i] as i16).abs() <= 1);
    }
}

#[test]
fn test_dequantize_and_idct_block_8x8_all_zero() {
    let mut output = [0u8; 8 * 8];
    dequantize_and_idct_block_8x8(&[0; 8 * 8], &[666; 8 * 8], 8, &mut output);
    assert_eq!(&output[..], &[128; 8 * 8][..]);
}

#[test]
fn test_dequantize_and_idct_block_8x8_saturated() {
    // Arch-specific IDCT implementations need not handle i16::MAX values.
    #[cfg(not(feature = "platform_independent"))]
    if crate::arch::get_dequantize_and_idct_block_8x8().is_some() {
        return;
    }
    let mut output = [0u8; 8 * 8];
    dequantize_and_idct_block_8x8(&[i16::MAX; 8 * 8], &[u16::MAX; 8 * 8], 8, &mut output);
    // The values wrap (as in libjpeg with such coefficients); the test is that nothing panics,
    // also in debug builds with overflow checks.
}
