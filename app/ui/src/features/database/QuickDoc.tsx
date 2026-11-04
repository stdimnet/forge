/**
 * Quick Documentation (Ctrl+Q) of a Database tool window node: its kind, location, comment,
 * properties and DDL, computed by the backend from the model (`db_object_doc`).
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { BookOpen, X } from "lucide-solid";
import { dbApi, type ObjectDoc, type ObjectRef } from "@/ipc/db";
import { showBalloon } from "@/keymap";
import { t } from "@/i18n";

interface Shown {
  x: number;
  y: number;
  doc: ObjectDoc;
}

const [shown, setShown] = createSignal<Shown | null>(null);

/** Show the documentation of a node next to the element `anchor`. */
export async function showQuickDoc(dataSource: string, schema: string, object: ObjectRef, anchor?: Element | null): Promise<void> {
  try {
    const doc = await dbApi.objectDoc(dataSource, schema, object);
    const r = anchor?.getBoundingClientRect();
    setShown({ x: r ? r.left + 24 : 80, y: r ? r.bottom + 4 : 80, doc });
  } catch (e) {
    showBalloon(t("database.quickDoc.noDocumentation"), e instanceof Error ? e.message : String(e));
  }
}

export const closeQuickDoc = () => setShown(null);
export const quickDocOpen = () => shown() !== null;

export function QuickDocPopup() {
  return (
    <Show when={shown()}>
      {(s) => <Popup {...s()} />}
    </Show>
  );
}

function Popup(p: Shown) {
  let el!: HTMLDivElement;
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closeQuickDoc();
  };
  const esc = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      closeQuickDoc();
    }
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    window.addEventListener("keydown", esc, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, window.innerHeight - r.height - 8)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
  });
  onCleanup(() => {
    window.removeEventListener("mousedown", away, true);
    window.removeEventListener("keydown", esc, true);
  });
  return (
    <div ref={el} class="db-doc" style={{ left: `${p.x}px`, top: `${p.y}px` }} data-testid="db-quick-doc" role="dialog" aria-label={t("database.quickDoc.quickDocumentation")}>
      <div class="db-doc-head">
        <BookOpen size={14} class="muted" />
        <span class="db-doc-kind">{p.doc.kind}</span>
        <b class="db-doc-name">{p.doc.name}</b>
        <Show when={p.doc.location}>
          <span class="muted">in {p.doc.location}</span>
        </Show>
        <span class="db-doc-spacer" />
        <button class="icon-btn" title={t("database.quickDoc.close", { shortcut: "Escape" })} onClick={closeQuickDoc}>
          <X size={13} />
        </button>
      </div>
      <Show when={p.doc.comment}>
        <div class="db-doc-comment">{p.doc.comment}</div>
      </Show>
      <Show when={p.doc.properties.length}>
        <table class="db-doc-props">
          <tbody>
            <For each={p.doc.properties}>
              {(prop) => (
                <tr>
                  <td class="muted">{prop.name}</td>
                  <td>{prop.value}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
      <pre class="db-doc-ddl">{p.doc.ddl}</pre>
    </div>
  );
}
