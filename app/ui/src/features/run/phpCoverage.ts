/**
 * The Code Coverage options of the PHP test configurations (PHPStorm: the coverage runner of
 * Run with Coverage): the driver (Xdebug or PCOV), Xdebug's path coverage and per-test
 * coverage. Rust builds the command line from them (`phpforge_test_frameworks::provider::coverage_plan`).
 */
import type { RunConfiguration } from "@/ipc/run";
import type { RunConfigField } from "./types";
import { t } from "@/i18n";

const driver = (cfg: RunConfiguration) => cfg.options?.coverageDriver ?? "";

/** The coverage fields of a PHP test framework type. */
export function phpCoverageFields(type: "phpunit" | "pest" | "codeception" | "behat" | "phpspec"): RunConfigField[] {
  const fields: RunConfigField[] = [
    {
      key: "coverageDriver",
      label: t("run.phpCoverage.coverageDriver"),
      kind: "segmented",
      default: "",
      options: [
        { value: "", label: t("run.phpCoverage.auto") },
        { value: "xdebug", label: "Xdebug" },
        { value: "pcov", label: "PCOV" },
      ],
    },
  ];
  if (type === "phpunit" || type === "pest")
    fields.push({ key: "pathCoverage", label: t("run.phpCoverage.branchAndPathCoverageXdebug"), kind: "bool", visible: (c) => driver(c) !== "pcov" });
  if (type === "phpunit" || type === "pest" || type === "codeception") fields.push({ key: "perTestCoverage", label: t("run.phpCoverage.trackPerTestCoverage"), kind: "bool" });
  return fields;
}
