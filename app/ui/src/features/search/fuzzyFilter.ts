/**
 * Speed search for popups and lists: candidates are matched and ranked in Rust
 * (`forge-fuzzy`, the `fuzzy_filter` command). An empty query keeps every candidate in its
 * order without a round trip.
 */
import { createEffect, createSignal, on, type Accessor } from "solid-js";
import { searchApi, type FilterHit, type FilterRequest } from "@/ipc/search";

const everything = (req: FilterRequest): FilterHit[] =>
  req.candidates.map((_, index) => ({ index, text: 0, score: 0, positions: [] }));

/** Reactive fuzzy filter: re-ranks whenever the request changes; stale answers are dropped. */
export function createFuzzyFilter(request: () => FilterRequest): Accessor<FilterHit[]> {
  const [hits, setHits] = createSignal<FilterHit[]>(everything(request()));
  let seq = 0;
  createEffect(
    on(request, (req) => {
      const id = ++seq;
      if (!req.query.trim()) {
        setHits(everything(req));
        return;
      }
      searchApi
        .filter(req)
        .then((res) => id === seq && setHits(res))
        .catch((err) => console.warn("fuzzy filter failed", err));
    }),
  );
  return hits;
}
