class Article < ApplicationRecord
  belongs_to :user
  has_many :comments, dependent: :destroy

  enum :status, { draft: 0, published: 1 }

  scope :published, -> { where(status: :published) }
  scope :recent, -> { order(published_at: :desc) }

  validates :title, presence: true
end
