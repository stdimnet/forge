/** The Extract Component dialog (`extractComponent.ts`): name, directory and the props. */
import { createSignal, Show } from "solid-js";
import { Buttons, Frame, focusSelect, keys } from "./MoveDialogs";
import { componentPath, validateComponentName, type ExtractComponentRequest } from "./extractComponent";

export function ExtractComponentDialog(props: { req: ExtractComponentRequest }) {
  const info = props.req.info;
  const [name, setName] = createSignal(info.name);
  const [dir, setDir] = createSignal(props.req.directory);
  const error = () => validateComponentName(name());
  const close = (preview: boolean | null) =>
    props.req.done(preview === null ? null : { name: name().trim(), directory: dir().trim(), preview });
  const run = () => !error() && close(false);
  return (
    <Frame title="Extract Component" class="hierarchy-dialog extract-component-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div class="refactor-note">
          Extract the selected {info.framework === "vue" ? "template elements into a Vue component" : "JSX into a React component"}
        </div>
        <label class="form-label">Component name:</label>
        <input ref={focusSelect} class="move-input" value={name()} spellcheck={false} aria-label="Component name" onInput={(e) => setName(e.currentTarget.value)} />
        <label class="form-label">Directory:</label>
        <input class="move-input" value={dir()} spellcheck={false} aria-label="Directory" onInput={(e) => setDir(e.currentTarget.value)} />
        <div class="refactor-note">
          New file: <code>{componentPath(dir(), name(), info.extension)}</code>
        </div>
        <fieldset class="refactor-group">
          <legend>Props</legend>
          <Show when={info.props.length} fallback={<div class="refactor-note">The component needs no props.</div>}>
            <div class="extract-component-props">
              {info.props.map((p) => (
                <code class="extract-component-prop">{p}</code>
              ))}
            </div>
          </Show>
          <Show when={info.imports.length}>
            <div class="refactor-note">Imported by the new component: {info.imports.join(", ")}</div>
          </Show>
        </fieldset>
        <Show when={error()}>
          <div class="dialog-error">{error()}</div>
        </Show>
      </div>
      <Buttons ok="Refactor" disabled={!!error()} preview={() => !error() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}
