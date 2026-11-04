import type { PluginContext } from "@forge/plugin-api";

/** Called on the first activation event (here: the first run of "Say Hello"). */
export function activate(context: PluginContext): void {
  context.subscriptions.push(
    forge.commands.register("{{id}}.hello", async () => {
      await forge.window.showMessage("Hello from {{name}}!");
    }),
  );
}
