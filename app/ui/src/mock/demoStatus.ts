/**
 * What the status bar of each browser-mode demo project shows: its git branch and the state
 * of its CI pipeline. The git and code-host mocks read this so a Lua game or a React
 * storefront does not show the Laravel shop's branch and pipeline. (The Docker widget
 * follows the project type detected in Rust.)
 */
export type DemoId = "laravel" | "remote" | "symfony" | "lua" | "java" | "react" | "vue" | "angular";

export type DemoCi = "success" | "running" | "failed" | "pending";

export interface DemoStatus {
  /** Checked-out branch. */
  branch: string;
  /** Other local branches. */
  branches: string[];
  /** Pipeline of the current branch; `null`: the project has no CI. */
  ci: DemoCi | null;
  /** Jobs of that pipeline. */
  jobs: [string, DemoCi][];
}

/** The demo the page opened (`?project=…`), `laravel` by default. */
export function currentDemo(): DemoId {
  if (typeof location === "undefined") return "laravel";
  const p = new URLSearchParams(location.search).get("project");
  return p === "remote" || p === "symfony" || p === "lua" || p === "java" || p === "react" || p === "vue" || p === "angular" ? p : "laravel";
}

const laravel: DemoStatus = {
  branch: "feature/order-totals",
  branches: ["main", "feature/checkout-flow", "bugfix/login-redirect"],
  ci: "running",
  jobs: [],
};

export const DEMO_STATUS: Record<DemoId, DemoStatus> = {
  laravel,
  remote: laravel,
  symfony: {
    branch: "feature/order-api",
    branches: ["main", "feature/doctrine-migrations"],
    ci: "success",
    jobs: [
      ["lint / php-cs-fixer", "success"],
      ["analyse / phpstan", "success"],
      ["test / phpunit", "success"],
    ],
  },
  java: {
    branch: "feature/low-stock-report",
    branches: ["main", "chore/gradle-8.10"],
    ci: "success",
    jobs: [
      ["build / gradle", "success"],
      ["test / junit", "success"],
    ],
  },
  lua: {
    branch: "main",
    branches: ["feature/fog-of-war"],
    ci: null,
    jobs: [],
  },
  react: {
    branch: "feature/cart-drawer",
    branches: ["main", "chore/vite-6"],
    ci: "success",
    jobs: [
      ["lint / eslint", "success"],
      ["typecheck / tsc", "success"],
      ["test / vitest", "success"],
      ["build / vite", "success"],
    ],
  },
  vue: {
    branch: "develop",
    branches: ["main", "feature/dark-mode"],
    ci: "failed",
    jobs: [
      ["lint / eslint", "success"],
      ["test / vitest", "failed"],
      ["build / vite", "success"],
    ],
  },
  angular: {
    branch: "feature/user-roles",
    branches: ["main"],
    ci: "running",
    jobs: [
      ["lint / ng lint", "success"],
      ["test / karma", "running"],
      ["build / ng build", "pending"],
    ],
  },
};

export const demoStatus = (): DemoStatus => DEMO_STATUS[currentDemo()];
