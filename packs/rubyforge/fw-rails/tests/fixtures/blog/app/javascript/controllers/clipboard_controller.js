import { Controller } from "@hotwired/stimulus"

// Connects to data-controller="clipboard"
export default class extends Controller {
  static targets = ["source", "button"]
  static values = { successDuration: { type: Number, default: 2000 } }

  copy(event) {
    event.preventDefault()
    navigator.clipboard.writeText(this.sourceTarget.value)
    this.buttonTarget.textContent = "Copied!"
    setTimeout(() => this.reset(), this.successDurationValue)
  }

  reset() {
    this.buttonTarget.textContent = "Copy link"
  }
}
