package com.acme.shop

import munit.FunSuite

class PriceSuite extends FunSuite {
  test("formats euros") {
    assertEquals(Price(1250).show, "12.50 EUR")
  }

  test("rounds half up") {
    assertEquals(Price.of(BigDecimal("0.125")).cents, 13L)
  }
}
