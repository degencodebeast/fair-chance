use ethnum::U256;

/// floor(a * b / c) without overflow. `c` must not be zero; the result must fit in u128.
pub fn mul_div(a: u128, b: u128, c: u128) -> u128 {
    let q = U256::from(a) * U256::from(b) / U256::from(c);
    q.as_u128()
}

/// ceil(a * b / c) without overflow. `c` must not be zero; the result must fit in u128.
pub fn mul_div_up(a: u128, b: u128, c: u128) -> u128 {
    let c = U256::from(c);
    let q = (U256::from(a) * U256::from(b) + c - 1) / c;
    q.as_u128()
}

/// a * b >= c * d, without overflow.
pub fn product_at_least(a: u128, b: u128, c: u128, d: u128) -> bool {
    U256::from(a) * U256::from(b) >= U256::from(c) * U256::from(d)
}

/// floor(sqrt(n)) by Newton's method, starting at a power of two at or above the root.
fn isqrt(n: U256) -> U256 {
    if n < U256::new(2) {
        return n;
    }
    let bits = 256 - n.leading_zeros();
    let mut x = U256::ONE << bits.div_ceil(2);
    loop {
        let next = (x + n / x) >> 1u32;
        if next >= x {
            return x;
        }
        x = next;
    }
}

/// The largest q with (y / (y + q))^2 >= (10_000 - drop_bps) / 10_000: the most tokens a sell can
/// put into a constant-product pool with token reserve y while the price falls by at most
/// `drop_bps`. `drop_bps` must be below 10_000.
pub fn tokens_for_price_drop(y: u128, drop_bps: u32) -> u128 {
    let y = U256::from(y);
    let bound = y * y * U256::from(10_000u32) / U256::from(10_000 - drop_bps);
    (isqrt(bound) - y).as_u128()
}
