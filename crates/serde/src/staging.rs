use crate::Error;

/// A decoded value and the unconsumed suffix of the original input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staging<'a, T> {
    pub value: T,
    pub rest: &'a [u8],
}

impl<'a, T> Staging<'a, T> {
    #[must_use]
    pub const fn new(value: T, rest: &'a [u8]) -> Self {
        Self { value, rest }
    }

    #[must_use]
    pub fn map<U>(self, transform: impl FnOnce(T) -> U) -> Staging<'a, U> {
        Staging {
            value: transform(self.value),
            rest: self.rest,
        }
    }

    /// Decode the next field and combine both values as an ordered product.
    pub fn and_then<U>(
        self,
        decode_next: impl FnOnce(&'a [u8]) -> DecodeResult<'a, U>,
    ) -> DecodeResult<'a, (T, U)> {
        let next = decode_next(self.rest)?;
        Ok(Staging::new((self.value, next.value), next.rest))
    }

    /// Finish an exact decode, rejecting any unconsumed suffix.
    pub fn finish(self, total_input_len: usize) -> Result<T, Error> {
        if self.rest.is_empty() {
            Ok(self.value)
        } else {
            let consumed = total_input_len
                .checked_sub(self.rest.len())
                .ok_or_else(|| {
                    Error::wrong_format("staging suffix is longer than the declared input")
                })?;
            Err(Error::redundant_bytes(consumed, total_input_len))
        }
    }
}

pub type DecodeResult<'a, T> = Result<Staging<'a, T>, Error>;

#[cfg(test)]
mod tests {
    use super::Staging;
    use crate::{Error, fixed};

    #[test]
    fn staged_products_preserve_order_and_suffix() {
        let first = fixed::decode_i32_be(&[0, 0, 0, 1, 0, 0, 0, 2, 0xaa]).expect("first field");
        let product = first.and_then(fixed::decode_i32_be).expect("second field");
        assert_eq!(product.value, (1, 2));
        assert_eq!(product.rest, &[0xaa]);
    }

    #[test]
    fn exact_finish_rejects_a_suffix() {
        let staged = Staging::new(7_u8, &[0xaa][..]);
        assert_eq!(
            staged.finish(2),
            Err(Error::RedundantBytes {
                expected: 1,
                actual: 2,
            })
        );
    }
}
