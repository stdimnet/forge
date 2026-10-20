/** Doctrine entities of the open project. */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { doctrineApi, type DoctrineEntity } from "@/ipc/doctrine";
import { isPluginActive, usesFramework } from "@/state/project";
import { workspace } from "@/state/workspace";

export const doctrineActive = (): boolean => usesFramework("doctrine") && isPluginActive("forge.doctrine");

const [entities, setEntities] = createSignal<DoctrineEntity[]>([]);
const [loading, setLoading] = createSignal(false);
export { entities, loading };

export async function loadEntities(refresh = false): Promise<void> {
  setLoading(true);
  try {
    setEntities(await doctrineApi.entities(refresh));
  } catch (e) {
    console.warn("Doctrine entities failed", e);
  } finally {
    setLoading(false);
  }
}

if (typeof window !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => [workspace()?.root, doctrineActive()] as const,
        ([root, active]) => {
          if (root && active) void loadEntities();
          else setEntities([]);
        },
      ),
    ),
  );
}
