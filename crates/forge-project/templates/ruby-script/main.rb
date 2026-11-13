#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative "../lib/{{module}}"

puts {{class}}::Greeter.new(ARGV.first || "world").greeting
