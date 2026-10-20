/**
 * A small Symfony 7 + Doctrine demo project for browser mode, opened with `?project=symfony`
 * (the default demo is the Laravel shop in demoProject.ts).
 */
import type { ProjectInfo } from "@/ipc/project";

export const symfonyDemoName = "symfony-api";

/** True when the page asks for the Symfony demo project. */
export const isSymfonyDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "symfony";

export const symfonyDemoInfo: ProjectInfo = {
  languages: ["php", "javascript"],
  frameworks: [
    { id: "symfony", name: "Symfony", version: "7.2.3", language: "php" },
    { id: "doctrine", name: "Doctrine", version: "3.3.2", language: "php" },
    { id: "phpunit", name: "PHPUnit", version: "11.5.3", language: "php" },
  ],
  packs: ["phpforge"],
  tools: ["git", "composer", "docker"],
  primary: { id: "symfony", name: "Symfony", version: "7.2.3", language: "php" },
};

export const symfonyDemoFiles: Record<string, string> = {
  "composer.json": `{
    "name": "acme/symfony-api",
    "type": "project",
    "require": {
        "php": ">=8.3",
        "doctrine/doctrine-bundle": "^2.13",
        "doctrine/orm": "^3.3",
        "symfony/console": "7.2.*",
        "symfony/framework-bundle": "7.2.*",
        "symfony/twig-bundle": "7.2.*",
        "symfony/security-bundle": "7.2.*"
    },
    "autoload": { "psr-4": { "App\\\\": "src/" } }
}
`,
  ".env": "APP_ENV=dev\nAPP_SECRET=change-me\nDATABASE_URL=\"postgresql://app:app@127.0.0.1:5432/shop?serverVersion=16\"\n",
  "bin/console": "#!/usr/bin/env php\n<?php\n\nuse App\\Kernel;\nuse Symfony\\Bundle\\FrameworkBundle\\Console\\Application;\n\nrequire_once dirname(__DIR__).'/vendor/autoload_runtime.php';\n\nreturn function (array $context) {\n    return new Application(new Kernel($context['APP_ENV'], (bool) $context['APP_DEBUG']));\n};\n",
  "public/index.php": "<?php\n\nuse App\\Kernel;\n\nrequire_once dirname(__DIR__).'/vendor/autoload_runtime.php';\n\nreturn fn (array $context) => new Kernel($context['APP_ENV'], (bool) $context['APP_DEBUG']);\n",
  "config/bundles.php": `<?php

return [
    Symfony\\Bundle\\FrameworkBundle\\FrameworkBundle::class => ['all' => true],
    Doctrine\\Bundle\\DoctrineBundle\\DoctrineBundle::class => ['all' => true],
    Symfony\\Bundle\\TwigBundle\\TwigBundle::class => ['all' => true],
    Symfony\\Bundle\\SecurityBundle\\SecurityBundle::class => ['all' => true],
];
`,
  "config/services.yaml": `# Services of the application. Autowiring registers every class in src/.
parameters:
    app.currency: 'EUR'
    app.supported_locales: ['en', 'de', 'fr']
    app.invoice_dir: '%kernel.project_dir%/var/invoices'

services:
    _defaults:
        autowire: true
        autoconfigure: true

    App\\:
        resource: '../src/'
        exclude:
            - '../src/DependencyInjection/'
            - '../src/Entity/'
            - '../src/Kernel.php'

    App\\Service\\PaymentGateway:
        arguments:
            $apiKey: '%env(PAYMENT_API_KEY)%'
            $currency: '%app.currency%'

    app.invoice_mailer:
        class: App\\Service\\InvoiceMailer
        arguments: ['@mailer.mailer', '%app.invoice_dir%']
        public: true
`,
  "config/routes.yaml": `controllers:
    resource:
        path: ../src/Controller/
        namespace: App\\Controller
    type: attribute

health:
    path: /health
    controller: App\\Controller\\HealthController::check
    methods: [GET]
`,
  "config/packages/twig.yaml": "twig:\n    default_path: '%kernel.project_dir%/templates'\n    form_themes: ['bootstrap_5_layout.html.twig']\n",
  "config/packages/doctrine.yaml": `doctrine:
    dbal:
        url: '%env(resolve:DATABASE_URL)%'
    orm:
        auto_generate_proxy_classes: true
        naming_strategy: doctrine.orm.naming_strategy.underscore_number_aware
        mappings:
            App:
                type: attribute
                dir: '%kernel.project_dir%/src/Entity'
                prefix: 'App\\Entity'
`,
  "src/Kernel.php": `<?php

namespace App;

use Symfony\\Bundle\\FrameworkBundle\\Kernel\\MicroKernelTrait;
use Symfony\\Component\\HttpKernel\\Kernel as BaseKernel;

class Kernel extends BaseKernel
{
    use MicroKernelTrait;
}
`,
  "src/Controller/OrderController.php": `<?php

namespace App\\Controller;

use App\\Entity\\Order;
use App\\Repository\\OrderRepository;
use App\\Service\\Checkout;
use Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;
use Symfony\\Component\\HttpFoundation\\Response;
use Symfony\\Component\\Routing\\Attribute\\Route;

#[Route('/orders', name: 'order_')]
final class OrderController extends AbstractController
{
    #[Route('', name: 'index', methods: ['GET'])]
    public function index(OrderRepository $orders): Response
    {
        return $this->render('order/index.html.twig', [
            'orders' => $orders->findRecentlyPaid(),
        ]);
    }

    #[Route('/{id}', name: 'show', methods: ['GET'], requirements: ['id' => '\\d+'])]
    public function show(Order $order): Response
    {
        return $this->render('order/show.html.twig', ['order' => $order]);
    }

    #[Route('/{id}/pay', name: 'pay', methods: ['POST'])]
    public function pay(Order $order, Checkout $checkout): Response
    {
        $checkout->pay($order);
        $this->addFlash('success', 'Payment received.');

        return $this->redirectToRoute('order_show', ['id' => $order->getId()]);
    }
}
`,
  "src/Controller/ProductController.php": `<?php

namespace App\\Controller;

use App\\Repository\\ProductRepository;
use Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;
use Symfony\\Component\\HttpFoundation\\Response;
use Symfony\\Component\\Routing\\Attribute\\Route;

final class ProductController extends AbstractController
{
    #[Route('/products', name: 'product_list', methods: ['GET'])]
    public function list(ProductRepository $products): Response
    {
        return $this->render('product/list.html.twig', [
            'products' => $products->findAvailable(),
        ]);
    }

    #[Route('/products/{slug}', name: 'product_show', methods: ['GET'])]
    public function show(string $slug, ProductRepository $products): Response
    {
        return $this->render('product/show.html.twig', [
            'product' => $products->findOneBy(['slug' => $slug]),
        ]);
    }
}
`,
  "src/Controller/HealthController.php": `<?php

namespace App\\Controller;

use Symfony\\Component\\HttpFoundation\\JsonResponse;

final class HealthController
{
    public function check(): JsonResponse
    {
        return new JsonResponse(['status' => 'ok']);
    }
}
`,
  "src/Controller/Admin/DashboardController.php": `<?php

namespace App\\Controller\\Admin;

use Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;
use Symfony\\Component\\HttpFoundation\\Response;
use Symfony\\Component\\Routing\\Attribute\\Route;
use Symfony\\Component\\Security\\Http\\Attribute\\IsGranted;

#[Route('/admin')]
#[IsGranted('ROLE_ADMIN')]
final class DashboardController extends AbstractController
{
    #[Route('', name: 'admin_dashboard')]
    public function index(): Response
    {
        return $this->render('admin/dashboard.html.twig');
    }
}
`,
  "src/Entity/Order.php": `<?php

namespace App\\Entity;

use App\\Repository\\OrderRepository;
use Doctrine\\Common\\Collections\\ArrayCollection;
use Doctrine\\Common\\Collections\\Collection;
use Doctrine\\DBAL\\Types\\Types;
use Doctrine\\ORM\\Mapping as ORM;

#[ORM\\Entity(repositoryClass: OrderRepository::class)]
#[ORM\\Table(name: 'orders')]
class Order
{
    #[ORM\\Id]
    #[ORM\\GeneratedValue]
    #[ORM\\Column]
    private ?int $id = null;

    #[ORM\\Column(length: 32)]
    private string $status = 'new';

    #[ORM\\Column(type: Types::DECIMAL, precision: 10, scale: 2)]
    private string $total = '0.00';

    #[ORM\\Column]
    private \\DateTimeImmutable $createdAt;

    #[ORM\\Column(nullable: true)]
    private ?\\DateTimeImmutable $paidAt = null;

    #[ORM\\ManyToOne(inversedBy: 'orders')]
    #[ORM\\JoinColumn(nullable: false)]
    private ?Customer $customer = null;

    /** @var Collection<int, OrderItem> */
    #[ORM\\OneToMany(targetEntity: OrderItem::class, mappedBy: 'order', orphanRemoval: true)]
    private Collection $items;

    #[ORM\\Embedded(class: Address::class)]
    private Address $shippingAddress;

    public function __construct()
    {
        $this->items = new ArrayCollection();
        $this->createdAt = new \\DateTimeImmutable();
    }

    public function getId(): ?int
    {
        return $this->id;
    }
}
`,
  "src/Entity/Customer.php": `<?php

namespace App\\Entity;

use App\\Repository\\CustomerRepository;
use Doctrine\\Common\\Collections\\Collection;
use Doctrine\\ORM\\Mapping as ORM;

#[ORM\\Entity(repositoryClass: CustomerRepository::class)]
#[ORM\\Table(name: 'customers')]
class Customer
{
    #[ORM\\Id]
    #[ORM\\GeneratedValue]
    #[ORM\\Column]
    private ?int $id = null;

    #[ORM\\Column(length: 180, unique: true)]
    private string $email;

    #[ORM\\Column(length: 120)]
    private string $name;

    /** @var Collection<int, Order> */
    #[ORM\\OneToMany(targetEntity: Order::class, mappedBy: 'customer')]
    private Collection $orders;
}
`,
  "src/Entity/OrderItem.php": `<?php

namespace App\\Entity;

use Doctrine\\ORM\\Mapping as ORM;

#[ORM\\Entity]
#[ORM\\Table(name: 'order_items')]
class OrderItem
{
    #[ORM\\Id]
    #[ORM\\GeneratedValue]
    #[ORM\\Column]
    private ?int $id = null;

    #[ORM\\Column]
    private int $quantity = 1;

    #[ORM\\Column(type: 'decimal', precision: 10, scale: 2)]
    private string $price;

    #[ORM\\ManyToOne(inversedBy: 'items')]
    #[ORM\\JoinColumn(nullable: false)]
    private ?Order $order = null;

    #[ORM\\ManyToOne]
    private ?Product $product = null;
}
`,
  "src/Entity/Product.php": `<?php

namespace App\\Entity;

use App\\Repository\\ProductRepository;
use Doctrine\\ORM\\Mapping as ORM;

/**
 * Legacy annotation mapping, still supported next to attributes.
 *
 * @ORM\\Entity(repositoryClass="App\\Repository\\ProductRepository")
 * @ORM\\Table(name="products")
 */
class Product
{
    /**
     * @ORM\\Id
     * @ORM\\GeneratedValue
     * @ORM\\Column(type="integer")
     */
    private $id;

    /** @ORM\\Column(type="string", length=255) */
    private $name;

    /** @ORM\\Column(type="string", length=255, unique=true) */
    private $slug;

    /** @ORM\\Column(type="decimal", precision=10, scale=2) */
    private $price;

    /** @ORM\\Column(type="boolean") */
    private $available = true;
}
`,
  "src/Entity/Address.php": `<?php

namespace App\\Entity;

use Doctrine\\ORM\\Mapping as ORM;

#[ORM\\Embeddable]
class Address
{
    #[ORM\\Column(length: 120)]
    private string $street = '';

    #[ORM\\Column(length: 80)]
    private string $city = '';

    #[ORM\\Column(length: 2)]
    private string $country = 'DE';
}
`,
  "src/Repository/OrderRepository.php": `<?php

namespace App\\Repository;

use App\\Entity\\Order;
use Doctrine\\Bundle\\DoctrineBundle\\Repository\\ServiceEntityRepository;
use Doctrine\\Persistence\\ManagerRegistry;

/**
 * @extends ServiceEntityRepository<Order>
 */
class OrderRepository extends ServiceEntityRepository
{
    public function __construct(ManagerRegistry $registry)
    {
        parent::__construct($registry, Order::class);
    }

    /** @return list<Order> */
    public function findRecentlyPaid(int $limit = 20): array
    {
        return $this->createQueryBuilder('o')
            ->join('o.customer', 'c')
            ->andWhere('o.status = :status')
            ->setParameter('status', 'paid')
            ->orderBy('o.paidAt', 'DESC')
            ->setMaxResults($limit)
            ->getQuery()
            ->getResult();
    }

    /** @return array<string, string> revenue per customer e-mail */
    public function revenueByCustomer(\\DateTimeImmutable $since): array
    {
        return $this->getEntityManager()->createQuery(
            'SELECT c.email, SUM(o.total) AS revenue
             FROM App\\Entity\\Order o
             JOIN o.customer c
             WHERE o.paidAt >= :since AND o.status = 'paid'
             GROUP BY c.email
             ORDER BY revenue DESC'
        )->setParameter('since', $since)->getResult();
    }
}
`,
  "src/Repository/CustomerRepository.php": `<?php

namespace App\\Repository;

use App\\Entity\\Customer;
use Doctrine\\Bundle\\DoctrineBundle\\Repository\\ServiceEntityRepository;
use Doctrine\\Persistence\\ManagerRegistry;

/** @extends ServiceEntityRepository<Customer> */
class CustomerRepository extends ServiceEntityRepository
{
    public function __construct(ManagerRegistry $registry)
    {
        parent::__construct($registry, Customer::class);
    }
}
`,
  "src/Repository/ProductRepository.php": `<?php

namespace App\\Repository;

use App\\Entity\\Product;
use Doctrine\\Bundle\\DoctrineBundle\\Repository\\ServiceEntityRepository;
use Doctrine\\Persistence\\ManagerRegistry;

/** @extends ServiceEntityRepository<Product> */
class ProductRepository extends ServiceEntityRepository
{
    public function __construct(ManagerRegistry $registry)
    {
        parent::__construct($registry, Product::class);
    }

    /** @return list<Product> */
    public function findAvailable(): array
    {
        return $this->createQueryBuilder('p')
            ->where('p.available = true')
            ->orderBy('p.name', 'ASC')
            ->getQuery()
            ->getResult();
    }
}
`,
  "src/Service/Checkout.php": `<?php

namespace App\\Service;

use App\\Entity\\Order;
use Symfony\\Component\\DependencyInjection\\Attribute\\Autowire;

final class Checkout
{
    public function __construct(
        private readonly PaymentGateway $gateway,
        #[Autowire(service: 'app.invoice_mailer')]
        private readonly InvoiceMailer $mailer,
        #[Autowire('%app.currency%')]
        private readonly string $currency,
    ) {
    }

    public function pay(Order $order): void
    {
        $this->gateway->charge($order, $this->currency);
        $this->mailer->send($order);
    }
}
`,
  "src/Service/PaymentGateway.php": `<?php

namespace App\\Service;

use App\\Entity\\Order;

class PaymentGateway
{
    public function __construct(private string $apiKey, private string $currency)
    {
    }

    public function charge(Order $order, string $currency): void
    {
    }
}
`,
  "src/Service/InvoiceMailer.php": `<?php

namespace App\\Service;

use App\\Entity\\Order;
use Symfony\\Component\\Mailer\\MailerInterface;

class InvoiceMailer
{
    public function __construct(private MailerInterface $mailer, private string $invoiceDir)
    {
    }

    public function send(Order $order): void
    {
    }
}
`,
  "src/Command/PurgeOrdersCommand.php": `<?php

namespace App\\Command;

use Symfony\\Component\\Console\\Attribute\\AsCommand;
use Symfony\\Component\\Console\\Command\\Command;
use Symfony\\Component\\Console\\Input\\InputInterface;
use Symfony\\Component\\Console\\Output\\OutputInterface;

#[AsCommand(name: 'app:orders:purge', description: 'Delete abandoned orders older than 30 days')]
final class PurgeOrdersCommand extends Command
{
    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $output->writeln('Purged 12 abandoned orders.');

        return Command::SUCCESS;
    }
}
`,
  "src/Command/ImportProductsCommand.php": `<?php

namespace App\\Command;

use Symfony\\Component\\Console\\Attribute\\AsCommand;
use Symfony\\Component\\Console\\Command\\Command;

#[AsCommand(name: 'app:products:import', description: 'Import products from a CSV file', aliases: ['app:import'])]
final class ImportProductsCommand extends Command
{
}
`,
  "src/Twig/AppExtension.php": `<?php

namespace App\\Twig;

use Twig\\Extension\\AbstractExtension;
use Twig\\TwigFilter;
use Twig\\TwigFunction;

final class AppExtension extends AbstractExtension
{
    public function getFilters(): array
    {
        return [
            new TwigFilter('price', [$this, 'formatPrice']),
        ];
    }

    public function getFunctions(): array
    {
        return [
            new TwigFunction('cart_count', [$this, 'cartCount']),
        ];
    }

    public function formatPrice(string $amount, string $currency = 'EUR'): string
    {
        return number_format((float) $amount, 2, ',', '.').' '.$currency;
    }
}
`,
  "templates/base.html.twig": `<!DOCTYPE html>
<html lang="{{ app.request.locale }}">
<head>
    <meta charset="UTF-8">
    <title>{% block title %}Acme Shop{% endblock %}</title>
    {% block stylesheets %}{% endblock %}
</head>
<body>
    <nav>
        <a href="{{ path('product_list') }}">Products</a>
        <a href="{{ path('order_index') }}">Orders ({{ cart_count() }})</a>
    </nav>
    {% for message in app.flashes('success') %}
        <div class="flash">{{ message }}</div>
    {% endfor %}
    {% block body %}{% endblock %}
</body>
</html>
`,
  "templates/order/index.html.twig": `{% extends 'base.html.twig' %}

{% block title %}Orders{% endblock %}

{% block body %}
    <h1>Recently paid orders</h1>
    <table>
        {% for order in orders %}
            <tr>
                <td><a href="{{ path('order_show', {id: order.id}) }}">#{{ order.id }}</a></td>
                <td>{{ order.customer.name }}</td>
                <td>{{ order.paidAt|date('d.m.Y') }}</td>
                <td>{{ order.total|price }}</td>
            </tr>
        {% else %}
            <tr><td colspan="4">No orders yet.</td></tr>
        {% endfor %}
    </table>
{% endblock %}
`,
  "templates/order/show.html.twig": `{% extends 'base.html.twig' %}

{% block title %}Order #{{ order.id }}{% endblock %}

{% block body %}
    <h1>Order #{{ order.id }}</h1>
    <p>Status: {{ order.status|capitalize }}</p>
    <ul>
        {% for item in order.items %}
            <li>{{ item.quantity }} × {{ item.product.name }} — {{ item.price|price }}</li>
        {% endfor %}
    </ul>
    <p class="total">{{ order.total|price }}</p>
    {% if order.status == 'new' %}
        <form method="post" action="{{ path('order_pay', {id: order.id}) }}">
            <button>Pay now</button>
        </form>
    {% endif %}
    <a href="{{ path('order_index') }}">Back to orders</a>
{% endblock %}
`,
  "templates/product/list.html.twig": `{% extends 'base.html.twig' %}

{% block body %}
    {% for product in products %}
        <a href="{{ path('product_show', {slug: product.slug}) }}">{{ product.name|upper }}</a>
    {% endfor %}
{% endblock %}
`,
  "templates/product/show.html.twig": "{% extends 'base.html.twig' %}\n\n{% block body %}\n    <h1>{{ product.name }}</h1>\n    <p>{{ product.price|price }}</p>\n{% endblock %}\n",
  "templates/admin/dashboard.html.twig": "{% extends 'base.html.twig' %}\n\n{% block body %}\n    <h1>Dashboard</h1>\n{% endblock %}\n",
  "tests/Controller/OrderControllerTest.php": `<?php

namespace App\\Tests\\Controller;

use Symfony\\Bundle\\FrameworkBundle\\Test\\WebTestCase;

final class OrderControllerTest extends WebTestCase
{
    public function testIndex(): void
    {
        $client = static::createClient();
        $client->request('GET', '/orders');

        self::assertResponseIsSuccessful();
    }
}
`,
  "phpunit.dist.xml": '<?xml version="1.0" encoding="UTF-8"?>\n<phpunit bootstrap="tests/bootstrap.php" colors="true">\n    <testsuites>\n        <testsuite name="Project Test Suite">\n            <directory>tests</directory>\n        </testsuite>\n    </testsuites>\n</phpunit>\n',
};
