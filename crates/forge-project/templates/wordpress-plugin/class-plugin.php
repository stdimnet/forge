<?php

if (!defined('ABSPATH')) {
    exit;
}

/**
 * Main class of the {{title}} plugin.
 */
final class {{class}}
{
    public static function init(): void
    {
        add_shortcode('{{module}}', [self::class, 'shortcode']);
    }

    /**
     * Renders [{{module}} name="World"].
     *
     * @param array<string, string>|string $atts
     */
    public static function shortcode($atts): string
    {
        $atts = shortcode_atts(['name' => 'World'], $atts, '{{module}}');

        return sprintf(
            '<p class="{{package}}">%s</p>',
            esc_html(sprintf(__('Hello, %s!', '{{package}}'), $atts['name']))
        );
    }
}
