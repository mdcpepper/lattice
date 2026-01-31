//! Prices

use std::ops::Deref;

/// Represents a price in pence/cents.
#[derive(Debug, Clone, Copy)]
pub struct Price {
    value: u64,
}

impl Price {
    /// Creates a new Price
    pub fn new(value: u64) -> Self {
        Price { value }
    }
}

impl Deref for Price {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_price() {
        let price = Price::new(1000);

        assert_eq!(price.value, 1000);
    }

    #[test]
    fn price_derefs_to_u64() {
        let price = Price { value: 100 };

        assert_eq!(*price, 100);
    }
}
