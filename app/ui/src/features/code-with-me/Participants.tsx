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
    following ? item("Stop Following", stopFollowing, "CodeWithMe.StopFollowing") : item("Follow", () => follow(p.id)),
    item("Jump To", () => void jumpTo(p), undefined, { disabled: !presence().get(p.id)?.file }),
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
  if (leader === session().me) return "following you";
  const name = session().participants.find((x) => x.id === leader)?.name;
  return name ? `following ${name}` : undefined;
}

export function Participants() {
  const s = () => session();
  return (
    <div class="cwm-participants" role="list" aria-label="Participants">
      <For each={s().participants}>
        {(p) => (
          <div
            class="cwm-participant"
            classList={{ followed: followState().leader === p.id, clickable: !isMe(p) }}
            role="listitem"
            data-participant={p.id}
            title={isMe(p) ? undefined : followState().leader === p.id ? `Stop following ${p.name}` : `Follow ${p.name}`}
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
                  {p.role === "host" ? " (host)" : ""}
                  {isMe(p) ? " (you)" : ""}
                </span>
              </span>
              <Show when={(!isMe(p) && locationLabel(presence().get(p.id))) || followingLabel(p)}>
                <span class="cwm-participant-where muted">
                  {[!isMe(p) ? locationLabel(presence().get(p.id)) : undefined, followingLabel(p)].filter(Boolean).join(" · ")}
                </span>
              </Show>
            </span>
            <Show when={followState().leader === p.id}>
              <Eye size={14} class="cwm-participant-following" aria-label={`Following ${p.name}`} />
            </Show>
            <Show when={p.role === "guest"}>
              <span class="cwm-participant-perm muted" title={describePermissions(p.permissions)}>
                {LEVEL_TITLES[levelOf(p.permissions)]}
              </span>
            </Show>
            <Show when={!isMe(p)}>
              <button
                class="icon-btn cwm-participant-menu"
                title={`Actions for ${p.name}`}
                aria-label={`Actions for ${p.name}`}
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
      <div class="cwm-toolbar-avatars" role="group" aria-label="Participants">
        <For each={others().slice(0, 4)}>
          {(p) => (
            <button
              class="cwm-toolbar-avatar"
              classList={{ followed: followState().leader === p.id }}
              data-participant={p.id}
              title={`${p.name}${p.role === "host" ? " (host)" : ` · ${LEVEL_TITLES[levelOf(p.permissions)]}`}${followState().leader === p.id ? " · following" : ""}`}
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

/** The participants following the host (for the host's popup). */
export const hostFollowers = () => followersOf(session().me ?? 0, presence()).length;
