class ArticlesController < ApplicationController
  before_action :set_article, only: %i[show edit update destroy publish]

  def index
    @articles = Article.published.order(created_at: :desc)
  end

  def show
    @comment = @article.comments.build
  end

  def new
    @article = Article.new
  end

  def edit
  end

  def create
    @article = Article.new(article_params)

    if @article.save
      redirect_to @article, notice: t(".created")
    else
      render :new, status: :unprocessable_entity
    end
  end

  def update
    if @article.update(article_params)
      redirect_to article_path(@article), notice: t(".updated")
    else
      render :edit, status: :unprocessable_entity
    end
  end

  def destroy
    @article.destroy!
    redirect_to articles_path, status: :see_other, notice: t(".destroyed")
  end

  def publish
    @article.update!(status: :published, published_at: Time.current)
    respond_to do |format|
      format.turbo_stream { render turbo_stream: turbo_stream.replace("article_header", partial: "articles/header") }
      format.html { redirect_to @article }
    end
  end

  def search
    @articles = Article.where("title ILIKE ?", "%#{params[:q]}%")
    render :index
  end

  private

  def set_article
    @article = Article.find(params[:id])
  end

  def article_params
    params.require(:article).permit(:title, :body, :status)
  end
end
