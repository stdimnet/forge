/**
 * Demo files of the PHP test frameworks in the Laravel demo: a Pest test with describe
 * blocks and a dataset, a Behat feature with its context, a PHPSpec spec and a Codeception
 * Cest, plus their configuration files (Settings | PHP | Test Frameworks detects them).
 */
export const demoTestFrameworkFiles: Record<string, string> = {
  "tests/Feature/CartTest.php": `<?php

use App\\Models\\User;
use App\\Services\\Checkout;

beforeEach(function () {
    $this->checkout = new Checkout();
});

test('an empty cart costs nothing', function () {
    expect($this->checkout->total(0.0, 0.19))->toBe(0.0);
});

describe('totals', function () {
    it('adds the tax', function (float $net, float $gross) {
        expect($this->checkout->total($net, 0.19))->toBe($gross);
    })->with([
        'ten euros' => [10.0, 11.9],
        'a hundred euros' => [100.0, 119.0],
    ]);

    it('rounds to cents', function () {
        expect($this->checkout->total(0.99, 0.19))->toBe(1.18);
    });
});

it('belongs to a user', function () {
    expect(new User())->orders->toBeEmpty();
});
`,
  "behat.yml": `default:
  suites:
    default:
      contexts:
        - FeatureContext
`,
  "features/checkout.feature": `@checkout
Feature: Checkout
  In order to buy products
  As a customer
  I need to be able to put products into my basket

  Background:
    Given there is a "Lightsaber", which costs £5

  Scenario: Buying a single product
    When I add the "Lightsaber" to the basket
    Then I should have 1 product in the basket
    And the overall basket price should be £9

  Scenario Outline: Buying several products
    When I add <count> products to the basket
    Then I should have <count> products in the basket

    Examples:
      | count |
      | 2     |
      | 3     |
`,
  "features/bootstrap/FeatureContext.php": `<?php

use App\\Services\\Checkout;
use Behat\\Behat\\Context\\Context;
use Behat\\Step\\Then;
use PHPUnit\\Framework\\Assert;

class FeatureContext implements Context
{
    private array $basket = [];
    private array $prices = [];

    /**
     * @Given there is a(n) :product, which costs £:price
     */
    public function thereIsAWhichCostsPs(string $product, float $price): void
    {
        $this->prices[$product] = $price;
    }

    /**
     * @When /^I add the "([^"]*)" to the basket$/
     */
    public function iAddTheToTheBasket(string $product): void
    {
        $this->basket[] = $product;
    }

    /**
     * @When I add :count products to the basket
     */
    public function iAddProductsToTheBasket(int $count): void
    {
        $this->basket = array_merge($this->basket, array_fill(0, $count, 'Lightsaber'));
    }

    #[Then('I should have :count product(s) in the basket')]
    public function iShouldHaveProductsInTheBasket(int $count): void
    {
        Assert::assertCount($count, $this->basket);
    }

    #[Then('the overall basket price should be £:price')]
    public function theOverallBasketPriceShouldBePs(float $price): void
    {
        $net = array_sum(array_map(fn ($p) => $this->prices[$p], $this->basket));
        Assert::assertSame($price, (new Checkout())->total($net, 0.19));
    }
}
`,
  "phpspec.yml": `suites:
  app_suite:
    namespace: App
    psr4_prefix: App
    src_path: app
`,
  "spec/App/Services/CheckoutSpec.php": `<?php

namespace spec\\App\\Services;

use App\\Services\\Checkout;
use PhpSpec\\ObjectBehavior;

class CheckoutSpec extends ObjectBehavior
{
    function it_is_initializable()
    {
        $this->shouldHaveType(Checkout::class);
    }

    function it_adds_the_tax_to_the_total()
    {
        $this->total(100.0, 0.19)->shouldReturn(119.0);
    }

    function it_rejects_negative_amounts()
    {
        $this->shouldThrow(\\InvalidArgumentException::class)->during('total', [-1.0, 0.19]);
    }
}
`,
  "codeception.yml": `namespace: Tests
support_namespace: Support
paths:
  tests: tests
  output: tests/_output
  data: tests/Support/Data
  support: tests/Support
`,
  "tests/Acceptance/LoginCest.php": `<?php

namespace Tests\\Acceptance;

use Tests\\Support\\AcceptanceTester;

class LoginCest
{
    public function _before(AcceptanceTester $I): void
    {
        $I->amOnPage('/login');
    }

    public function loginWithValidCredentials(AcceptanceTester $I): void
    {
        $I->fillField('email', 'grace@example.com');
        $I->fillField('password', 'secret');
        $I->click('Log in');
        $I->see('Your orders');
    }

    public function loginWithWrongPassword(AcceptanceTester $I): void
    {
        $I->fillField('email', 'grace@example.com');
        $I->fillField('password', 'nope');
        $I->click('Log in');
        $I->see('These credentials do not match our records.');
    }
}
`,
};
