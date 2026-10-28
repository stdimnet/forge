module Admin
  class DashboardController < ApplicationController
    def show
      @users = User.order(:name)
    end
  end
end
