/**
 * Welcome screen > Remote Development > Code With Me: join a session without opening a
 * project first. Join Session… is the same action as Tools | Code With Me | Join Session…;
 * the host's project then opens in this window.
 */
import { Link, Users } from "lucide-solid";
import { runCommand } from "@/state/commands";
import { t } from "@/i18n";

const join = () => void runCommand("CodeWithMe.JoinSession");

export function CodeWithMeWelcomePage() {
  return (
    <div class="welcome-remote" data-testid="welcome-code-with-me">
      <div class="welcome-remote-head">
        <div>
          <h2>{t("tools.codeWithMe.title")}</h2>
          <p class="muted">{t("tools.codeWithMe.welcomeText")}</p>
        </div>
      </div>

      <div class="welcome-remote-section">
        <div class="welcome-remote-section-title">
          <Users size={15} />
          <span>{t("tools.codeWithMe.joinSession")}</span>
          <button class="btn-primary welcome-remote-new" onClick={join} data-testid="welcome-cwm-join">
            <Link size={14} /> {t("tools.codeWithMe.joinSessionEllipsis")}
          </button>
        </div>
        <div class="welcome-remote-empty">
          <Users size={30} class="muted" />
          <p>{t("tools.codeWithMe.askHostForLink")}</p>
          <button class="btn-secondary" onClick={join}>
            {t("tools.codeWithMe.joinSessionEllipsis")}
          </button>
        </div>
      </div>
    </div>
  );
}
