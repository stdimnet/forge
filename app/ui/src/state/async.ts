/**
 * Helpers for async work owned by a component or a store: subscriptions that resolve after
 * their owner may already be gone, and loads whose replies can arrive out of order.
 */
import { onCleanup } from "solid-js";

/**
 * Unsubscribe with the current owner (a component), also when the subscription resolves only
 * after the owner was cleaned up. Call it synchronously in the component: `onCleanup` after an
 * `await` has no owner and is never run.
 */
export function unsubscribeOnCleanup(subscription: Promise<() => void>): void {
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  onCleanup(() => {
    disposed = true;
    unsubscribe?.();
  });
  subscription.then(
    (un) => (disposed ? un() : (unsubscribe = un)),
    () => undefined,
  );
}

/**
 * A guard for loads whose replies may arrive out of order: `begin()` starts a load and returns
 * a check that stays true only until a newer load begins, so a slow older reply is dropped
 * instead of overwriting the newer state.
 */
export function latestOnly(): { begin: () => () => boolean; invalidate: () => void } {
  let seq = 0;
  return {
    begin: () => {
      const mine = ++seq;
      return () => mine === seq;
    },
    invalidate: () => void ++seq,
  };
}
