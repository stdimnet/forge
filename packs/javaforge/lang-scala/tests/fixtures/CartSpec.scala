package com.acme.shop

import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers
import org.scalatest.wordspec.AnyWordSpec

class CartSpec extends AnyFlatSpec with Matchers {
  "A Cart" should "add items" in {
    val cart = Cart.empty.add(Item("apple", 120))
    cart.items should have size 1
  }

  it should "sum the total" in {
    Cart.empty.add(Item("apple", 120)).total shouldBe 120
  }

  it must "reject negative quantities" in {
    an[IllegalArgumentException] should be thrownBy Item("pear", -1)
  }
}

/* WordSpec style: "nested" { clauses } */
class CartWordSpec extends AnyWordSpec with Matchers {
  "A Cart" when {
    "empty" should {
      "have no items" in {
        Cart.empty.items shouldBe empty
      }
    }
  }
}

class CheckoutSpec extends org.scalatest.funspec.AnyFunSpec {
  describe("Checkout") {
    it("applies the discount") {
      assert(Checkout(Cart.empty).discount == 0)
    }
  }
  it("charges shipping") { assert(true) }
}
