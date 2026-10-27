use std::collections::HashMap;

use shop_core::{Cart, Item};

fn main() -> anyhow::Result<()> {
    let customer = String::from("Anna Schmidt");
    let mut cart = Cart::default();
    cart.add(Item::new("Coffee", 450, 2));
    cart.add(Item::new("Croissant", 220, 3));
    cart.voucher = Some("AUTUMN10".to_string());
    let quantities: HashMap<String, u32> = cart.quantities();
    let total = cart.total();
    println!("{customer}: {} items, {}", quantities.len(), shop_core::price(total));
    Ok(())
}
