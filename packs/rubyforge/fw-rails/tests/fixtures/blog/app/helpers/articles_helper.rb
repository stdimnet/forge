module ArticlesHelper
  def status_badge(article)
    tag.span article.status.humanize, class: "badge badge-#{article.status}"
  end
end
