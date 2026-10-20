/** "Test Connection" with the host key question, and its result line. */
import { createSignal, Match, Show, Switch } from "solid-js";
import { CircleCheck, CircleX, LoaderCircle } from "lucide-solid";
import { connectErrorText, remoteApi, type SshConfiguration, type TestOutcome } from "@/ipc/remote";
import { confirmHostKey } from "./store";

export type TestState = { running: true } | ({ running: false } & TestOutcome) | null;

/** Run a connection test, asking about unknown or changed host keys on the way. */
export async function testConnection(config: SshConfiguration, secret: string | undefined): Promise<TestOutcome> {
  let accept: string | undefined;
  for (;;) {
    const r = await remoteApi.test(config, secret || undefined, accept);
    const e = r.error;
    if (!r.ok && e?.kind === "hostKey" && e.hostKey.status !== "revoked" && e.hostKey.status !== "trusted" && !accept) {
      if (!(await confirmHostKey(e.hostKey))) return r;
      accept = e.hostKey.fingerprint;
      continue;
    }
    return r;
  }
}

export function useConnectionTest() {
  const [state, setState] = createSignal<TestState>(null);
  const run = async (config: SshConfiguration, secret: string) => {
    setState({ running: true });
    try {
      setState({ running: false, ...(await testConnection(config, secret)) });
    } catch (e) {
      setState({ running: false, ok: false, error: { kind: "other", message: String((e as Error)?.message ?? e) } });
    }
  };
  return { state, run, reset: () => setState(null) };
}

export function TestResultLine(props: { state: TestState; host: string }) {
  return (
    <Show when={props.state}>
      {(s) => (
        <div class="ssh-test-result" data-testid="ssh-test-result">
          <Switch>
            <Match when={s().running}>
              <LoaderCircle size={14} class="ssh-spin" />
              <span class="muted">Connecting to {props.host}…</span>
            </Match>
            <Match when={!s().running && (s() as TestOutcome).ok && (s() as TestOutcome)}>
              {(ok) => (
                <>
                  <CircleCheck size={14} class="ssh-ok" />
                  <span>
                    Successfully connected to {props.host}
                    <Show when={ok().system}> ({ok().system})</Show>
                    <Show when={ok().latencyMs != null}> in {ok().latencyMs} ms</Show>
                  </span>
                </>
              )}
            </Match>
            <Match when={!s().running && (s() as TestOutcome).error}>
              {(e) => (
                <>
                  <CircleX size={14} class="ssh-bad" />
                  <span class="ssh-error-text">{connectErrorText(e())}</span>
                </>
              )}
            </Match>
          </Switch>
        </div>
      )}
    </Show>
  );
}
