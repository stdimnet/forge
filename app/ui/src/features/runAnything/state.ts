import { createSignal } from "solid-js";

const [raOpen, setRaOpen] = createSignal(false);
const [raQuery, setRaQuery] = createSignal("");

export { raOpen, raQuery, setRaQuery };

/** Open Run Anything (double Ctrl) with an empty input; again while open closes it. */
export function toggleRunAnything(query = ""): void {
  if (raOpen()) {
    setRaOpen(false);
    return;
  }
  setRaQuery(query);
  setRaOpen(true);
}

export function closeRunAnything(): void {
  setRaOpen(false);
}
