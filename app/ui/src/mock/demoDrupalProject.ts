/**
 * A small Drupal module for browser mode, opened with `?project=drupal`: "Acme Tools" with a
 * few hook implementations, a route and a Twig template. Written for Forge's demo (no
 * Drupal core code), so Go to the hook docs falls back to api.drupal.org.
 */
import type { ProjectInfo } from "@/ipc/project";

export const drupalDemoName = "acme_tools";

export const isDrupalDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "drupal";

export const drupalDemoInfo: ProjectInfo = {
  languages: ["php"],
  frameworks: [{ id: "drupal", name: "Drupal", version: "11.0.5", language: "php" }],
  packs: ["phpforge"],
  tools: ["git", "composer"],
  primary: { id: "drupal", name: "Drupal", version: "11.0.5", language: "php" },
};

export const drupalDemoFiles: Record<string, string> = {
  "acme_tools.info.yml": `name: Acme Tools
type: module
description: 'Small helpers for the Acme site.'
core_version_requirement: ^10.3 || ^11
package: Acme
`,
  "acme_tools.module": `<?php

/**
 * @file
 * Hook implementations of Acme Tools.
 */

use Drupal\\Core\\Form\\FormStateInterface;
use Drupal\\Core\\Routing\\RouteMatchInterface;

/**
 * Implements hook_help().
 */
function acme_tools_help($route_name, RouteMatchInterface $route_match) {
  if ($route_name === 'help.page.acme_tools') {
    return '<p>' . t('Small helpers for the Acme site.') . '</p>';
  }
}

/**
 * Implements hook_form_FORM_ID_alter().
 */
function acme_tools_form_user_login_form_alter(&$form, FormStateInterface $form_state, $form_id) {
  $form['name']['#description'] = t('Your Acme account name.');
}

/**
 * Implements hook_theme().
 */
function acme_tools_theme($existing, $type, $theme, $path) {
  return [
    'acme_tools_block' => [
      'variables' => ['items' => []],
    ],
  ];
}

`,
  "acme_tools.install": `<?php

/**
 * @file
 * Install and update hooks of Acme Tools.
 */

/**
 * Implements hook_install().
 */
function acme_tools_install($is_syncing) {
  \\Drupal::state()->set('acme_tools.installed', \\Drupal::time()->getRequestTime());
}
`,
  "acme_tools.routing.yml": `acme_tools.overview:
  path: '/admin/acme'
  defaults:
    _controller: '\\Drupal\\acme_tools\\Controller\\OverviewController::build'
    _title: 'Acme overview'
  requirements:
    _permission: 'administer site configuration'
`,
  "acme_tools.services.yml": `services:
  acme_tools.greeter:
    class: Drupal\\acme_tools\\Greeter
    arguments: ['@current_user']
`,
  "src/Controller/OverviewController.php": `<?php

namespace Drupal\\acme_tools\\Controller;

use Drupal\\Core\\Controller\\ControllerBase;

final class OverviewController extends ControllerBase {

  public function build(): array {
    return ['#theme' => 'acme_tools_block', '#items' => ['One', 'Two']];
  }

}
`,
  "templates/acme-tools-block.html.twig": `{#
/**
 * @file
 * The Acme tools block.
 */
#}
<ul class="acme-tools">
  {% for item in items %}
    <li>{{ item|upper }}</li>
  {% endfor %}
</ul>
`,
  "composer.json": `{
    "name": "acme/acme_tools",
    "type": "drupal-module",
    "require": {
        "drupal/core": "^10.3 || ^11"
    }
}
`,
};
