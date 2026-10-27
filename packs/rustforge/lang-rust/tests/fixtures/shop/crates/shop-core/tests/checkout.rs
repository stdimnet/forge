use shop_core::{Cart, Item};

#[test]
fn checkout_works() {
    let mut cart = Cart::default();
    cart.add(Item::new("Coffee", 450, 2));
    assert_eq!(shop_core::price(cart.total()), "9.00 EUR");
}
