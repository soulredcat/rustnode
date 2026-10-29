//! `util.U256` and `util.I256` operations whose edge cases follow Java's
//! `BigInteger`.

use alephium_model::{I256, U256};
use num_bigint::{BigInt, Sign};
use num_traits::{ToPrimitive, Zero};

fn big_u(value: U256) -> BigInt {
    BigInt::from_bytes_be(Sign::Plus, &value.to_be_bytes())
}

fn big_i(value: I256) -> BigInt {
    BigInt::from_signed_bytes_be(&value.to_be_bytes())
}

fn u256(value: &BigInt) -> Option<U256> {
    if value.sign() == Sign::Minus || value.bits() > 256 {
        return None;
    }
    let (_, bytes) = value.to_bytes_be();
    let mut array = [0u8; 32];
    array[32 - bytes.len()..].copy_from_slice(&bytes);
    Some(U256::from_be_bytes(array))
}

/// `I256.validate`: `bitLength <= 255`.
fn i256(value: &BigInt) -> Option<I256> {
    if java_bit_length(value) > 255 {
        return None;
    }
    let bytes = value.to_signed_bytes_be();
    let fill = if value.sign() == Sign::Minus { 0xff } else { 0 };
    let mut array = [fill; 32];
    array[32 - bytes.len()..].copy_from_slice(&bytes);
    Some(I256::from_be_bytes(array))
}

/// `BigInteger.bitLength`: the bits of the two's complement value without
/// its sign bit.
fn java_bit_length(value: &BigInt) -> u64 {
    if value.sign() == Sign::Minus {
        (-value - 1u32).bits()
    } else {
        value.bits()
    }
}

/// `U256.byteLength() >= 3`: shifts by `2^16` or more.
fn shift_amount(n: U256) -> Option<u32> {
    (n < U256::from(1u32 << 16)).then(|| n.as_u32())
}

pub fn u256_shl(x: U256, n: U256) -> Option<U256> {
    if x == U256::ZERO {
        return Some(U256::ZERO);
    }
    let n = shift_amount(n)?;
    if n >= 256 {
        return None;
    }
    u256(&(big_u(x) << n))
}

pub fn u256_shr(x: U256, n: U256) -> U256 {
    match shift_amount(n) {
        Some(n) if x != U256::ZERO && n < 256 => x >> n,
        _ => U256::ZERO,
    }
}

pub fn i256_shl(x: I256, n: U256) -> Option<I256> {
    if x == I256::ZERO {
        return Some(I256::ZERO);
    }
    let n = shift_amount(n)?;
    if n >= 256 {
        return None;
    }
    i256(&(big_i(x) << n))
}

/// Negative values shift toward zero.
pub fn i256_shr(x: I256, n: U256) -> I256 {
    let Some(n) = shift_amount(n) else {
        return I256::ZERO;
    };
    if x == I256::ZERO {
        return I256::ZERO;
    }
    let value = big_i(x);
    let shifted = if value.sign() == Sign::Minus {
        -((-value) >> n)
    } else {
        value >> n
    };
    i256(&shifted).unwrap_or(I256::ZERO)
}

pub fn i256_div(x: I256, y: I256) -> Option<I256> {
    x.checked_div(y)
}

pub fn i256_mod(x: I256, y: I256) -> Option<I256> {
    x.checked_rem(y)
}

pub fn i256_round_infinity_div(x: I256, y: I256) -> Option<I256> {
    if y == I256::ZERO || (y == I256::MINUS_ONE && x == I256::MIN) {
        return None;
    }
    let (x, y) = (big_i(x), big_i(y));
    let same_sign = (x.sign() != Sign::Minus && y.sign() == Sign::Plus)
        || (x.sign() != Sign::Plus && y.sign() == Sign::Minus);
    let magnitude = (x.magnitude() + y.magnitude() - 1u32) / y.magnitude();
    let magnitude = BigInt::from_biguint(Sign::Plus, magnitude);
    i256(&if same_sign { magnitude } else { -magnitude })
}

pub fn u256_round_infinity_div(x: U256, y: U256) -> Option<U256> {
    if y == U256::ZERO {
        return None;
    }
    u256(&((big_u(x) + big_u(y) - 1u32) / big_u(y)))
}

/// `U256.toInt`.
fn exponent(exp: U256) -> Option<u32> {
    (exp < U256::from(1u32 << 31)).then(|| exp.as_u32())
}

pub fn u256_pow(base: U256, exp: U256) -> Option<U256> {
    if base == U256::ZERO {
        return Some(if exp == U256::ZERO {
            U256::ONE
        } else {
            U256::ZERO
        });
    }
    if base == U256::ONE {
        return Some(U256::ONE);
    }
    let n = exponent(exp)?;
    let bits = big_u(base).bits();
    if n > 256 || (bits - 1) * u64::from(n) > 256 {
        return None;
    }
    u256(&big_u(base).pow(n))
}

pub fn i256_pow(base: I256, exp: U256) -> Option<I256> {
    if base == I256::ZERO {
        return Some(if exp == U256::ZERO {
            I256::ONE
        } else {
            I256::ZERO
        });
    }
    if base == I256::ONE {
        return Some(I256::ONE);
    }
    if base == I256::MINUS_ONE {
        return Some(if exp & U256::ONE == U256::ONE {
            I256::MINUS_ONE
        } else {
            I256::ONE
        });
    }
    let n = exponent(exp)?;
    let bits = java_bit_length(&big_i(base));
    if n > 256 || (bits.saturating_sub(1)) * u64::from(n) > 256 {
        return None;
    }
    i256(&big_i(base).pow(n))
}

/// `modPow(exp, 2^256)`.
pub fn u256_mod_pow(base: U256, exp: U256) -> U256 {
    let mut result = U256::ONE;
    let mut base = base;
    let mut exp = exp;
    while exp != U256::ZERO {
        if exp & U256::ONE == U256::ONE {
            result = result.wrapping_mul(base);
        }
        base = base.wrapping_mul(base);
        exp >>= 1;
    }
    result
}

/// `mulModN` / `addModN`.
pub fn mod_n(x: U256, y: U256, n: U256, multiply: bool) -> Option<U256> {
    if n == U256::ZERO {
        return None;
    }
    let value = if multiply {
        big_u(x) * big_u(y)
    } else {
        big_u(x) + big_u(y)
    };
    let remainder = value % big_u(n);
    if remainder.is_zero() {
        return Some(U256::ZERO);
    }
    remainder
        .to_u128()
        .map_or_else(|| u256(&remainder), |small| Some(U256::from(small)))
}
