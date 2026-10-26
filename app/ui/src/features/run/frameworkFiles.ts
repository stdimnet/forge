/**
 * Which files hold tests of the test frameworks plugin manifests declare
 * (`[[contributions.testFrameworks]]`, `testFiles` globs): the editor attaches the test
 * gutter to them. Discovery itself runs in Rust (`test_discover`).
 */
import { createMemo, createRoot } from "solid-js";
import { settingsState } from "@/state/settings";

/** A `testFiles` glob as a regular expression (a glob without `/` matches the file name), like `forge_testing::spec::glob_regex`. */
export function globRegExp(glob: string): RegExp {
  const g = glob.trim().replace(/^\.\//, "");
  let re = g.includes("/") ? "^" : "^(?:.*/)?";
  for (let i = 0; i < g.length; i++) {
    const c = g[i];
    if (c === "*" && g[i + 1] === "*") {
      i++;
      if (g[i + 1] === "/") {
        i++;
        re += "(?:.*/)?";
      } else re += ".*";
    } else if (c === "*") re += "[^/]*";
    else if (c === "?") re += "[^/]";
    else re += c.replace(/[.+()|[\]{}^$\\#&~-]/g, "\\$&");
  }
  return new RegExp(`${re}$`);
}

const enabled = (id: string) => settingsState.plugins.some((p) => p.id === id && p.enabled);

const patterns = createRoot(() =>
  createMemo(() =>
    settingsState.plugins
      .filter((p) => enabled(p.id))
      .flatMap((p) => (p.contributions?.testFrameworks ?? []).flatMap((f) => f.testFiles ?? []))
      .map(globRegExp),
  ),
);

/** Whether a project file matches a manifest test framework's `testFiles`. */
export const isFrameworkTestFile = (path: string) => patterns().some((re) => re.test(path.replace(/\\/g, "/")));
