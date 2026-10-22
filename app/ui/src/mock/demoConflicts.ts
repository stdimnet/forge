/**
 * The merge conflicts of the demo repository: merging `feature/checkout-flow` into the
 * current branch (or opening the demo with `?conflicts`) stops with
 * - `app/Services/PriceCalculator.php`: changed on both sides (non-conflicting changes on
 *   each side, a simple conflict, real conflicts),
 * - `app/Reports/SalesReport.php`: modified here, deleted on the branch,
 * - `public/images/logo.png`: a binary file changed on both sides.
 */
import { demoFiles } from "./demoProject";

export interface DemoConflict {
  base: string | null;
  ours: string | null;
  theirs: string | null;
  binary?: boolean;
}

export const CONFLICT_BRANCH = "feature/checkout-flow";

const swap = (s: string, from: string, to: string) => {
  if (!s.includes(from)) throw new Error(`demoConflicts: snippet not found: ${from.slice(0, 40)}`);
  // A function, so `$'` and `$&` in PHP code are not replacement patterns.
  return s.replace(from, () => to);
};

const edits = (s: string, list: [string, string][]) => list.reduce((acc, [a, b]) => swap(acc, a, b), s);

/** The three versions of every conflicted file. */
export function demoConflicts(): Record<string, DemoConflict> {
  const calc = demoFiles["app/Services/PriceCalculator.php"];
  const usd = "            'USD' => '$' . number_format($amount, 2),\n";
  const ours = swap(calc, usd, `${usd}            'CHF' => 'CHF ' . number_format($amount, 2, '.', "'"),\n`);
  const base = edits(calc, [
    ["LOYALTY_THRESHOLD = 1000.0;", "LOYALTY_THRESHOLD = 500.0;"],
    ["$price = $line->price * $line->quantity;", "$price = $line->price * $line->qty;"],
    ["return $net * 0.05;", "return $net * 0.03;"],
  ]);
  const theirs = edits(base, [
    ["    private const DEFAULT_TAX_RATE = 0.19;\n", "    private const DEFAULT_TAX_RATE = 0.19;\n    private const LOYALTY_RATE = 0.05;\n"],
    ["$price = $line->price * $line->qty;", "$price = $line->netPrice * $line->qty;"],
    ["     * @deprecated Use {@see DiscountPolicy::loyalty()} instead.\n", ""],
    ["return $net * 0.03;", "return $net * self::LOYALTY_RATE;"],
    [usd, `${usd}            'GBP' => '£' . number_format($amount, 2),\n`],
  ]);
  const report = demoFiles["app/Reports/SalesReport.php"];
  return {
    "app/Services/PriceCalculator.php": { base, ours, theirs },
    "app/Reports/SalesReport.php": { base: report, ours: swap(report, "namespace App\\Reports;", "namespace App\\Reports;\n\n// Monthly totals per customer."), theirs: null },
    "public/images/logo.png": { base: "", ours: "", theirs: "", binary: true },
  };
}
