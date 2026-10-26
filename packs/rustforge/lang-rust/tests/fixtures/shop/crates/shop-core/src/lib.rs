//! Carts, money and discounts of the shop.
//!
//! ```
//! use shop_core::{Cart, Item};
//!
//! let mut cart = Cart::default();
//! cart.add(Item::new("Coffee", 450, 2));
//! assert_eq!(cart.total(), 900);
//! ```

pub mod cart;
pub mod money;

pub use cart::{Cart, Item};

/// Format cents as a price.
///
/// ```
/// assert_eq!(shop_core::price(1999), "19.99 EUR");
/// ```
pub fn price(cents: i64) -> String {
    money::format_cents(cents)
}
