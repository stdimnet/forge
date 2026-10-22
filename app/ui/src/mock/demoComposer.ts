/**
 * Composer demo data for browser mode: a small catalog standing in for Packagist and the
 * demo project's composer.lock (laravel/framework and phpunit/phpunit have updates,
 * guzzlehttp/psr7 has a security advisory).
 */

export interface DemoPackage {
  name: string;
  description: string;
  homepage?: string;
  repository?: string;
  abandoned?: string;
  versions: string[];
  /** What Packagist's statistics would say. */
  downloads: number;
  favers: number;
}

const gh = (repo: string) => `https://github.com/${repo}`;

/** Demo stand-in for Packagist (newest versions first). */
export const DEMO_CATALOG: DemoPackage[] = [
  {
    name: "laravel/framework",
    description: "The Laravel Framework.",
    homepage: "https://laravel.com",
    repository: gh("laravel/framework"),
    versions: ["v12.3.0", "v12.2.0", "v12.1.1", "v12.1.0", "v12.0.1", "v12.0.0", "v11.44.2", "v11.44.0", "v10.48.29"],
    downloads: 431_208_771,
    favers: 33_412,
  },
  {
    name: "laravel/sanctum",
    description: "Laravel Sanctum provides a featherweight authentication system for SPAs and simple APIs.",
    repository: gh("laravel/sanctum"),
    versions: ["v4.0.8", "v4.0.7", "v4.0.0", "v3.3.3"],
    downloads: 121_004_211,
    favers: 2_812,
  },
  {
    name: "laravel/horizon",
    description: "Dashboard and code-driven configuration for Laravel queues.",
    homepage: "https://laravel.com/docs/horizon",
    repository: gh("laravel/horizon"),
    versions: ["v5.31.0", "v5.30.3", "v5.29.0"],
    downloads: 64_771_020,
    favers: 3_942,
  },
  {
    name: "laravel/telescope",
    description: "An elegant debug assistant for the Laravel framework.",
    repository: gh("laravel/telescope"),
    versions: ["v5.5.0", "v5.4.0", "v4.17.6"],
    downloads: 41_220_118,
    favers: 4_955,
  },
  {
    name: "laravel/pint",
    description: "An opinionated code formatter for PHP.",
    homepage: "https://laravel.com",
    repository: gh("laravel/pint"),
    versions: ["v1.21.2", "v1.21.0", "v1.20.0"],
    downloads: 72_302_400,
    favers: 2_901,
  },
  {
    name: "doctrine/dbal",
    description: "Powerful PHP database abstraction layer (DBAL) with many features for database schema introspection and management.",
    homepage: "https://www.doctrine-project.org/projects/dbal.html",
    repository: gh("doctrine/dbal"),
    versions: ["4.2.3", "4.2.2", "4.2.1", "4.1.1", "4.0.4", "3.9.4", "3.9.3"],
    downloads: 389_120_440,
    favers: 9_581,
  },
  {
    name: "doctrine/orm",
    description: "Object-Relational-Mapper for PHP",
    homepage: "https://www.doctrine-project.org/projects/orm.html",
    repository: gh("doctrine/orm"),
    versions: ["3.3.2", "3.3.1", "3.2.2", "2.20.2"],
    downloads: 268_400_131,
    favers: 10_012,
  },
  {
    name: "guzzlehttp/guzzle",
    description: "Guzzle is a PHP HTTP client library",
    repository: gh("guzzle/guzzle"),
    versions: ["7.9.3", "7.9.2", "7.9.1", "7.8.2", "6.5.8"],
    downloads: 812_445_300,
    favers: 23_288,
  },
  {
    name: "guzzlehttp/psr7",
    description: "PSR-7 message implementation that also provides common utility methods",
    repository: gh("guzzle/psr7"),
    versions: ["2.7.1", "2.7.0", "2.4.5", "2.4.4", "1.9.1"],
    downloads: 790_005_002,
    favers: 7_891,
  },
  {
    name: "phpunit/phpunit",
    description: "The PHP Unit Testing framework.",
    homepage: "https://phpunit.de/",
    repository: gh("sebastianbergmann/phpunit"),
    versions: ["12.0.2", "12.0.0", "11.5.12", "11.5.2", "11.4.0", "10.5.45"],
    downloads: 870_331_092,
    favers: 19_870,
  },
  {
    name: "pestphp/pest",
    description: "The elegant PHP Testing Framework.",
    homepage: "https://pestphp.com",
    repository: gh("pestphp/pest"),
    versions: ["v3.7.4", "v3.7.0", "v2.36.0"],
    downloads: 39_120_771,
    favers: 10_120,
  },
  {
    name: "spatie/laravel-permission",
    description: "Permission handling for Laravel 8.0 and up",
    homepage: "https://github.com/spatie/laravel-permission",
    repository: gh("spatie/laravel-permission"),
    versions: ["6.16.0", "6.15.0", "5.11.1"],
    downloads: 70_003_455,
    favers: 12_380,
  },
  {
    name: "monolog/monolog",
    description: "Sends your logs to files, sockets, inboxes, databases and various web services",
    homepage: "https://github.com/Seldaek/monolog",
    repository: gh("Seldaek/monolog"),
    versions: ["3.9.0", "3.8.1", "2.10.0"],
    downloads: 901_221_000,
    favers: 21_090,
  },
  {
    name: "nesbot/carbon",
    description: "An API extension for DateTime that supports 281 different languages.",
    homepage: "https://carbon.nesbot.com",
    repository: gh("briannesbitt/Carbon"),
    versions: ["3.8.6", "3.8.4", "2.73.0"],
    downloads: 560_404_118,
    favers: 16_544,
  },
  {
    name: "swiftmailer/swiftmailer",
    description: "Swiftmailer, free feature-rich PHP mailer",
    repository: gh("swiftmailer/swiftmailer"),
    abandoned: "symfony/mailer",
    versions: ["v6.3.0", "v6.2.7"],
    downloads: 223_004_001,
    favers: 9_510,
  },
];

/** The demo's composer.lock: laravel/framework and phpunit/phpunit have updates. */
const lock = (pkgs: [name: string, version: string][]) =>
  pkgs.map(([name, version]) => {
    const info = DEMO_CATALOG.find((p) => p.name === name);
    return {
      name,
      version,
      source: { type: "git", url: `${info?.repository ?? gh(name)}.git`, reference: "4f1c2d8e5b7a" },
      description: info?.description ?? "",
      ...(info?.homepage ? { homepage: info.homepage } : {}),
      type: "library",
    };
  });

export const DEMO_LOCK = `${JSON.stringify(
  {
    "_readme": ["This file locks the dependencies of your project to a known state", "This file is @generated automatically"],
    "content-hash": "8c0e4b7d6a2f41c3b5e9d1f07a6c2b3e",
    packages: lock([
      ["doctrine/dbal", "4.2.3"],
      ["guzzlehttp/guzzle", "7.9.3"],
      ["guzzlehttp/psr7", "2.4.4"],
      ["laravel/framework", "v12.1.0"],
      ["monolog/monolog", "3.9.0"],
      ["nesbot/carbon", "3.8.6"],
    ]),
    "packages-dev": lock([["phpunit/phpunit", "11.4.0"]]),
    "minimum-stability": "stable",
    "prefer-stable": true,
  },
  null,
  4,
)}\n`;

