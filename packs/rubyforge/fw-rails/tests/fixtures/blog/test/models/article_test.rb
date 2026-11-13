require "test_helper"

class ArticleTest < ActiveSupport::TestCase
  test "requires a title" do
    assert_not Article.new(body: "x").valid?
  end
end
