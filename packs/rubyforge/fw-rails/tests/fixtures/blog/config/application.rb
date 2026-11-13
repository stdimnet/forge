require_relative "boot"

require "rails/all"

Bundler.require(*Rails.groups)

module Blog
  class Application < Rails::Application
    config.load_defaults 7.2
    config.i18n.default_locale = :en
    config.i18n.available_locales = %i[en de]
  end
end
