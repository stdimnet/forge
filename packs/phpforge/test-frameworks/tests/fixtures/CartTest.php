<?php

use App\Services\Checkout;

test('an empty cart costs nothing', function () {
    expect((new Checkout())->total(0.0, 0.19))->toBe(0.0);
});

describe('totals', function () {
    it('adds the tax', function (float $net, float $gross) {
        expect((new Checkout())->total($net, 0.19))->toBe($gross);
    })->with([
        'ten euros' => [10.0, 11.9],
    ]);

    it('rounds to cents', function () {
        expect((new Checkout())->total(0.99, 0.19))->toBe(1.18);
    });
});
