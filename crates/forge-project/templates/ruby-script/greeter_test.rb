# frozen_string_literal: true

require "minitest/autorun"
require "{{module}}"

class GreeterTest < Minitest::Test
  def test_greeting
    assert_equal "Hello, Ruby!", {{class}}::Greeter.new("Ruby").greeting
  end
end
