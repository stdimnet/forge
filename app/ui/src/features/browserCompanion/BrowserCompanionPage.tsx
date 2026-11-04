/**
 * Settings | Tools | Browser Companion: the built-in server, paired browsers, forge:// links
 * and copy-paste (or one-click) integrations for error pages and dev servers.
 */
import { createMemo, createResource, createSignal, For, onCleanup, Show } from "solid-js";
import {
  Check,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  Copy,
  Globe,
  Link2,
  LoaderCircle,
  Plus,
  SquareTerminal,
  Wand2,
} from "lucide-solid";
import { companionApi, type PairedClient, type PairingCode } from "@/ipc/companion";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { workspace } from "@/state/workspace";
import { confirmDialog } from "./ConfirmDialog";
import {
  FORGE_URL_TEMPLATE,
  isApplied,
  PROFILE_ENV_KEYS,
  PROFILE_FILES,
  rankIntegrations,
  type Integration,
  type Snippet,
} from "./integrations";
import { formatAgo, formatCountdown, formatPairingCode } from "./format";
import { t } from "@/i18n";
import { writeClipboard } from "@/features/clipboard/system";

const PAGE_ID = "tools.browser-companion";
const errorText = (e: unknown) => String((e as Error)?.message ?? e);
const nowSecs = () => Math.floor(Date.now() / 1000);

function Section(props: { title: string; children: unknown }) {
  return (
    <section class="set-section">
      <div class="set-section-title">
        <span>{props.title}</span>
      </div>
      <div class="bc-section-body">{props.children as never}</div>
    </section>
  );
}

/** Copy button with a short "Copied" confirmation. */
function CopyButton(props: { text: string; label?: string }) {
  const [copied, setCopied] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  const copy = async () => {
    // On failure a balloon says so; the text stays selectable.
    if (!(await writeClipboard(props.text))) return;
    setCopied(true);
    clearTimeout(timer);
    timer = setTimeout(() => setCopied(false), 1500);
  };
  return (
    <button class="bc-copy" classList={{ done: copied() }} title={t("tools.browserCompanion.copyToClipboard")} onClick={copy}>
      <Show when={copied()} fallback={<Copy size={13} />}>
        <Check size={13} />
      </Show>
      {props.label ?? (copied() ? t("common.copied") : t("common.copy"))}
    </button>
  );
}

/** Text where `backticked` parts render as inline code. */
function RichText(props: { text: string }) {
  return (
    <For each={props.text.split("`")}>
      {(part, i) => (i() % 2 ? <span class="mono bc-inline-code">{part}</span> : part)}
    </For>
  );
}

function CodeBlock(props: { snippet: Snippet }) {
  return (
    <div class="bc-code">
      <div class="bc-code-head">
        <span class="bc-code-file">{props.snippet.file}</span>
        <CopyButton text={props.snippet.code} />
      </div>
      <pre>
        <code>{props.snippet.code}</code>
      </pre>
    </div>
  );
}

function ServerStatusLine(props: { status?: { running: boolean; port?: number | null; error?: string | null } }) {
  return (
    <div class="bc-status" data-testid="bc-server-status">
      <Show
        when={props.status?.running}
        fallback={
          <Show when={props.status?.error} fallback={<span class="bc-dot off" />}>
            <CircleAlert size={14} class="bc-error-icon" />
          </Show>
        }
      >
        <span class="bc-dot on" />
      </Show>
      <span>
        <Show
          when={props.status?.running}
          fallback={props.status?.error ?? "Not running. Enable it above and click Apply."}
        >
          {t("tools.browserCompanion.listeningOn")} <span class="mono">127.0.0.1:{props.status!.port}</span> {t("tools.browserCompanion.localConnectionsOnly")}
        </Show>
      </span>
    </div>
  );
}

function ClientRow(props: { client: PairedClient; onRevoke: () => void }) {
  const c = () => props.client;
  const browser = () => (c().origin?.startsWith("moz-") ? "Firefox" : "Chromium");
  return (
    <div class="bc-client" data-client={c().id}>
      <Globe size={18} class="bc-client-icon" />
      <div class="bc-client-text">
        <div class="bc-client-name">{c().name}</div>
        <div class="muted bc-small">
          {t("tools.browserCompanion.extensionPaired", { browser: browser(), value: formatAgo(c().createdAt, nowSecs()) })}
        </div>
      </div>
      <div class="muted bc-small bc-client-used">
        {c().lastUsedAt ? t("tools.browserCompanion.lastUsed", { value: formatAgo(c().lastUsedAt!, nowSecs()) }) : t("tools.browserCompanion.neverUsed")}
      </div>
      <button class="btn-secondary bc-btn-small" onClick={props.onRevoke}>
        {t("tools.browserCompanion.unpair")}
      </button>
    </div>
  );
}

function PairingCard(props: { code: PairingCode; onCancel: () => void }) {
  const [now, setNow] = createSignal(nowSecs());
  const timer = setInterval(() => setNow(nowSecs()), 1000);
  onCleanup(() => clearInterval(timer));
  const left = () => Math.max(0, props.code.expiresAt - now());
  return (
    <div class="bc-pairing" data-testid="bc-pairing">
      <div class="bc-pairing-code" aria-label={t("tools.browserCompanion.pairingCode")}>
        {formatPairingCode(props.code.code)}
      </div>
      <div class="bc-pairing-text">
        <div class="bc-pairing-title">{t("tools.browserCompanion.enterThisCodeInThe")}</div>
        <div class="muted bc-small">
          {t("tools.browserCompanion.clickTheForgeIconIn")}
        </div>
        <div class="bc-row-actions bc-pairing-actions">
          <span class="bc-small bc-countdown" classList={{ expired: left() === 0 }}>
            <Show when={left() > 0} fallback="Expired">
              {t("tools.browserCompanion.expiresIn", { value: formatCountdown(left()) })}
            </Show>
          </span>
          <CopyButton text={props.code.code} label={t("tools.browserCompanion.copyCode")} />
          <button class="btn-secondary bc-btn-small" onClick={props.onCancel}>
            {t("common.cancel")}
          </button>
        </div>
      </div>
    </div>
  );
}

function IntegrationCard(props: {
  integration: Integration;
  detected: boolean;
  applied: boolean;
  canApply: boolean;
  onApply: () => Promise<void>;
}) {
  const i = () => props.integration;
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const apply = async () => {
    setBusy(true);
    setError(undefined);
    try {
      await props.onApply();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="bc-card" data-integration={i().id}>
      <div class="bc-card-head">
        <div class="bc-card-title">
          {i().title}
          <Show when={props.detected}>
            <span class="bc-badge">{t("common.detected")}</span>
          </Show>
        </div>
        <Show when={i().apply}>
          <Show
            when={!props.applied}
            fallback={
              <span class="bc-applied">
                <Check size={14} /> {t("tools.browserCompanion.setIn", { file: i().apply!.file })}
              </span>
            }
          >
            <button
              class="btn-secondary bc-btn-small"
              disabled={!props.canApply || busy()}
              title={props.canApply ? t("tools.browserCompanion.setIn2", { key: i().apply!.key, file: i().apply!.file }) : t("tools.browserCompanion.openAProjectFirst")}
              onClick={apply}
            >
              <Show when={busy()} fallback={<Wand2 size={13} />}>
                <LoaderCircle size={13} class="spin" />
              </Show>
              {t("tools.browserCompanion.applyToEllipsis", { file: i().apply!.file })}
            </button>
          </Show>
        </Show>
      </div>
      <div class="muted bc-small bc-card-desc">{i().description}</div>
      <For each={i().snippets}>{(s) => <CodeBlock snippet={s} />}</For>
      <Show when={i().note}>
        <div class="muted bc-small bc-card-note">
          <RichText text={i().note!} />
        </div>
      </Show>
      <Show when={error()}>
        <div class="bc-error bc-small">{error()}</div>
      </Show>
    </div>
  );
}

export function BrowserCompanionPage() {
  const [status, { refetch }] = createResource(companionApi.status);
  const [profile, { refetch: refetchProfile }] = createResource(
    () => workspace()?.root ?? false,
    () => companionApi.projectProfile(PROFILE_ENV_KEYS, PROFILE_FILES).catch(() => null),
  );
  const [pairError, setPairError] = createSignal<string>();
  const [showAll, setShowAll] = createSignal(false);
  const [linkUrl, setLinkUrl] = createSignal("forge://open?file=app/Models/User.php&line=42");
  const [linkError, setLinkError] = createSignal<string>();

  // Refresh while a code is shown so a successful pairing appears without reopening.
  const poll = setInterval(() => status()?.pairing && void refetch(), 1500);
  onCleanup(() => clearInterval(poll));
  void companionApi.onStatus(() => void refetch()).then((un) => onCleanup(un));

  const ranked = createMemo(() => rankIntegrations(profile()));
  const detectedCount = () => ranked().filter((r) => r.detected).length;
  const visible = () => (showAll() || detectedCount() === 0 ? ranked() : ranked().filter((r) => r.detected));

  async function startPairing() {
    setPairError(undefined);
    try {
      await companionApi.startPairing();
      await refetch();
    } catch (e) {
      setPairError(errorText(e));
    }
  }

  async function cancelPairing() {
    await companionApi.cancelPairing();
    await refetch();
  }

  async function revoke(client: PairedClient) {
    const ok = await confirmDialog({
      title: t("tools.browserCompanion.unpairBrowser"),
      message: `“${client.name}” will no longer be able to open files in Forge.`,
      okLabel: t("tools.browserCompanion.unpair"),
    });
    if (!ok) return;
    await companionApi.revokeClient(client.id);
    await refetch();
  }

  async function applyIntegration(i: Integration) {
    const change = i.apply!;
    const line = `${change.key}=${change.value}`;
    const ok = await confirmDialog({
      title: `Apply ${i.title.split(" (")[0]}`,
      message: t("tools.browserCompanion.forgeWillSetThisVariableLabel", { file: change.file, value: workspace()?.name ?? "the project" }),
      detail: line,
      question: "An existing value of the variable is replaced; nothing else in the file changes.",
      okLabel: t("common.apply"),
    });
    if (!ok) return;
    await companionApi.setEnvVar(change.file, change.key, change.value);
    await refetchProfile();
  }

  async function testLink() {
    setLinkError(undefined);
    try {
      await companionApi.openLink(linkUrl());
    } catch (e) {
      setLinkError(errorText(e));
    }
  }

  return (
    <div class="bc-page">
      <p class="muted bc-intro">
        {t("tools.browserCompanion.clickAComponentInThe")}
      </p>

      <SchemaPage pageId={PAGE_ID} />
      <div class="bc-status-row">
        <ServerStatusLine status={status()?.server} />
      </div>

      <Section title={t("tools.browserCompanion.pairedBrowsers")}>
        <div class="bc-clients">
          <For
            each={status()?.clients ?? []}
            fallback={
              <div class="bc-empty muted">
                {t("tools.browserCompanion.noBrowserIsPairedYet")}
              </div>
            }
          >
            {(c) => <ClientRow client={c} onRevoke={() => void revoke(c)} />}
          </For>
        </div>
        <Show
          when={status()?.pairing}
          keyed
          fallback={
            <div class="bc-row-actions">
              <button class="btn-secondary" data-testid="bc-pair" onClick={startPairing}>
                <Plus size={14} />
                {t("tools.browserCompanion.pairNewBrowserEllipsis")}
              </button>
              <span class="muted bc-small">
                {t("tools.browserCompanion.extensionBuild")} <span class="mono">browser-extension/</span> {t("tools.browserCompanion.andLoadItUnpackedSee")}
              </span>
            </div>
          }
        >
          {(code) => <PairingCard code={code} onCancel={() => void cancelPairing()} />}
        </Show>
        <Show when={pairError()}>
          <div class="bc-error bc-small">{pairError()}</div>
        </Show>
      </Section>

      <Section title={t("tools.browserCompanion.linksAndLauncher")}>
        <div class="bc-grid">
          <Link2 size={16} class="bc-grid-icon" />
          <div>
            <div>
              {t("tools.browserCompanion.toolsWithACustomEditor")}
              <span class="mono bc-inline-code">{FORGE_URL_TEMPLATE}</span>
            </div>
            <div class="bc-row-actions bc-test-link">
              <input
                class="set-input mono bc-link-input"
                value={linkUrl()}
                spellcheck={false}
                aria-label={t("tools.browserCompanion.linkToTest")}
                onInput={(e) => setLinkUrl(e.currentTarget.value)}
              />
              <button class="btn-secondary bc-btn-small" onClick={testLink}>
                {t("tools.browserCompanion.openLink")}
              </button>
            </div>
            <Show when={linkError()}>
              <div class="bc-error bc-small">{linkError()}</div>
            </Show>
          </div>
          <SquareTerminal size={16} class="bc-grid-icon" />
          <div>
            {t("tools.browserCompanion.fromATerminalLabel")} <span class="mono bc-inline-code">{t("tools.browserCompanion.forgeAppModelsUserPhp")}</span>{" "}
            <span class="muted">or</span> <span class="mono bc-inline-code">{t("tools.browserCompanion.forgeLine42AppModels")}</span>
            <div class="muted bc-small">{t("tools.browserCompanion.aRunningForgeWindowOpens")}</div>
          </div>
        </div>
      </Section>

      <Section title={t("tools.browserCompanion.frameworkIntegrations")}>
        <Show when={workspace()} fallback={<div class="muted bc-small">{t("tools.browserCompanion.openAProjectToSee")}</div>}>
          <div class="muted bc-small bc-detected-line">
            <Show when={detectedCount()} fallback="No supported framework was found in this project; all integrations are shown.">
              {t("tools.browserCompanion.showingWhatWasFoundIn", { name: workspace()!.name })}
            </Show>
          </div>
        </Show>
        <div class="bc-cards">
          <For each={visible()}>
            {(row) => (
              <IntegrationCard
                integration={row.integration}
                detected={row.detected}
                applied={isApplied(row.integration, profile())}
                canApply={!!workspace()}
                onApply={() => applyIntegration(row.integration)}
              />
            )}
          </For>
        </div>
        <Show when={detectedCount() > 0 && detectedCount() < ranked().length}>
          <button class="bc-link-btn" onClick={() => setShowAll((v) => !v)}>
            <Show when={showAll()} fallback={<ChevronRight size={14} />}>
              <ChevronDown size={14} />
            </Show>
            {showAll() ? t("tools.browserCompanion.showDetectedOnly") : t("tools.browserCompanion.showAllIntegrationsMore", { value: ranked().length - detectedCount() })}
          </button>
        </Show>
      </Section>
    </div>
  );
}
