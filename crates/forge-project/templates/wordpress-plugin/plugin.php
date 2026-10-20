<?php
/**
 * Plugin Name:       {{title}}
 * Description:       A WordPress plugin created with Forge.
 * Version:           0.1.0
 * Requires at least: 6.4
 * Requires PHP:      {{php_version}}
 * License:           GPL-2.0-or-later
 * License URI:       https://www.gnu.org/licenses/gpl-2.0.html
 * Text Domain:       {{package}}
 */

if (!defined('ABSPATH')) {
    exit;
}

define('{{constant}}_VERSION', '0.1.0');
define('{{constant}}_PATH', plugin_dir_path(__FILE__));

require_once {{constant}}_PATH . 'includes/class-{{package}}.php';

add_action('plugins_loaded', ['{{class}}', 'init']);
