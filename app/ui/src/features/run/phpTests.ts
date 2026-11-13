/** Run Context Configuration outside a test: the PHP file as a PHP Script configuration. */
import type { RunConfiguration } from "@/ipc/run";

/** Temporary PHP Script configuration for a plain PHP file. */
export function scriptConfiguration(path: string): RunConfiguration {
  return { name: path.split("/").pop() ?? path, type: "php-script", options: { file: path } };
}
