use ethnum::U256;

/// floor(a * b / c) without overflow. `c` must not be zero; the result must fit in u128.
pub fn mul_div(a: u128, b: u128, c: u128) -> u128 {
    let q = U256::from(a) * U256::from(b) / U256::from(c);
    q.as_u128()
}

/// a * b >= c * d, without overflow.
pub fn product_at_least(a: u128, b: u128, c: u128, d: u128) -> bool {
    U256::from(a) * U256::from(b) >= U256::from(c) * U256::from(d)
}
