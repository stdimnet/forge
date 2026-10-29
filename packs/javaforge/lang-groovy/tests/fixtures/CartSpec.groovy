package com.acme.cart

import spock.lang.Ignore
import spock.lang.Specification
import spock.lang.Unroll

class CartSpec extends Specification {

    def cart = newCart()

    def setup() {
        cart.clear()
    }

    def "adds items"() {
        given: "an apple"
        def apple = new Item("apple", 120)

        when:
        cart.add(apple)

        then:
        cart.items == [apple]
    }

    @Unroll
    def "totals #items items to #total"() {
        expect:
        cart.fill(items).total() == total

        where:
        items | total
        1     | 120
        3     | 360
    }

    @Ignore("waiting for the stock service")
    def "removes items"() {
        expect:
        cart.remove("apple")
    }

    def rejectsNegativeQuantities() {
        when:
        cart.add(new Item("pear", -1))

        then:
        thrown(IllegalArgumentException)
    }

    private Cart newCart() {
        new Cart()
    }
}
