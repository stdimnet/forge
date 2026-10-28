/**
 * A small Ruby gem for browser mode, opened with `?project=ruby`: `pantry`, a grocery
 * inventory with Bundler (Gemfile + Gemfile.lock), Rake tasks, RSpec specs, Minitest
 * tests, a script in `bin/`, an ERB view and something for RuboCop to find. rubyMock.ts
 * answers the `ruby_*` commands, running the Rust crates through WebAssembly.
 */
import type { ProjectInfo } from "@/ipc/project";

export const rubyDemoName = "pantry";

/** True when the page asks for the Ruby demo project. */
export const isRubyDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "ruby";

export const rubyDemoInfo: ProjectInfo = {
  languages: ["ruby"],
  frameworks: [],
  packs: ["rubyforge"],
  tools: ["bundler", "rake", "git"],
  primary: { id: "ruby", name: "Ruby", language: "ruby" },
};

const GEMFILE = `# frozen_string_literal: true

source "https://rubygems.org"

ruby file: ".ruby-version"

gemspec

gem "rake", "~> 13.2"
gem "money", ">= 6.16", "< 7"

group :development, :test do
  gem "debug", platforms: %i[mri windows], require: "debug/prelude"
  gem "rubocop", "~> 1.66", require: false
end

group :test do
  gem "rspec", "~> 3.13"
  gem "minitest", "~> 5.25"
end
`;

const GEMFILE_LOCK = `PATH
  remote: .
  specs:
    pantry (0.4.0)
      money (>= 6.16)

GEM
  remote: https://rubygems.org/
  specs:
    ast (2.4.2)
    bigdecimal (3.1.8)
    concurrent-ruby (1.3.4)
    debug (1.9.2)
      irb (~> 1.10)
      reline (>= 0.3.8)
    diff-lcs (1.5.1)
    i18n (1.14.6)
      concurrent-ruby (~> 1.0)
    io-console (0.7.2)
    irb (1.14.1)
      rdoc (>= 4.0.0)
      reline (>= 0.4.2)
    json (2.7.2)
    language_server-protocol (3.17.0.3)
    minitest (5.25.1)
    money (6.19.0)
      i18n (>= 0.6.4, <= 2)
    parallel (1.26.3)
    parser (3.3.5.0)
      ast (~> 2.4.1)
      racc
    psych (5.1.2)
      stringio
    racc (1.8.1)
    rainbow (3.1.1)
    rake (13.2.1)
    rdoc (6.7.0)
      psych (>= 4.0.0)
    regexp_parser (2.9.2)
    reline (0.5.10)
      io-console (~> 0.5)
    rspec (3.13.0)
      rspec-core (~> 3.13.0)
      rspec-expectations (~> 3.13.0)
      rspec-mocks (~> 3.13.0)
    rspec-core (3.13.1)
      rspec-support (~> 3.13.0)
    rspec-expectations (3.13.3)
      diff-lcs (>= 1.2.0, < 2.0)
      rspec-support (~> 3.13.0)
    rspec-mocks (3.13.2)
      diff-lcs (>= 1.2.0, < 2.0)
      rspec-support (~> 3.13.0)
    rspec-support (3.13.1)
    rubocop (1.66.1)
      json (~> 2.3)
      language_server-protocol (>= 3.17.0)
      parallel (~> 1.10)
      parser (>= 3.3.0.2)
      rainbow (>= 2.2.2, < 4.0)
      regexp_parser (>= 2.4, < 3.0)
      rubocop-ast (>= 1.32.2, < 2.0)
      ruby-progressbar (~> 1.7)
      unicode-display_width (>= 2.4.0, < 3.0)
    rubocop-ast (1.32.3)
      parser (>= 3.3.1.0)
    ruby-progressbar (1.13.0)
    stringio (3.1.1)
    unicode-display_width (2.6.0)

PLATFORMS
  arm64-darwin
  x86_64-linux

DEPENDENCIES
  debug
  minitest (~> 5.25)
  money (>= 6.16, < 7)
  pantry!
  rake (~> 13.2)
  rspec (~> 3.13)
  rubocop (~> 1.66)

RUBY VERSION
   ruby 3.3.6p108

BUNDLED WITH
   2.5.22
`;

const GEMSPEC = `# frozen_string_literal: true

require_relative "lib/pantry/version"

Gem::Specification.new do |spec|
  spec.name = "pantry"
  spec.version = Pantry::VERSION
  spec.authors = ["Anna Schmidt"]
  spec.summary = "Keeps track of what is in the pantry and what to buy."
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.2"
  spec.files = Dir["lib/**/*.rb", "bin/*"]
  spec.bindir = "bin"
  spec.executables = ["restock"]
  spec.add_dependency "money", ">= 6.16"
end
`;

const RAKEFILE = `# frozen_string_literal: true

require "bundler/gem_tasks"
require "rspec/core/rake_task"
require "rake/testtask"
require "rubocop/rake_task"

RSpec::Core::RakeTask.new(:spec)

Rake::TestTask.new(:test) do |t|
  t.libs << "test" << "lib"
  t.test_files = FileList["test/**/*_test.rb"]
end

RuboCop::RakeTask.new

namespace :db do
  desc "Load the sample pantry"
  task :seed do
    ruby "bin/restock --seed"
  end

  desc "Empty the pantry"
  task :reset
end

task default: %i[spec test rubocop]
`;

const REPORT_RAKE = `# frozen_string_literal: true

desc "Print the shopping list for a month"
task :shopping_list, [:month] do |_t, args|
  require "pantry"
  puts Pantry::Inventory.load.shopping_list(month: args[:month])
end
`;

const PANTRY = `# frozen_string_literal: true

require "money"
require_relative "pantry/version"
require_relative "pantry/item"
require_relative "pantry/inventory"

# Keeps track of what is in the pantry.
module Pantry
  class Error < StandardError; end
end
`;

const VERSION = `# frozen_string_literal: true

module Pantry
  VERSION = "0.4.0"
end
`;

const ITEM = `# frozen_string_literal: true

module Pantry
  # Something on a shelf: how many there are and how many there should be.
  Item = Struct.new(:name, :quantity, :minimum, :price_cents, keyword_init: true) do
    def low?
      quantity < minimum
    end

    def missing
      [minimum - quantity, 0].max
    end

    def price
      Money.new(price_cents, "EUR")
    end
  end
end
`;

const INVENTORY = `# frozen_string_literal: true

module Pantry
  # The items of the pantry and what to buy.
  class Inventory
    attr_reader :items

    def initialize(items = [])
      @items = items
    end

    def add(name, quantity:, minimum: 1, price_cents: 0)
      items << Item.new(name: name, quantity: quantity, minimum: minimum, price_cents: price_cents)
      self
    end

    def shopping_list
      total = 0
      items.select(&:low?).map { |item| "#{item.missing} x #{item.name}" }
    end

    def total_cents
      items.sum { |item| item.missing * item.price_cents }
    end

    def to_s
      'Pantry with ' + items.size.to_s + ' items'
    end
  end
end
`;

const SPEC_HELPER = `# frozen_string_literal: true

require "pantry"

RSpec.configure do |config|
  config.expect_with :rspec do |c|
    c.syntax = :expect
  end
end
`;

const INVENTORY_SPEC = `# frozen_string_literal: true

RSpec.describe Pantry::Inventory do
  subject(:inventory) { described_class.new }

  it "starts empty" do
    expect(inventory.items).to be_empty
  end

  describe "#shopping_list" do
    it "lists the items below their minimum" do
      inventory.add("Rice", quantity: 0, minimum: 2)
      expect(inventory.shopping_list).to eq(["2 x Rice"])
    end

    it "skips items that are stocked" do
      inventory.add("Salt", quantity: 3, minimum: 1)
      expect(inventory.shopping_list).to be_empty
    end
  end

  describe "#total_cents" do
    it "adds up what is missing" do
      inventory.add("Coffee", quantity: 0, minimum: 2, price_cents: 525)
      expect(inventory.total_cents).to eq(1050)
    end

    context "with a voucher" do
      it "takes ten percent off"
    end
  end
end
`;

const TEST_HELPER = `# frozen_string_literal: true

$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "pantry"
require "minitest/autorun"
`;

const ITEM_TEST = `# frozen_string_literal: true

require "test_helper"

module Pantry
  class ItemTest < Minitest::Test
    def setup
      @item = Item.new(name: "Flour", quantity: 1, minimum: 3, price_cents: 129)
    end

    def test_low
      assert_predicate @item, :low?
    end

    def test_missing
      assert_equal 2, @item.missing
    end

    def test_price
      assert_equal "1.29", @item.price.to_s
    end
  end
end
`;

const RESTOCK = `#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative "../lib/pantry"

inventory = Pantry::Inventory.new
inventory.add("Rice", quantity: 0, minimum: 2, price_cents: 199)
inventory.add("Coffee", quantity: 1, minimum: 2, price_cents: 525)

puts "Shopping list:"
inventory.shopping_list.each { |line| puts "  #{line}" }
puts "Total: #{Money.new(inventory.total_cents, 'EUR').format}"
`;

const VIEW = `<h1>Pantry</h1>
<%# The shopping list of the month %>
<% if @inventory.items.empty? %>
  <p class="empty">Nothing on the shelves yet.</p>
<% else %>
  <ul class="items">
    <% @inventory.items.each do |item| %>
      <li class="<%= item.low? ? "low" : "ok" %>">
        <%= item.name %> &times; <%= item.quantity %>
      </li>
    <% end %>
  </ul>
<% end %>
<p>Total: <%= number_to_currency(@inventory.total_cents / 100.0) %></p>
`;

const RUBOCOP_YML = `AllCops:
  TargetRubyVersion: 3.3
  NewCops: enable
  SuggestExtensions: false

Style/StringLiterals:
  EnforcedStyle: double_quotes
`;

const README = `# pantry

Keeps track of what is in the pantry and what to buy.

    bundle install
    bundle exec rake          # specs, tests and RuboCop
    bin/restock
`;

export const rubyDemoFiles: Record<string, string> = {
  Gemfile: GEMFILE,
  "Gemfile.lock": GEMFILE_LOCK,
  "pantry.gemspec": GEMSPEC,
  Rakefile: RAKEFILE,
  ".ruby-version": "3.3.6\n",
  ".rubocop.yml": RUBOCOP_YML,
  ".rspec": "--require spec_helper\n",
  ".gitignore": "/.bundle/\n/pkg/\n/coverage/\n",
  "README.md": README,
  "bin/restock": RESTOCK,
  "lib/pantry.rb": PANTRY,
  "lib/pantry/version.rb": VERSION,
  "lib/pantry/item.rb": ITEM,
  "lib/pantry/inventory.rb": INVENTORY,
  "lib/tasks/report.rake": REPORT_RAKE,
  "spec/spec_helper.rb": SPEC_HELPER,
  "spec/pantry/inventory_spec.rb": INVENTORY_SPEC,
  "test/test_helper.rb": TEST_HELPER,
  "test/pantry/item_test.rb": ITEM_TEST,
  "app/views/inventory/index.html.erb": VIEW,
};
