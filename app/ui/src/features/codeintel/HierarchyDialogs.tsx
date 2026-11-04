/**
 * The dialogs of Extract Interface, Extract Superclass, Pull Members Up and Push Members
 * Down (`hierarchyRefactorings.ts`), worded like PHPStorm: the new type's name, namespace
 * and directory (PSR-4), the target class, and the member table with "Make abstract" /
 * "Keep abstract" and the members the chosen ones use highlighted.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import type { ClassMember, HierarchyInfo } from "@/ipc/refactor";
import { Buttons, Check, Frame, NamespaceFields, focusSelect, keys, validNamespace } from "./MoveDialogs";
import {
  abstractBox,
  canChoose,
  dependencies,
  hierarchyDialog,
  initialChoice,
  suggestedName,
  validateTypeName,
  type ExtractInterfaceRequest,
  type ExtractSuperclassRequest,
  type HierarchyDialog,
  type PullUpRequest,
  type PushDownRequest,
  type TableMode,
} from "./hierarchyRefactorings";
import { t as tr } from "@/i18n";

const KIND_LETTER: Record<ClassMember["kind"], string> = { method: "m", property: "f", constant: "c" };

function toggle(set: Set<string>, id: string, on: boolean): Set<string> {
  const next = new Set(set);
  if (on) next.add(id);
  else next.delete(id);
  return next;
}

/** The members of the class with check boxes (and the abstract column). */
function MemberTable(props: {
  info: HierarchyInfo;
  mode: TableMode;
  legend: string;
  abstractLabel?: string;
  chosen: Set<string>;
  abstract: Set<string>;
  onChosen: (s: Set<string>) => void;
  onAbstract: (s: Set<string>) => void;
}) {
  const members = () => props.info.members.filter((m) => canChoose(m, props.mode));
  const deps = createMemo(() => dependencies(props.info.members, props.chosen));
  const withAbstract = () => !!props.abstractLabel && members().some((m) => abstractBox(m, props.mode, props.info.kind));
  return (
    <fieldset class="refactor-group">
      <legend>{props.legend}</legend>
      <div class="hierarchy-members" classList={{ "with-abstract": withAbstract() }} role="grid" aria-label={props.legend}>
        <div class="hierarchy-member head" role="row">
          <span />
          <span>{tr("codeintel.hierarchyDialogs.member")}</span>
          <Show when={withAbstract()}>
            <span class="hierarchy-abstract-head">{props.abstractLabel}</span>
          </Show>
        </div>
        <For each={members()} fallback={<div class="refactor-note hierarchy-empty">{tr("codeintel.hierarchyDialogs.noMembers")}</div>}>
          {(m) => {
            const box = () => abstractBox(m, props.mode, props.info.kind);
            return (
              <label
                class="hierarchy-member"
                role="row"
                classList={{ dependency: deps().has(m.id) }}
                title={deps().has(m.id) ? tr("codeintel.hierarchyDialogs.usedByTheSelectedMembers") : undefined}
              >
                <input
                  type="checkbox"
                  checked={props.chosen.has(m.id)}
                  aria-label={m.id}
                  onChange={(e) => props.onChosen(toggle(props.chosen, m.id, e.currentTarget.checked))}
                />
                <span class="hierarchy-member-name">
                  <span class={`move-member-kind ${m.kind}`}>{KIND_LETTER[m.kind]}</span>
                  <code class="move-member-signature">{m.signature}</code>
                </span>
                <Show when={withAbstract()}>
                  <span class="hierarchy-abstract">
                    <Show when={box()}>
                      {(b) => (
                        <input
                          type="checkbox"
                          aria-label={`${props.abstractLabel} ${m.id}`}
                          checked={b().implied || props.abstract.has(m.id)}
                          disabled={!b().enabled || !props.chosen.has(m.id)}
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => props.onAbstract(toggle(props.abstract, m.id, e.currentTarget.checked))}
                        />
                      )}
                    </Show>
                  </span>
                </Show>
              </label>
            );
          }}
        </For>
      </div>
      <Show when={deps().size}>
        <div class="refactor-note hierarchy-legend">
          <span class="hierarchy-legend-swatch" /> {tr("codeintel.hierarchyDialogs.usedByTheSelectedMembers")}
        </div>
      </Show>
    </fieldset>
  );
}

const choices = (chosen: Set<string>, abstract: Set<string>, info: HierarchyInfo, mode: TableMode) =>
  info.members
    .filter((m) => chosen.has(m.id) && canChoose(m, mode))
    .map((m) => ({ id: m.id, makeAbstract: abstract.has(m.id) && !!abstractBox(m, mode, info.kind)?.enabled }));

/** Name, namespace and directory of a new interface or superclass. */
function NewTypeFields(props: {
  label: string;
  name: string;
  setName: (v: string) => void;
  namespace: string;
  setNamespace: (v: string) => void;
  directory: string;
  setDirectory: (v: string) => void;
}) {
  return (
    <>
      <label class="form-label">{props.label}</label>
      <input ref={focusSelect} class="move-input" value={props.name} spellcheck={false} aria-label={tr("codeintel.hierarchyDialogs.name")} onInput={(e) => props.setName(e.currentTarget.value)} />
      <NamespaceFields
        namespace={props.namespace}
        setNamespace={props.setNamespace}
        directory={props.directory}
        setDirectory={props.setDirectory}
        label={tr("codeintel.hierarchyDialogs.namespace")}
      />
    </>
  );
}

export function ExtractInterfaceDialog(props: { req: ExtractInterfaceRequest }) {
  const info = props.req.info;
  const [name, setName] = createSignal(suggestedName(info, "interface"));
  const [ns, setNs] = createSignal(info.namespace);
  const [dir, setDir] = createSignal(info.directory);
  const [chosen, setChosen] = createSignal(initialChoice(info, "interface"));
  const [replace, setReplace] = createSignal(false);
  const error = () => validateTypeName(name(), info) ?? (validNamespace(ns()) ? null : `'${ns().trim()}' is not a valid namespace name`);
  const close = (preview: boolean | null) =>
    props.req.done(
      preview === null
        ? null
        : {
            name: name().trim(),
            namespace: ns().trim().replace(/^\\/, ""),
            directory: dir().trim(),
            members: choices(chosen(), new Set(), info, "interface").map((c) => c.id),
            replaceReferences: replace(),
            preview,
          },
    );
  const run = () => !error() && close(false);
  return (
    <Frame title={tr("codeintel.hierarchyDialogs.extractInterface")} class="hierarchy-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div class="refactor-note">
          Extract interface from: <code>{info.fqn}</code>
        </div>
        <NewTypeFields label={tr("codeintel.hierarchyDialogs.interfaceName")} name={name()} setName={setName} namespace={ns()} setNamespace={setNs} directory={dir()} setDirectory={setDir} />
        <MemberTable
          info={info}
          mode="interface"
          legend={tr("codeintel.hierarchyDialogs.membersToFormInterface")}
          chosen={chosen()}
          abstract={new Set()}
          onChosen={setChosen}
          onAbstract={() => undefined}
        />
        <Check label={tr("codeintel.hierarchyDialogs.replaceClassReferencesWithInterface")} checked={replace()} onChange={setReplace} />
        <Show when={error()}>
          <div class="dialog-error">{error()}</div>
        </Show>
      </div>
      <Buttons ok="Refactor" disabled={!!error()} preview={() => !error() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

export function ExtractSuperclassDialog(props: { req: ExtractSuperclassRequest }) {
  const info = props.req.info;
  const [name, setName] = createSignal(suggestedName(info, "superclass"));
  const [ns, setNs] = createSignal(info.namespace);
  const [dir, setDir] = createSignal(info.directory);
  const [chosen, setChosen] = createSignal(initialChoice(info, "superclass"));
  const [abstract, setAbstract] = createSignal(new Set<string>());
  const error = () => validateTypeName(name(), info) ?? (validNamespace(ns()) ? null : `'${ns().trim()}' is not a valid namespace name`);
  const close = (preview: boolean | null) =>
    props.req.done(
      preview === null
        ? null
        : {
            name: name().trim(),
            namespace: ns().trim().replace(/^\\/, ""),
            directory: dir().trim(),
            members: choices(chosen(), abstract(), info, "superclass"),
            preview,
          },
    );
  const run = () => !error() && close(false);
  return (
    <Frame title={tr("codeintel.hierarchyDialogs.extractSuperclass")} class="hierarchy-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div class="refactor-note">
          Extract superclass from: <code>{info.fqn}</code>
          <Show when={info.parent}> (between it and {info.parent})</Show>
        </div>
        <NewTypeFields label={tr("codeintel.hierarchyDialogs.superclassName")} name={name()} setName={setName} namespace={ns()} setNamespace={setNs} directory={dir()} setDirectory={setDir} />
        <MemberTable
          info={info}
          mode="superclass"
          legend={tr("codeintel.hierarchyDialogs.membersToFormSuperclass")}
          abstractLabel="Make abstract"
          chosen={chosen()}
          abstract={abstract()}
          onChosen={setChosen}
          onAbstract={setAbstract}
        />
        <Show when={error()}>
          <div class="dialog-error">{error()}</div>
        </Show>
      </div>
      <Buttons ok="Refactor" disabled={!!error()} preview={() => !error() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

export function PullUpDialog(props: { req: PullUpRequest }) {
  const info = props.req.info;
  const [target, setTarget] = createSignal(info.targets[0]?.fqn ?? "");
  const mode = (): TableMode => (info.targets.find((t) => t.fqn === target())?.kind === "interface" ? "pullUpInterface" : "pullUpClass");
  const [chosen, setChosen] = createSignal(initialChoice(info, "pullUpClass"));
  const [abstract, setAbstract] = createSignal(new Set<string>());
  const picked = () => choices(chosen(), abstract(), info, mode());
  const valid = () => !!target() && picked().length > 0;
  const close = (preview: boolean | null) => props.req.done(preview === null ? null : { target: target(), members: picked(), preview });
  const run = () => valid() && close(false);
  return (
    <Frame title={tr("codeintel.hierarchyDialogs.pullMembersUp")} class="hierarchy-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <label class="form-label">
          Pull up members of <code>{info.fqn}</code> to:
        </label>
        <select class="move-input" value={target()} aria-label={tr("codeintel.hierarchyDialogs.targetClass")} onChange={(e) => setTarget(e.currentTarget.value)}>
          <For each={info.targets}>
            {(t) => (
              <option value={t.fqn}>
                {t.kind === "interface" ? "interface" : "class"} {t.fqn}
              </option>
            )}
          </For>
        </select>
        <MemberTable
          info={info}
          mode={mode()}
          legend={tr("codeintel.hierarchyDialogs.membersToBePulledUp")}
          abstractLabel="Make abstract"
          chosen={chosen()}
          abstract={abstract()}
          onChosen={setChosen}
          onAbstract={setAbstract}
        />
      </div>
      <Buttons ok="Refactor" disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

export function PushDownDialog(props: { req: PushDownRequest }) {
  const info = props.req.info;
  const [chosen, setChosen] = createSignal(initialChoice(info, "pushDown"));
  const [abstract, setAbstract] = createSignal(new Set<string>());
  const picked = () => choices(chosen(), abstract(), info, "pushDown");
  const valid = () => picked().length > 0;
  const close = (preview: boolean | null) => props.req.done(preview === null ? null : { members: picked(), preview });
  const run = () => valid() && close(false);
  return (
    <Frame title={tr("codeintel.hierarchyDialogs.pushMembersDown")} class="hierarchy-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div class="refactor-note">
          Push members from <code>{info.fqn}</code> down to: {info.targets.map((t) => t.name).join(", ")}
        </div>
        <MemberTable
          info={info}
          mode="pushDown"
          legend={tr("codeintel.hierarchyDialogs.membersToBePushedDown")}
          abstractLabel="Keep abstract"
          chosen={chosen()}
          abstract={abstract()}
          onChosen={setChosen}
          onAbstract={setAbstract}
        />
      </div>
      <Buttons ok="Refactor" disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

/** The open hierarchy refactoring dialog. */
export function HierarchyDialogHost() {
  const d = (): HierarchyDialog | null => hierarchyDialog();
  return (
    <Show when={d()} keyed>
      {(x) => {
        switch (x.kind) {
          case "extractInterface":
            return <ExtractInterfaceDialog req={x.req} />;
          case "extractSuperclass":
            return <ExtractSuperclassDialog req={x.req} />;
          case "pullUp":
            return <PullUpDialog req={x.req} />;
          case "pushDown":
            return <PushDownDialog req={x.req} />;
        }
      }}
    </Show>
  );
}
