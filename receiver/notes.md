- Switched sinc() to a match statement
- Remove return from slice
- Add `anyhow` for error handling (usually used in combination with `thiserror`). It provides a compatible (impl Error) struct
simplifies Result<T,E> into a simplified Result<T>, and wraps any error automatically.
- Change main() to return Result<()> so we can use the ? operator everywhere to unwrap Result<T,E>. () is the empty tuple, which means it returns nothing.
- Added a trait to implement read_le_complex_f32() to anything implementing `Read`
- Show an example iterator reading from a file, with the whole block being used in an assignment expression.
    (We have to annotate Vec<_> as the type because we use it as &[Complex<f32>] later on, and you can't collect into a slice)
- Changed bpsk_demod to take a &[Complex<f32>] instead, there is an impl. of the `Deref` trait for converting from a Vec<T> to a &[T]
- let mut taps - we can skip the type declaration by suffixing the value with its type, like 0f32
- Note: any array indexing will is automatically bound-checked, for this reason the function style is preferred in many cases, since there are no bound checking involved
- Move convolution to a functional-style loop
- Move bit slicing to show .chunks() (which is implemented on all slices)

