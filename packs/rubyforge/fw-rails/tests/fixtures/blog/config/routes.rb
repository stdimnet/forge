Rails.application.routes.draw do
  get "up" => "rails/health#show", as: :rails_health_check

  root "articles#index"

  resources :articles do
    resources :comments, only: %i[create destroy]
    member do
      patch :publish
    end
    collection do
      get :search
    end
  end

  resource :session, only: %i[new create destroy]
  get "about", to: "pages#about"

  namespace :admin do
    root to: "dashboard#show"
    resources :users, only: %i[index show]
  end
end
