use num::Complex;
use std::io::Read;
use std::io::Result;

// Since we want to read complex samples, we can define a trait and provide a default
// implementation for anything implementing `Read`

pub trait ComplexReader {
    // Reads two sequential f32 representing the real and imaginary part
    fn read_le_complex_f32(&mut self) -> Result<Complex<f32>>;
}

impl<T: Read> ComplexReader for T {
    fn read_le_complex_f32(&mut self) -> Result<Complex<f32>> {
        let mut i = [0u8; 4];
        let mut q = [0u8; 4];

        self.read_exact(&mut i)?;    // The ? operator automatically returns the error
        self.read_exact(&mut q)?;    // if anything happens during the read operation

        let i = f32::from_le_bytes(i);
        let q = f32::from_le_bytes(q);

        Ok(Complex::new(i, q))
    }
}

// The typical convention is to define a module name test, in the same file, and import the parent mod
#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use assert_approx_eq::assert_approx_eq;

    #[test]
    fn read_le_complex_f32() -> Result<()> {
        let mut f = File::open("../iq/nrf24-2460-4e6.iq")?;
        let sample = f.read_le_complex_f32()?;

        assert_approx_eq!(sample.re, -0.0056154043, 1e-4);
        assert_approx_eq!(sample.im, -0.011474957, 1e-4);

        // Returns success
        Ok(())
    }
}