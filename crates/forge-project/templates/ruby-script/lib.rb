# frozen_string_literal: true

module {{class}}
  # Greets someone.
  class Greeter
    def initialize(name)
      @name = name
    end

    def greeting
      "Hello, #{@name}!"
    end
  end
end
