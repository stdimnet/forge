class CommentsController < ApplicationController
  def create
    @article = Article.find(params[:article_id])
    @comment = @article.comments.create!(comment_params)
    redirect_to @article
  end

  def destroy
    Comment.find(params[:id]).destroy
    redirect_to article_path(params[:article_id]), status: :see_other
  end

  private

  def comment_params
    params.require(:comment).permit(:body)
  end
end
