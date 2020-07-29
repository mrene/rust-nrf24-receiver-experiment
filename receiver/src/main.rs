// Declares complexreader.rs as a module
mod complexreader;

use complexreader::ComplexReader;
use num::complex::Complex;
use std::fs::File;
use std::io::BufReader;
use anyhow::Result;
use num::Zero;

#[derive(Default, Clone, Copy)]
struct Filter([f32; 8]);

impl Filter {
    fn new(offset: f32) -> Self {
        let mut filter = Filter([0f32; 8]);
        for i in 0..8 {
            filter.0[i] = sinc(-4.0 + (i as f32) + offset) / 8.0;
        }
        filter
    }

    fn convolve(&self, data: &[f32]) -> f32 {
        data.iter().rev()
            .zip(self.0.iter())
            .map(|(data, tap)| data * tap)
            .sum()
    }
}

fn main() -> Result<()> {

    // path to input IQ (10 seconds @ 4MHz sample rate centered at 2460MHz)
    // - recorded with the following command on a USRP B210:
    //   $ uhd_rx_cfile -A TX/RX -r 4e6 -f 2460e6 -g 30 -N 40e6 nrf24-2460-4e6.iq
    let iq_path = "../iq/nrf24-2460-4e6.iq";

    // read the IQ file
    let samples: Vec<_> = {
        // Create a buffered reader so we limit syscalls
        let mut file = BufReader::new(File::open(iq_path).expect("Unable to open IQ file."));

        // Iterate the until we can't read anymore.
        // Iterators return None once they are done.
        let samples = std::iter::from_fn(|| {
            let sample = file.read_le_complex_f32();
            match sample {
                Ok(x) => Some(x),
                Err(_) => None,
            }
        });

        // Collect samples into a Vec
        samples.collect()
    };

    // demodulate the input IQ (2SPS) to bits
    let bits = bpsk_demod(&samples, 2.0);

    // look for packets in the demodulated bitstream
    let mut alt_count = 0;
    for i in 1..(bits.len() - 64 * 8) {

        // update the alternating bit count 
        if bits[i] != bits[i - 1] {
            alt_count += 1;
        } else {
            alt_count = 0;
        }

        // check for a possible preamble + first address bit
        if alt_count >= 9 && alt_count <= 17 {

            // parse address (assumes 4-byte address)
            let mut address: [u8; 4] = [0; 4];
            let mut offset = i;
            for ibyte in 0..4 {
                for ibit in 0..8 {
                    address[ibyte] <<= 1;
                    address[ibyte] |= bits[offset + ibyte * 8 + ibit];
                }
            }
            offset += 32;

            // parse packet length (6 bits)
            let mut length: u8 = 0;
            for ibit in 0..6 {
                length <<= 1;
                length |= bits[offset + ibit];
            }
            offset += 6;

            // filter out invalid lengths
            if length > 32 {
                continue;
            }

            // parse packet ID (2 bits)
            let pid: u8 = bits[offset] << 1 | bits[offset + 1];
            offset += 2;

            // parse no-ACK bit
            // let no_ack: u8 = bits[offset];
            offset += 1;

            // parse payload
            let mut payload: Vec<u8> = vec![0; length as usize];
            for ibyte in 0..length {
                for _ibit in 0..8 {
                    payload[ibyte as usize] <<= 1;
                    payload[ibyte as usize] |= bits[offset];
                    offset += 1;
                }
            }

            // parse CRC
            let mut crc_given: u16 = 0;
            for _ibyte in 0..2 {
                for _ibit in 0..8 {
                    crc_given <<= 1;
                    crc_given |= bits[offset] as u16;
                    offset += 1;
                }
            }

            // compute CRC
            let total_bits = 32 /* address */ + 9 /* PCF */ + (length as u32) * 8;
            let mut crc_calc: u16 = 0xffff;
            for ibit in 0..total_bits {
                if bits[i + ibit as usize] != ((crc_calc >> 15) as u8) {
                    crc_calc = (crc_calc << 1) ^ 0x1021;
                } else {
                    crc_calc <<= 1;
                }
            }

            // check CRC
            if crc_calc == crc_given {

                // print the address to stdout
                print!("address=");
                for b in address.iter() {
                    print!("{:02x}", b);
                }
                print!(",  ");

                // print the PID to stdout
                print!("pld={},  ", pid);

                // print the payload to stdout
                print!("payload=");
                for ibyte in 0..length {
                    print!("{:02x}", payload[ibyte as usize]);
                }
                println!();
            }
        }
    }

    Ok(())
}

fn slice(val: f32) -> f32 {
    if val < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn sinc(x: f32) -> f32 {

    // This is just to show that there are trait methods for everything
    // this one is defined inside `num-traits` - and if you were to
    // templatize everything to make it with with any number type (incl fixed point eventually)
    // you'd use these.
    if x.is_zero() {
        1.0
    } else {
        let pi_x = x * std::f32::consts::PI;
        pi_x.sin() / pi_x
    }
}

fn bpsk_demod(samples: &[Complex<f32>], sps: f32) -> Vec<u8> {

    // quadrature demodulate

    // We can iterate on the previous and current sample by zipping them together
    let soft_demod: Vec<_> =
        samples.iter().zip(samples[1..].iter())
            .map(|(previous, current)| {
                (current.conj() * previous).arg()
            })
            .collect();


    // generate sinc filter taps for interpolator
    let filters = {
        let mut taps: [Filter; 129] = [Default::default(); 129];

        let mut offset = 0.0;
        let step = 0.25 / 129.0;

        for filter in taps.iter_mut() {
            *filter = Filter::new(offset);
            offset += step;
        }

        taps
    };

    // clock recovery parameters and state
    let mut sps_actual = sps;
    let sps_expected = sps;
    let sps_tolerance = 0.005;
    let gain_sample_offset = 0.175;
    let gain_sps = 0.25 * gain_sample_offset * gain_sample_offset;
    let mut sample_offset = 0.5;
    let mut last_sample = 0.0;

    // perform clock recovery
    let mut interpolated: Vec<f32> = vec![0.0; soft_demod.len()];
    for i in 0..soft_demod.len() {

        // compute the interpolator filter coefficient offset
        let filter_index = (sample_offset * 129.0) as usize;

        interpolated[i] = soft_demod[i];

        if i >= 8 {
            // interpolate the output sample
            // XXX: Why is the convolution on the previous *output* samples (+ the current one)
            // instead of the previous input ones?
            interpolated[i] = filters[filter_index].convolve(&interpolated[i - 7..i + 1]);
        }
        // calculate the error value (Muller & Mueller)
        let error = slice(last_sample) * interpolated[i] - slice(interpolated[i]) * last_sample;
        last_sample = interpolated[i];

        // update the actual samples per symbol
        sps_actual += gain_sps * error;
        sps_actual = sps_actual.min(sps_expected + sps_tolerance).max(sps_expected - sps_tolerance);

        // update the fractional sample offset
        sample_offset += sps_actual + gain_sample_offset * error;
        sample_offset -= sample_offset.floor();
    }

    // Slice the bits
    // chunks returns an iterator iterating by `sps`
    let bits: Vec<_> = interpolated.chunks(sps as usize)
        .map(|symbol| {
            if symbol[0].is_sign_negative() {
                0
            } else {
                1
            }
        })
        .collect();

    bits
}
