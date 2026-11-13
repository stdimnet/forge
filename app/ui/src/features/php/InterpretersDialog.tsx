/**
 * The "CLI Interpreters" dialog of Settings | Languages & Frameworks | PHP (the "…" button
 * next to CLI Interpreter), laid out like PHPStorm's: the interpreters on the left (+ / − /
 * copy), the selected one on the right (Name, "Visible only for this project", General with
 * PHP executable, version, php.ini, debugger, Path mappings for Docker / Compose / SSH / WSL,
 * Additional configuration options, Show phpinfo and Reload). OK hands the list back to the
 * PHP page, which keeps it in the Settings draft until Apply.
 */
import { createEffect, createResource, createSignal, For, Match, on, onCleanup, Show, Switch, type JSX } from "solid-js";
import { createStore, produce, unwrap } from "solid-js/store";
import { Layer } from "@/shell/layers";
import { Box, Container, Copy, FolderOpen, Minus, Plus, RefreshCw, Server, Terminal } from "lucide-solid";
import { isTauri } from "@/ipc";
import { remoteApi, type SshConfigView } from "@/ipc/remote";
import { servicesApi } from "@/ipc/services";
import { suspendKeymap } from "@/keymap";
import { chrome } from "@/shell/window/chrome";
import {
  emptyInterpreter,
  interpreterDetail,
  interpretersApi,
  KIND_LABELS,
  newInterpreterId,
  uniqueName,
  type DetectedInterpreter,
  type InterpreterKind,
  type PathMapping,
  type PhpInterpreter,
} from "./interpreters";
import { t } from "@/i18n";
import { listboxKeyDown, optionTabIndex } from "@/features/project/treeNavigation";

const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

const KIND_ICONS: Record<InterpreterKind, (p: { size: number }) => JSX.Element> = {
  local: (p) => <Terminal size={p.size} />,
  docker: (p) => <Box size={p.size} />,
  compose: (p) => <Container size={p.size} />,
  ssh: (p) => <Server size={p.size} />,
  wsl: (p) => <Terminal size={p.size} />,
};

async function browseFile(): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: false, multiple: false });
  return typeof picked === "string" ? picked : null;
}

/** A modal frame with Escape / Enter handling (dialog keys, not shortcuts). */
function Modal(props: { title: string; width: number; class?: string; testId?: string; onClose: () => void; onOk?: () => void; children: JSX.Element; buttons: JSX.Element }) {
  const release = suspendKeymap();
  onCleanup(release);
  return (
    <Layer kind="modal">
      <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
        <div
          class={`dialog php-interp-dialog ${props.class ?? ""}`}
          style={{ width: `min(${props.width}px, calc(100vw - 32px))` }}
          role="dialog"
          data-testid={props.testId}
          onKeyDown={(e) => {
            if (e.key === "Escape") props.onClose();
            else if (e.key === "Enter" && props.onOk && !(e.target instanceof HTMLTextAreaElement)) props.onOk();
            else return;
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          <div class="dialog-title">{props.title}</div>
          {props.children}
          <div class="dialog-buttons">{props.buttons}</div>
        </div>
      </div>
    </Layer>
  );
}

export interface InterpretersDialogProps {
  interpreters: PhpInterpreter[];
  /** The project's interpreter (a name or a local path). */
  selected: string;
  onOk: (list: PhpInterpreter[], selected: string | null) => void;
  onClose: () => void;
}

export function InterpretersDialog(props: InterpretersDialogProps) {
  const [list, setList] = createStore<PhpInterpreter[]>(clone(props.interpreters).map((i) => ({ ...i, id: i.id || newInterpreterId() })));
  const initial = list.find((i) => i.name === props.selected) ?? list[0];
  const [current, setCurrent] = createSignal<string | null>(initial?.id ?? null);
  const [addMenu, setAddMenu] = createSignal(false);
  const [remote, setRemote] = createSignal(false);
  const [phpinfo, setPhpinfo] = createSignal<{ name: string; text: string | null; error?: string } | null>(null);
  const [errors, setErrors] = createStore<Record<string, string>>({});
  const [reloading, setReloading] = createSignal<string | null>(null);
  const [detected] = createResource(() => interpretersApi.detect().catch((): DetectedInterpreter[] => []));

  const index = () => list.findIndex((i) => i.id === current());
  const sel = () => list[index()];
  const edit = (fn: (i: PhpInterpreter) => void) => {
    const i = index();
    if (i >= 0) setList(i, produce(fn));
  };

  const reload = async (id: string) => {
    const i = list.findIndex((x) => x.id === id);
    if (i < 0) return;
    setReloading(id);
    setErrors(id, "");
    try {
      const info = await interpretersApi.info(clone(unwrap(list[i])));
      setList(
        i,
        produce((x) => {
          x.version = info.version;
          x.configFile = info.configFile;
          x.additionalIni = info.additionalIni;
          x.xdebug = info.xdebug;
          x.extensions = info.extensions;
        }),
      );
    } catch (e) {
      setErrors(id, String(e).replace(/^Error: /, ""));
    } finally {
      setReloading(null);
    }
  };

  const add = (i: PhpInterpreter, probe = true) => {
    const item = { ...i, id: newInterpreterId(), name: uniqueName(i.name || "PHP", list) };
    setList(list.length, item);
    setCurrent(item.id);
    setAddMenu(false);
    if (probe && !item.version) void reload(item.id);
  };
  const addLocal = (path = "", version = "") =>
    add({ ...emptyInterpreter("local"), path, version, name: version ? `PHP ${version.split(".").slice(0, 2).join(".")}` : "PHP" }, !version);
  const remove = () => {
    const i = index();
    if (i < 0) return;
    setList((l) => l.filter((_, j) => j !== i));
    setCurrent(list[Math.min(i, list.length - 1)]?.id ?? null);
  };
  const copy = () => {
    const s = sel();
    if (s) add({ ...clone(unwrap(s)), name: `${s.name} (copy)` }, false);
  };
  const ok = () => {
    const names = new Set<string>();
    for (const i of list) {
      const problem = !i.name.trim() ? "The name must not be empty" : names.has(i.name.trim()) ? `The name "${i.name}" is used twice` : null;
      if (problem) {
        setCurrent(i.id);
        setErrors(i.id, problem);
        return;
      }
      names.add(i.name.trim());
    }
    props.onOk(clone(unwrap(list)), sel()?.name ?? null);
  };

  return (
    <>
      <Modal
        title={t("langs.php.cliInterpreters")}
        width={860}
        class="php-interp-main"
        testId="php-interpreters-dialog"
        onClose={props.onClose}
        onOk={ok}
        buttons={
          <>
            <button class="btn-secondary" onClick={props.onClose}>
              {t("common.cancel")}
            </button>
            <button class="btn-primary" onClick={ok}>
              OK
            </button>
          </>
        }
      >
        <div class="php-interp-body">
          <div class="php-interp-list">
            <div class="php-interp-toolbar">
              <div class="php-interp-add">
                <button class="icon-btn" title={t("common.add")} data-testid="php-interp-add" onClick={() => setAddMenu(!addMenu())}>
                  <Plus size={14} />
                </button>
                <Show when={addMenu()}>
                  <div class="php-interp-menu" data-testid="php-interp-add-menu" onMouseLeave={() => setAddMenu(false)}>
                    <button onClick={() => addLocal()}>
                      <Terminal size={13} /> {t("langs.php.localPathToInterpreterEllipsis")}
                    </button>
                    <button
                      onClick={() => {
                        setAddMenu(false);
                        setRemote(true);
                      }}
                    >
                      <Box size={13} /> {t("langs.php.fromDockerVagrantVmWslEllipsis")}
                    </button>
                    <Show when={(detected() ?? []).length}>
                      <div class="php-interp-menu-sep">{t("common.detected")}</div>
                      <For each={detected()}>
                        {(d) => (
                          <button title={d.path} onClick={() => addLocal(d.path, d.version)}>
                            <Terminal size={13} /> {d.path} <span class="php-hint">PHP {d.version}</span>
                          </button>
                        )}
                      </For>
                    </Show>
                  </div>
                </Show>
              </div>
              <button class="icon-btn" title={t("common.remove")} disabled={!sel()} onClick={remove}>
                <Minus size={14} />
              </button>
              <button class="icon-btn" title={t("common.copy")} disabled={!sel()} onClick={copy}>
                <Copy size={13} />
              </button>
            </div>
            <div
              class="php-interp-items"
              role="listbox"
              aria-label={t("langs.php.cliInterpreters")}
              onKeyDown={(e) => listboxKeyDown(e, list.map((x) => x.id), current(), setCurrent)}
            >
              <For each={list} fallback={<div class="php-hint php-interp-empty">{t("langs.php.noInterpretersConfiguredClickTo")}</div>}>
                {(i) => (
                  <div
                    class="tree-row php-interp-row"
                    classList={{ selected: i.id === current() }}
                    role="option"
                    aria-selected={i.id === current()}
                    tabIndex={optionTabIndex(i.id, current(), list[0]?.id)}
                    data-interpreter={i.name}
                    data-option-id={i.id}
                    onClick={() => setCurrent(i.id)}
                    onFocus={() => setCurrent(i.id)}
                  >
                    <span class="php-interp-icon">{KIND_ICONS[i.kind]({ size: 13 })}</span>
                    <span class="php-interp-name">{i.name}</span>
                    <Show when={i.version}>
                      <span class="php-hint">{i.version}</span>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </div>
          <div class="php-interp-form">
            <Show when={sel()} fallback={<div class="php-hint">{t("langs.php.addALocalOrRemote")}</div>}>
              {(s) => (
                <InterpreterForm
                  interp={s()}
                  edit={edit}
                  error={errors[s().id]}
                  reloading={reloading() === s().id}
                  onReload={() => void reload(s().id)}
                  onPhpinfo={() => {
                    const target = clone(unwrap(s()));
                    setPhpinfo({ name: target.name, text: null });
                    interpretersApi
                      .phpinfo(target)
                      .then((text) => setPhpinfo({ name: target.name, text }))
                      .catch((e) => setPhpinfo({ name: target.name, text: "", error: String(e).replace(/^Error: /, "") }));
                  }}
                />
              )}
            </Show>
          </div>
        </div>
      </Modal>
      <Show when={remote()}>
        <RemoteInterpreterDialog onClose={() => setRemote(false)} onOk={(i) => (setRemote(false), add(i))} />
      </Show>
      <Show when={phpinfo()}>
        {(p) => (
          <Modal
            title={`phpinfo: ${p().name}`}
            width={760}
            testId="php-phpinfo-dialog"
            onClose={() => setPhpinfo(null)}
            buttons={
              <button class="btn-primary" onClick={() => setPhpinfo(null)}>
                {t("common.close")}
              </button>
            }
          >
            <div class="dialog-body">
              <Show when={p().text !== null} fallback={<div class="php-hint">{t("langs.php.runningPhpIEllipsis")}</div>}>
                <Show when={p().error}>
                  <div class="dialog-error">{p().error}</div>
                </Show>
                <pre class="php-phpinfo">{p().text}</pre>
              </Show>
            </div>
          </Modal>
        )}
      </Show>
    </>
  );
}

function Row(props: { label: string; children: JSX.Element; for?: string }) {
  return (
    <div class="php-interp-field">
      <label for={props.for}>{props.label}</label>
      <div class="php-interp-value">{props.children}</div>
    </div>
  );
}

function InterpreterForm(props: {
  interp: PhpInterpreter;
  edit: (fn: (i: PhpInterpreter) => void) => void;
  error?: string;
  reloading: boolean;
  onReload: () => void;
  onPhpinfo: () => void;
}) {
  const i = () => props.interp;
  const remote = () => i().kind !== "local";
  // The mappings Forge derives (Docker's /opt/project, compose volumes, WSL drives).
  const [automatic, setAutomatic] = createSignal<PathMapping[]>([]);
  createEffect(
    on(
      () => [i().id, i().kind, i().image, i().composeFiles.join("\n"), i().service, i().distribution],
      () => {
        if (!remote()) return setAutomatic([]);
        const target = clone(unwrap(i()));
        interpretersApi
          .automaticMappings(target)
          .then(setAutomatic)
          .catch(() => setAutomatic([]));
      },
    ),
  );
  const [ssh] = createResource(() => (i().kind === "ssh" ? remoteApi.configs().catch((): SshConfigView[] => []) : Promise.resolve([] as SshConfigView[])));
  return (
    <div class="php-interp-fields">
      <Row label={t("common.nameLabel")} for="php-interp-name">
        <input id="php-interp-name" type="text" value={i().name} onInput={(e) => props.edit((x) => (x.name = e.currentTarget.value))} />
      </Row>
      <label class="php-interp-check">
        <input type="checkbox" checked={i().project} onChange={(e) => props.edit((x) => (x.project = e.currentTarget.checked))} />
        {t("langs.php.visibleOnlyForThisProject")}
      </label>

      <Show when={remote()}>
        <div class="php-interp-section">{KIND_LABELS[i().kind]}</div>
        <Switch>
          <Match when={i().kind === "docker"}>
            <Row label={t("langs.php.serverLabel")}>
              <input type="text" value={i().dockerServer || "Docker"} onChange={(e) => props.edit((x) => (x.dockerServer = e.currentTarget.value.trim() === "Docker" ? "" : e.currentTarget.value.trim()))} />
            </Row>
            <Row label={t("langs.php.imageNameLabel")}>
              <input type="text" value={i().image} placeholder="php:8.3-cli" onInput={(e) => props.edit((x) => (x.image = e.currentTarget.value))} />
            </Row>
          </Match>
          <Match when={i().kind === "compose"}>
            <Row label={t("langs.php.serverLabel")}>
              <input type="text" value={i().dockerServer || "Docker"} onChange={(e) => props.edit((x) => (x.dockerServer = e.currentTarget.value.trim() === "Docker" ? "" : e.currentTarget.value.trim()))} />
            </Row>
            <Row label={t("langs.php.configurationFilesLabel")}>
              <input
                type="text"
                value={i().composeFiles.join("; ")}
                placeholder="./docker-compose.yml"
                onChange={(e) => props.edit((x) => (x.composeFiles = e.currentTarget.value.split(";").map((f) => f.trim()).filter(Boolean)))}
              />
            </Row>
            <Row label={t("langs.php.serviceLabel")}>
              <input type="text" value={i().service} placeholder="app" onInput={(e) => props.edit((x) => (x.service = e.currentTarget.value))} />
            </Row>
            <Row label={t("langs.php.lifecycleLabel")}>
              <div class="php-interp-radios">
                <label>
                  <input type="radio" name="php-lifecycle" checked={i().lifecycle === "run"} onChange={() => props.edit((x) => (x.lifecycle = "run"))} />
                  {t("langs.php.alwaysStartANewContainer")}
                </label>
                <label>
                  <input type="radio" name="php-lifecycle" checked={i().lifecycle === "exec"} onChange={() => props.edit((x) => (x.lifecycle = "exec"))} />
                  {t("langs.php.connectToExistingContainerDocker")}
                </label>
              </div>
            </Row>
          </Match>
          <Match when={i().kind === "ssh"}>
            <Row label={t("common.sshConfigurationLabel")}>
              <select value={i().sshConfig} onChange={(e) => props.edit((x) => (x.sshConfig = e.currentTarget.value))}>
                <option value="">{t("langs.php.selectAnSshConfiguration")}</option>
                <For each={ssh() ?? []}>{(c) => <option value={c.id}>{c.name || c.label}</option>}</For>
              </select>
            </Row>
          </Match>
          <Match when={i().kind === "wsl"}>
            <Row label={t("langs.php.distributionLabel")}>
              <input type="text" value={i().distribution} placeholder={t("langs.php.defaultDistribution")} onInput={(e) => props.edit((x) => (x.distribution = e.currentTarget.value))} />
            </Row>
          </Match>
        </Switch>
      </Show>

      <div class="php-interp-section">{t("langs.php.general")}</div>
      <Row label={t("langs.php.phpExecutableLabel")} for="php-interp-path">
        <div class="php-interp-path">
          <input
            id="php-interp-path"
            type="text"
            value={i().path}
            placeholder={remote() ? "php" : "/usr/bin/php"}
            onInput={(e) => props.edit((x) => (x.path = e.currentTarget.value))}
          />
          <Show when={!remote()}>
            <button
              class="icon-btn"
              title={t("langs.php.browseForThePhpExecutable")}
              onClick={async () => {
                const p = await browseFile();
                if (p) props.edit((x) => (x.path = p));
              }}
            >
              <FolderOpen size={14} />
            </button>
          </Show>
          <button class="icon-btn" title={t("langs.php.reloadPhpinfo")} data-testid="php-interp-reload" disabled={props.reloading} onClick={props.onReload}>
            <RefreshCw size={14} class={props.reloading ? "php-spin" : undefined} />
          </button>
        </div>
      </Row>
      <Row label={t("langs.php.phpVersionLabel")}>
        <span class="php-interp-info" data-testid="php-interp-version">
          {props.reloading ? t("langs.php.detectingEllipsis") : i().version ? i().version : t("langs.php.notDetectedClickReload")}
        </span>
      </Row>
      <Row label={t("langs.php.configurationFileLabel")}>
        <span class="php-interp-info php-interp-mono">{i().configFile || (i().version ? t("common.notFound2") : "")}</span>
      </Row>
      <Show when={i().additionalIni}>
        <Row label={t("langs.php.additionalIniFilesLabel")}>
          <span class="php-interp-info php-interp-mono php-interp-wrap">{i().additionalIni}</span>
        </Row>
      </Show>
      <Row label={t("langs.php.debuggerLabel")}>
        <span class="php-interp-info">{i().xdebug ? `Xdebug ${i().xdebug === "unknown" ? "" : i().xdebug}`.trim() : i().version ? t("common.notInstalled") : ""}</span>
      </Row>
      <Row label={t("langs.php.debuggerExtensionLabel")}>
        <input
          type="text"
          value={i().debuggerExtension}
          placeholder={remote() ? "/usr/local/lib/php/extensions/xdebug.so" : "/usr/lib/php/20230831/xdebug.so"}
          onInput={(e) => props.edit((x) => (x.debuggerExtension = e.currentTarget.value))}
        />
      </Row>
      <Show when={props.error}>
        <div class="dialog-error" data-testid="php-interp-error">
          {props.error}
        </div>
      </Show>

      <Show when={remote()}>
        <div class="php-interp-section">{t("langs.php.pathMappings")}</div>
        <div class="php-interp-table" data-testid="php-interp-mappings">
          <div class="php-interp-thead">
            <span>{t("langs.php.localPath")}</span>
            <span>{t("langs.php.remotePath")}</span>
            <span />
          </div>
          <For each={automatic()}>
            {(m) => (
              <div class="php-interp-trow auto" title={t("langs.php.derivedFromTheInterpreter")}>
                <span class="php-interp-mono">{m.local || t("langs.php.projectRoot")}</span>
                <span class="php-interp-mono">{m.remote}</span>
                <span class="php-hint">auto</span>
              </div>
            )}
          </For>
          <For each={i().mappings}>
            {(m, n) => (
              <div class="php-interp-trow">
                <input type="text" value={m.local} placeholder={t("langs.php.projectRoot")} onChange={(e) => props.edit((x) => (x.mappings[n()].local = e.currentTarget.value.trim()))} />
                <input type="text" value={m.remote} placeholder="/var/www/html" onChange={(e) => props.edit((x) => (x.mappings[n()].remote = e.currentTarget.value.trim()))} />
                <button class="icon-btn" title={t("langs.php.removeMapping")} onClick={() => props.edit((x) => x.mappings.splice(n(), 1))}>
                  <Minus size={13} />
                </button>
              </div>
            )}
          </For>
          <button class="php-interp-link" onClick={() => props.edit((x) => x.mappings.push({ local: "", remote: "" }))}>
            <Plus size={12} /> {t("langs.php.addMapping")}
          </button>
        </div>
      </Show>

      <div class="php-interp-section">{t("langs.php.additional")}</div>
      <div class="php-interp-table" data-testid="php-interp-options">
        <div class="php-interp-thead">
          <span>{t("langs.php.configurationDirective")}</span>
          <span>{t("common.value")}</span>
          <span />
        </div>
        <For each={i().options} fallback={<div class="php-hint php-interp-none">{t("langs.php.noConfigurationOptionsPassedAs")}</div>}>
          {(o, n) => (
            <div class="php-interp-trow">
              <input type="text" value={o.name} placeholder="memory_limit" onChange={(e) => props.edit((x) => (x.options[n()].name = e.currentTarget.value.trim()))} />
              <input type="text" value={o.value} placeholder="-1" onChange={(e) => props.edit((x) => (x.options[n()].value = e.currentTarget.value.trim()))} />
              <button class="icon-btn" title={t("common.remove")} onClick={() => props.edit((x) => x.options.splice(n(), 1))}>
                <Minus size={13} />
              </button>
            </div>
          )}
        </For>
        <button class="php-interp-link" onClick={() => props.edit((x) => x.options.push({ name: "", value: "" }))}>
          <Plus size={12} /> {t("langs.php.addOption")}
        </button>
      </div>
      <div class="php-interp-actions">
        <button class="btn-secondary" data-testid="php-interp-phpinfo" onClick={props.onPhpinfo}>
          {t("langs.php.showPhpinfo")}
        </button>
        <span class="php-hint">{interpreterDetail(i())}</span>
      </div>
    </div>
  );
}

/** "Configure Remote PHP Interpreter": Docker, Docker Compose, SSH or WSL. */
export function RemoteInterpreterDialog(props: { onClose: () => void; onOk: (i: PhpInterpreter) => void }) {
  const windows = () => chrome()?.os === "windows" || /Windows/.test(typeof navigator === "undefined" ? "" : navigator.userAgent);
  const [kind, setKind] = createSignal<InterpreterKind>("docker");
  const [draft, setDraft] = createStore<PhpInterpreter>(emptyInterpreter("docker"));
  const [contexts] = createResource(() => servicesApi.dockerContexts().catch(() => []));
  const [ssh] = createResource(() => remoteApi.configs().catch((): SshConfigView[] => []));
  const [services] = createResource(
    () => (kind() === "compose" ? draft.composeFiles.join("\n") || " " : null),
    () => interpretersApi.composeServices([...draft.composeFiles]).catch((): string[] => []),
  );
  const [distributions] = createResource(() => interpretersApi.wslDistributions().catch((): string[] => []));
  createEffect(() => {
    const first = services()?.[0];
    if (kind() === "compose" && first && !draft.service) setDraft("service", first);
  });
  createEffect(() => {
    const first = ssh()?.[0];
    if (kind() === "ssh" && first && !draft.sshConfig) setDraft("sshConfig", first.id);
  });
  const valid = () =>
    kind() === "docker" ? !!draft.image.trim() : kind() === "compose" ? !!draft.service.trim() : kind() === "ssh" ? !!draft.sshConfig : true;
  const nameFor = (): string => {
    switch (kind()) {
      case "docker":
        return `${draft.image.trim()} (Docker)`;
      case "compose":
        return `${draft.service.trim()} (Docker Compose)`;
      case "ssh": {
        const c = ssh()?.find((x) => x.id === draft.sshConfig);
        return `${c?.name || c?.label || "SSH"} (SSH)`;
      }
      default:
        return `${draft.distribution.trim() || "WSL"} (WSL)`;
    }
  };
  const ok = () => {
    if (!valid()) return;
    props.onOk({ ...clone(unwrap(draft)), kind: kind(), name: nameFor() });
  };
  const kinds: { id: InterpreterKind; label: string }[] = [
    { id: "docker", label: "Docker" },
    { id: "compose", label: t("langs.php.dockerCompose") },
    { id: "ssh", label: "SSH" },
    { id: "wsl", label: "WSL" },
  ];
  return (
    <Modal
      title={t("langs.php.configureRemotePhpInterpreter")}
      width={560}
      testId="php-remote-interpreter-dialog"
      onClose={props.onClose}
      onOk={ok}
      buttons={
        <>
          <button class="btn-secondary" onClick={props.onClose}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!valid()} onClick={ok}>
            OK
          </button>
        </>
      }
    >
      <div class="dialog-body php-interp-fields">
        <div class="php-interp-kinds">
          <For each={kinds}>
            {(k) => (
              <label classList={{ disabled: k.id === "wsl" && !windows() }} title={k.id === "wsl" && !windows() ? t("langs.php.wslIsAvailableOnWindows") : undefined}>
                <input type="radio" name="php-remote-kind" checked={kind() === k.id} disabled={k.id === "wsl" && !windows()} onChange={() => setKind(k.id)} />
                {k.label}
              </label>
            )}
          </For>
        </div>
        <Switch>
          <Match when={kind() === "docker" || kind() === "compose"}>
            <Row label={t("langs.php.serverLabel")}>
              <select value={draft.dockerServer} onChange={(e) => setDraft("dockerServer", e.currentTarget.value)}>
                <option value="">Docker</option>
                <For each={(contexts() ?? []).filter((c) => c.name !== "default")}>{(c) => <option value={c.name}>{c.name}</option>}</For>
              </select>
            </Row>
            <Show
              when={kind() === "compose"}
              fallback={
                <Row label={t("langs.php.imageNameLabel")} for="php-remote-image">
                  <input id="php-remote-image" type="text" value={draft.image} placeholder="php:8.3-cli" onInput={(e) => setDraft("image", e.currentTarget.value)} />
                </Row>
              }
            >
              <Row label={t("langs.php.configurationFilesLabel")}>
                <input
                  type="text"
                  value={draft.composeFiles.join("; ")}
                  placeholder="./docker-compose.yml"
                  onChange={(e) =>
                    setDraft(
                      "composeFiles",
                      e.currentTarget.value
                        .split(";")
                        .map((f) => f.trim())
                        .filter(Boolean),
                    )
                  }
                />
              </Row>
              <Row label={t("langs.php.serviceLabel")}>
                <select value={draft.service} onChange={(e) => setDraft("service", e.currentTarget.value)}>
                  <Show when={!(services() ?? []).length}>
                    <option value="">{t("langs.php.noServicesFound")}</option>
                  </Show>
                  <For each={services() ?? []}>{(s) => <option value={s}>{s}</option>}</For>
                </select>
              </Row>
              <Row label={t("langs.php.lifecycleLabel")}>
                <div class="php-interp-radios">
                  <label>
                    <input type="radio" name="php-remote-lifecycle" checked={draft.lifecycle === "run"} onChange={() => setDraft("lifecycle", "run")} />
                    {t("langs.php.alwaysStartANewContainer")}
                  </label>
                  <label>
                    <input type="radio" name="php-remote-lifecycle" checked={draft.lifecycle === "exec"} onChange={() => setDraft("lifecycle", "exec")} />
                    {t("langs.php.connectToExistingContainerDocker")}
                  </label>
                </div>
              </Row>
            </Show>
          </Match>
          <Match when={kind() === "ssh"}>
            <Row label={t("common.sshConfigurationLabel")}>
              <select value={draft.sshConfig} onChange={(e) => setDraft("sshConfig", e.currentTarget.value)}>
                <Show when={!(ssh() ?? []).length}>
                  <option value="">{t("langs.php.noSshConfigurationsSettingsTools")}</option>
                </Show>
                <For each={ssh() ?? []}>{(c) => <option value={c.id}>{c.name || c.label}</option>}</For>
              </select>
            </Row>
          </Match>
          <Match when={kind() === "wsl"}>
            <Row label={t("langs.php.linuxDistributionLabel")}>
              <select value={draft.distribution} onChange={(e) => setDraft("distribution", e.currentTarget.value)}>
                <option value="">{t("common.default")}</option>
                <For each={distributions() ?? []}>{(d) => <option value={d}>{d}</option>}</For>
              </select>
            </Row>
          </Match>
        </Switch>
        <Row label={t("langs.php.phpInterpreterPathLabel")} for="php-remote-path">
          <input id="php-remote-path" type="text" value={draft.path} placeholder="php" onInput={(e) => setDraft("path", e.currentTarget.value)} />
        </Row>
      </div>
    </Modal>
  );
}
