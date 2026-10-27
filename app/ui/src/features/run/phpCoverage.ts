/**
 * The Code Coverage options of the PHP test configurations (PHPStorm: the coverage runner of
 * Run with Coverage): the driver (Xdebug or PCOV), Xdebug's path coverage and per-test
 * coverage. Rust builds the command line from them (`phpforge_test_frameworks::provider::coverage_plan`).
 */
import type { RunConfiguration } from "@/ipc/run";
import type { RunConfigField } from "./types";

const driver = (cfg: RunConfiguration) => cfg.options?.coverageDriver ?? "";

/** The coverage fields of a PHP test framework type. */
export function phpCoverageFields(type: "phpunit" | "pest" | "codeception" | "behat" | "phpspec"): RunConfigField[] {
  const fields: RunConfigField[] = [
    {
      key: "coverageDriver",
      label: "Coverage driver",
      kind: "segmented",
      default: "",
      options: [
        { value: "", label: "Auto" },
        { value: "xdebug", label: "Xdebug" },
        { value: "pcov", label: "PCOV" },
      ],
    },
  ];
  if (type === "phpunit" || type === "pest")
    fields.push({ key: "pathCoverage", label: "Branch and path coverage (Xdebug, PHPUnit 10+)", kind: "bool", visible: (c) => driver(c) !== "pcov" });
  if (type === "phpunit" || type === "pest" || type === "codeception") fields.push({ key: "perTestCoverage", label: "Track per-test coverage", kind: "bool" });
  return fields;
}
