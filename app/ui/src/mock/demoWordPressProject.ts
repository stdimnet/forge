/**
 * A small WordPress plugin for browser mode, opened with `?project=wordpress`: "Acme Events"
 * registers an event post type, filters the content and fires hooks of its own. Written for
 * Forge's demo (no WordPress core code); `vendor/php-stubs/wordpress-stubs` holds a handful
 * of hand-written signatures so Go to Declaration on WordPress functions has a target.
 */
import type { ProjectInfo } from "@/ipc/project";

export const wordpressDemoName = "acme-events";

/** True when the page asks for the WordPress demo project. */
export const isWordPressDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "wordpress";

export const wordpressDemoInfo: ProjectInfo = {
  languages: ["php", "css"],
  frameworks: [{ id: "wordpress", name: "WordPress", language: "php" }],
  packs: ["phpforge"],
  tools: ["git", "composer"],
  primary: { id: "wordpress", name: "WordPress", language: "php" },
};

export const wordpressDemoFiles: Record<string, string> = {
  "acme-events.php": `<?php
/**
 * Plugin Name:       Acme Events
 * Description:       Events with dates and venues, listed on any page.
 * Version:           1.2.0
 * Requires at least: 6.4
 * Requires PHP:      8.1
 * Author:            Acme
 * License:           GPL-2.0-or-later
 * Text Domain:       acme-events
 */

defined( 'ABSPATH' ) || exit;

define( 'ACME_EVENTS_VERSION', '1.2.0' );

require_once __DIR__ . '/includes/class-acme-events.php';
require_once __DIR__ . '/includes/functions.php';

add_action( 'plugins_loaded', 'acme_events_boot' );
add_action( 'wp_enqueue_scripts', 'acme_events_assets' );

function acme_events_boot() {
	$plugin = new Acme_Events();
	$plugin->register();
	do_action( 'acme_events_loaded', $plugin );
}

function acme_events_assets() {
	wp_enqueue_style( 'acme-events', plugins_url( 'assets/css/acme-events.css', __FILE__ ), array(), ACME_EVENTS_VERSION );
}
`,
  "includes/class-acme-events.php": `<?php
/**
 * The event post type and its front-end output.
 */
class Acme_Events {

	public function register() {
		add_action( 'init', array( $this, 'register_post_type' ) );
		add_filter( 'the_content', array( $this, 'append_details' ), 20 );
		add_filter( 'acme_events_date_format', array( $this, 'short_dates' ) );
	}

	public function register_post_type() {
		$args = apply_filters(
			'acme_events_post_type_args',
			array(
				'label'        => __( 'Events', 'acme-events' ),
				'public'       => true,
				'show_in_rest' => true,
				'supports'     => array( 'title', 'editor', 'thumbnail' ),
			)
		);
		register_post_type( 'acme_event', $args );
		do_action( 'acme_events_registered', 'acme_event' );
	}

	public function append_details( $content ) {
		if ( get_post_type() !== 'acme_event' ) {
			return $content;
		}
		$format = apply_filters( 'acme_events_date_format', 'F j, Y' );
		$date   = get_post_meta( get_the_ID(), '_acme_event_date', true );
		$label  = apply_filters( "acme_events_label_{$date}", esc_html( date_i18n( $format, strtotime( $date ) ) ) );
		return $content . '<p class="acme-event-date">' . $label . '</p>';
	}

	public function short_dates( $format ) {
		return 'M j';
	}
}
`,
  "includes/functions.php": `<?php
/**
 * Template helpers and integrations.
 */

add_action( 'acme_events_registered', 'acme_events_flush_rules' );
add_action( 'save_post_acme_event', 'acme_events_save_meta', 10, 2 );
add_filter( 'acme_events_label_2026-12-24', 'acme_events_holiday_label' );

function acme_events_flush_rules( $post_type ) {
	if ( get_option( 'acme_events_flushed' ) === false ) {
		flush_rewrite_rules();
		update_option( 'acme_events_flushed', 1 );
	}
}

function acme_events_save_meta( $post_id, $post ) {
	if ( $post->post_status == 'auto-draft' ) {
		return;
	}
	if ( isset( $_POST['acme_event_date'] ) ) {
		update_post_meta( $post_id, '_acme_event_date', sanitize_text_field( wp_unslash( $_POST['acme_event_date'] ) ) );
	}
}

function acme_events_holiday_label( $label ) {
	return $label . ' 🎄';
}

function acme_events_upcoming( $count = 5 ) {
	$query = new WP_Query(
		array(
			'post_type'      => 'acme_event',
			'posts_per_page' => $count,
		)
	);
	return $query->posts;
}
`,
  "templates/upcoming-events.php": `<?php
/**
 * Template: the next events as a list.
 */
$events = acme_events_upcoming();
?>
<ul class="acme-events">
	<?php foreach ( $events as $event ) : ?>
		<li><a href="<?php echo esc_url( get_permalink( $event ) ); ?>"><?php echo esc_html( get_the_title( $event ) ); ?></a></li>
	<?php endforeach; ?>
</ul>
<?php do_action( 'acme_events_after_list', $events ); ?>
`,
  "uninstall.php": `<?php
defined( 'WP_UNINSTALL_PLUGIN' ) || exit;

delete_option( 'acme_events_flushed' );
`,
  "assets/css/acme-events.css": `.acme-event-date {
	font-weight: 600;
}

.acme-events li + li {
	margin-top: 0.25rem;
}
`,
  "readme.txt": `=== Acme Events ===
Contributors: acme
Tags: events, calendar
Requires at least: 6.4
Tested up to: 6.6
Stable tag: 1.2.0
License: GPLv2 or later

Events with dates and venues, listed on any page.
`,
  "composer.json": `{
    "name": "acme/acme-events",
    "type": "wordpress-plugin",
    "require": {
        "php": ">=8.1"
    },
    "require-dev": {
        "php-stubs/wordpress-stubs": "^6.6",
        "wp-cli/wp-cli-bundle": "^2.11"
    }
}
`,
  "vendor/bin/wp": "#!/usr/bin/env php\n<?php\n// WP-CLI launcher installed by Composer.\n",
  "vendor/php-stubs/wordpress-stubs/wordpress-stubs.php": `<?php
// Demo stand-in for php-stubs/wordpress-stubs: a few signatures, no bodies.
namespace {
    function add_action($hook_name, $callback, $priority = 10, $accepted_args = 1) {}
    function add_filter($hook_name, $callback, $priority = 10, $accepted_args = 1) {}
    function do_action($hook_name, ...$arg) {}
    function apply_filters($hook_name, $value, ...$args) {}
    function register_post_type($post_type, $args = array()) {}
    function get_option($option, $default_value = false) {}
    function update_option($option, $value, $autoload = null) {}
    function update_post_meta($post_id, $meta_key, $meta_value, $prev_value = '') {}
    function get_post_meta($post_id, $key = '', $single = false) {}
}
`,
  ".gitignore": "/vendor/\n",
};
