use std::collections::HashMap;

#[cfg_attr(feature = "json", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub name: String,
    pub cents: i64,
    pub quantity: u32,
}

impl Item {
    pub fn new(name: &str, cents: i64, quantity: u32) -> Self {
        Item { name: name.to_string(), cents, quantity }
    }
}

#[cfg_attr(feature = "json", derive(serde::Serialize))]
#[derive(Debug, Default)]
pub struct Cart {
    pub items: Vec<Item>,
    pub voucher: Option<String>,
}

impl Cart {
    pub fn add(&mut self, item: Item) {
        self.items.push(item);
    }

    /// The sum of all items in cents.
    ///
    /// ```
    /// # use shop_core::{Cart, Item};
    /// let cart = Cart { items: vec![Item::new("Tea", 300, 3)], voucher: None };
    /// assert_eq!(cart.total(), 900);
    /// ```
    pub fn total(&self) -> i64 {
        self.items.iter().map(|i| i.cents * i.quantity as i64).sum()
    }

    /// Quantities by item name.
    pub fn quantities(&self) -> HashMap<String, u32> {
        let mut out = HashMap::new();
        for item in &self.items {
            *out.entry(item.name.clone()).or_insert(0) += item.quantity;
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        return self.items.len() == 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_items() {
        let mut cart = Cart::default();
        cart.add(Item::new("Coffee", 450, 2));
        assert_eq!(cart.items.len(), 1);
        assert_eq!(cart.total(), 900);
    }

    #[test]
    fn rounds_to_cents() {
        let cart = Cart { items: vec![Item::new("Bread", 239, 1), Item::new("Jam", 349, 1)], voucher: None };
        assert_eq!(cart.total(), 589);
    }

    #[test]
    #[ignore = "needs the voucher service"]
    fn vouchers() {
        assert!(Cart::default().voucher.is_none());
    }
}
