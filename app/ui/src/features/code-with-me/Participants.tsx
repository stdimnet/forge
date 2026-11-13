/**
 * The participant list of the session popup and the Code With Me tool window, and each
 * participant's menu: Follow / Stop Following, Jump To and, for the host, the guest's
 * Permissions and Remove Guest. Also the participants' avatars next to the toolbar icon.
 */
import { For, Show } from "solid-js";
import { EllipsisVertical, Eye } from "lucide-solid";
import { levelOf, type Participant } from "@/ipc/collab";
import { item, separator, showContextMenu, type MenuEntry } from "@/features/contextMenu/state";
import { session } from "./store";
import { presence } from "./presence";
import { follow, followState, jumpTo, stopFollowing } from "./follow";
import { followersOf, locationLabel } from "./followState";
import { guestEntries, LEVEL_TITLES } from "./hostActions";
import { describePermissions } from "./permissionText";
import { ParticipantPath } from "./ConnectionStatus";
import { tooltip } from "@/shell/tooltip";
import { t } from "@/i18n";

const initials = (name: string) =>
  name
    .split(/[\s._-]+/)
    .filter(Boolean)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase() || "?";

/** A participant's colored circle with initials (also used for carets' colors). */
export function Avatar(props: { participant: Participant; size?: number }) {
  return (
    <span
      class="cwm-avatar"
      style={{
        background: `var(--collab-${Math.min(8, Math.max(1, props.participant.color))})`,
        width: `${props.size ?? 20}px`,
        height: `${props.size ?? 20}px`,
      }}
      aria-hidden="true"
    >
      {initials(props.participant.name)}
    </span>
  );
}

const isMe = (p: Participant) => p.id === session().me;

/** Follow or Stop Following, and Jump To, for another participant. */
export function followEntries(p: Participant): MenuEntry[] {
  if (isMe(p)) return [];
  const following = followState().leader === p.id;
  return [
    following
      ? item(t("tools.codeWithMe.stopFollowing"), stopFollowing, "CodeWithMe.StopFollowing")
      : item(t("tools.codeWithMe.follow"), () => follow(p.id)),
    item(t("tools.codeWithMe.jumpTo"), () => void jumpTo(p), undefined, { disabled: !presence().get(p.id)?.file }),
  ];
}

export function participantEntries(p: Participant): MenuEntry[] {
  const host = session().role === "host" && p.role === "guest" ? guestEntries(p) : [];
  return [...followEntries(p), ...(host.length ? [separator, ...host] : [])];
}

export function showParticipantMenu(p: Participant, x: number, y: number): void {
  const entries = participantEntries(p);
  if (entries.length) showContextMenu(entries, x, y);
}

/** "following you" / "following Alex" for a participant row. */
function followingLabel(p: Participant): string | undefined {
  const leader = isMe(p) ? followState().leader : presence().get(p.id)?.following;
  if (leader === null || leader === undefined) return undefined;
  if (leader === session().me) return t("tools.codeWithMe.followingYou");
  const name = session().participants.find((x) => x.id === leader)?.name;
  return name ? t("tools.codeWithMe.followingName", { name }) : undefined;
}

/** Where a participant is and whom they follow, e.g. "User.php:12 · following you". */
function whereText(p: Participant): string {
  return [!isMe(p) ? locationLabel(presence().get(p.id)) : undefined, followingLabel(p)].filter(Boolean).join(" · ");
}

/**
 * The participant rows. `paths` (the session popup and the tool window) adds each
 * connection's path (Direct / Relay, Connecting…, Reconnecting…) below the name, so there is a
 * single participant list.
 */
export function Participants(props: { paths?: boolean }) {
  const s = () => session();
  return (
    <div class="cwm-participants" role="list" aria-label={t("tools.codeWithMe.participants")}>
      <For each={s().participants}>
        {(p) => (
          <div
            class="cwm-participant"
            classList={{ followed: followState().leader === p.id, clickable: !isMe(p), reconnecting: !!p.reconnecting }}
            role="listitem"
            tabIndex={isMe(p) ? undefined : -1}
            data-nav={isMe(p) ? undefined : ""}
            data-participant={p.id}
            title={
              isMe(p)
                ? undefined
                : followState().leader === p.id
                  ? t("tools.codeWithMe.stopFollowingName", { name: p.name })
                  : t("tools.codeWithMe.followName", { name: p.name })
            }
            onClick={(e) => {
              if (isMe(p) || (e.target as Element).closest(".cwm-participant-menu")) return;
              if (followState().leader === p.id) stopFollowing();
              else follow(p.id);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              showParticipantMenu(p, e.clientX, e.clientY);
            }}
          >
            <Avatar participant={p} />
            <span class="cwm-participant-text">
              <span class="cwm-participant-name">
                {p.name}
                <span class="muted">
                  {p.role === "host" ? ` ${t("tools.codeWithMe.hostTag")}` : ""}
                  {isMe(p) ? ` ${t("tools.codeWithMe.youTag")}` : ""}
                </span>
              </span>
              <Show when={p.reconnecting && !props.paths}>
                <span class="cwm-participant-where cwm-participant-reconnecting" title={t("tools.codeWithMe.connectionDropped")}>
                  {t("tools.codeWithMe.reconnecting")}
                </span>
              </Show>
              <Show
                when={props.paths && !isMe(p)}
                fallback={
                  <Show when={!p.reconnecting && whereText(p)}>
                    {(text) => <span class="cwm-participant-where muted">{text()}</span>}
                  </Show>
                }
              >
                <span class="cwm-participant-detail">
                  <ParticipantPath participant={p} />
                  <Show when={!p.reconnecting && whereText(p)}>
                    {(text) => <span class="cwm-participant-where muted">{text()}</span>}
                  </Show>
                </span>
              </Show>
            </span>
            <Show when={followState().leader === p.id}>
              <Eye size={14} class="cwm-participant-following" aria-label={t("tools.codeWithMe.followingLeader", { name: p.name })} />
            </Show>
            <Show when={p.role === "guest"}>
              <span class="cwm-participant-perm muted" title={describePermissions(p.permissions)}>
                {LEVEL_TITLES[levelOf(p.permissions)]}
              </span>
            </Show>
            <Show when={!isMe(p)}>
              <button
                class="icon-btn cwm-participant-menu"
                title={t("tools.codeWithMe.actionsFor", { name: p.name })}
                aria-label={t("tools.codeWithMe.actionsFor", { name: p.name })}
                onClick={(e) => {
                  const r = e.currentTarget.getBoundingClientRect();
                  showParticipantMenu(p, r.left, r.bottom + 2);
                }}
              >
                <EllipsisVertical size={14} />
              </button>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}

/** The other participants' avatars next to the toolbar icon; clicking one follows them. */
export function ToolbarAvatars() {
  const others = () => session().participants.filter((p) => !isMe(p));
  return (
    <Show when={session().role !== "none" && others().length > 0}>
      <div class="cwm-toolbar-avatars" role="group" aria-label={t("tools.codeWithMe.participants")}>
        <For each={others().slice(0, 4)}>
          {(p) => (
            <button
              class="cwm-toolbar-avatar"
              data-participant={p.id}
              classList={{ followed: followState().leader === p.id, reconnecting: !!p.reconnecting }}
              {...tooltip(() => avatarTitle(p))}
              aria-pressed={followState().leader === p.id}
              onClick={() => (followState().leader === p.id ? stopFollowing() : follow(p.id))}
              onContextMenu={(e) => {
                e.preventDefault();
                showParticipantMenu(p, e.clientX, e.clientY);
              }}
            >
              <Avatar participant={p} size={20} />
            </button>
          )}
        </For>
        <Show when={others().length > 4}>
          <span class="cwm-toolbar-more muted">+{others().length - 4}</span>
        </Show>
      </div>
    </Show>
  );
}

/** "Alex · Edit files · following" for an avatar next to the toolbar icon. */
function avatarTitle(p: Participant): string {
  return [
    p.role === "host" ? t("tools.codeWithMe.participantHost", { name: p.name }) : p.name,
    p.role === "host" ? undefined : LEVEL_TITLES[levelOf(p.permissions)],
    p.reconnecting ? t("tools.codeWithMe.reconnecting") : undefined,
    followState().leader === p.id ? t("tools.codeWithMe.followingTag") : undefined,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** The participants following the host (for the host's popup). */
export const hostFollowers = () => followersOf(session().me ?? 0, presence()).length;
