class SessionsController < ApplicationController
  def new
  end

  def create
    user = User.find_by(email_address: params[:email_address])
    redirect_to user ? root_path : new_session_path
  end

  def destroy
    redirect_to root_path, status: :see_other
  end
end
