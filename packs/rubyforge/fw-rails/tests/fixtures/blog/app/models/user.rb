class User < ApplicationRecord
  has_secure_password
  has_many :articles
  has_many :comments

  normalizes :email_address, with: ->(e) { e.strip.downcase }
end
