/**
 * Welcome screen > Remote Development > Code With Me: join a session without opening a
 * project first. Join Session… is the same action as Tools | Code With Me | Join Session…;
 * the host's project then opens in this window.
 */
import { Link, Users } from "lucide-solid";
import { runCommand } from "@/state/commands";

const join = () => void runCommand("CodeWithMe.JoinSession");

export function CodeWithMeWelcomePage() {
  return (
    <div class="welcome-remote" data-testid="welcome-code-with-me">
      <div class="welcome-remote-head">
        <div>
          <h2>Code With Me</h2>
          <p class="muted">
            Join a session another developer started. Paste the session link from the host; once the host accepts you, the
            host's project opens in this window.
          </p>
        </div>
      </div>

      <div class="welcome-remote-section">
        <div class="welcome-remote-section-title">
          <Users size={15} />
          <span>Join Session</span>
          <button class="btn-primary welcome-remote-new" onClick={join} data-testid="welcome-cwm-join">
            <Link size={14} /> Join Session…
          </button>
        </div>
        <div class="welcome-remote-empty">
          <Users size={30} class="muted" />
          <p>Ask the host for the session link (Tools | Code With Me | Copy Session Link).</p>
          <button class="btn-secondary" onClick={join}>
            Join Session…
          </button>
        </div>
      </div>
    </div>
  );
}
